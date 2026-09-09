//! Option/Result/Graph/Field as executable .emath declaration types, ops, and laws.

use std::collections::BTreeMap;

use emath_exec_ir::interp::{EvalFault, Value};
use emath_exec_ir::runner::{TestVerdict, eval_definitions_values};
use emath_ir::canonical::canonical_package;
use emath_syntax::parse_str;
use emath_test_harness::{boot, Probe, Source};

/// Build a minimal admitting `function` carrying the given `inputs:` field lines.
fn fn_with_inputs(inputs: &str) -> String {
    format!("emath function probe:\n    inputs:\n{inputs}\n    definitions:\n        t = 1.0\n")
}

/// Every declared input binds to `Int 0` (E-SEC-130 named surface; the evaluator
/// needs a value for every declared input even when no definition reads it).
fn probe_inputs(
    package: &emath_ir::SemanticPackage,
    declaration_index: usize,
) -> BTreeMap<String, Value> {
    package.declarations[declaration_index]
        .inputs
        .iter()
        .map(|field| (field.name.clone(), Value::I64(0)))
        .collect()
}

fn errors_of(result: &emath_sema::CheckResult) -> Vec<String> {
    result
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
}

fn first_output_type(result: &emath_sema::CheckResult) -> Option<emath_ir::TypeNode> {
    let package = &result.package;
    let field = package.declarations.first()?.outputs.first()?;
    package.types.get(field.ty.index()).cloned()
}

fn first_input_type(result: &emath_sema::CheckResult) -> Option<emath_ir::TypeNode> {
    let package = &result.package;
    let field = package.declarations.first()?.inputs.first()?;
    package.types.get(field.ty.index()).cloned()
}

fn input_types(result: &emath_sema::CheckResult) -> Vec<emath_ir::TypeNode> {
    let package = &result.package;
    let declaration = package
        .declarations
        .first()
        .expect("at least one declaration");
    declaration
        .inputs
        .iter()
        .map(|field| {
            package
                .types
                .get(field.ty.index())
                .cloned()
                .expect("declared input carries an admitted semantic type")
        })
        .collect()
}

/// Admit `source`, then evaluate declaration `index` over `bindings`.
fn text_values_at(
    p: &mut Probe,
    name: &str,
    source: &str,
    index: usize,
    bindings: BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let result = Source::from_str(name, source).must_admit(p);
    if result.diagnostics.has_errors() {
        p.fail(format!("{name}:eval"), "cannot evaluate a source that did not admit");
        return BTreeMap::new();
    }
    let Some(declaration) = result.package.declarations.get(index) else {
        p.fail(format!("{name}:eval"), format!("declaration {index} out of bounds"));
        return BTreeMap::new();
    };
    match eval_definitions_values(&result.package, declaration, &bindings, &BTreeMap::new()) {
        Ok(values) => values,
        Err(fault) => {
            p.fail(format!("{name}:eval"), format!("declaration {index} must evaluate: {fault:?}"));
            BTreeMap::new()
        }
    }
}

/// Admit `source`, then evaluate the first declaration over default `Int 0` inputs.
fn text_values(p: &mut Probe, name: &str, source: &str) -> BTreeMap<String, Value> {
    let result = Source::from_str(name, source).must_admit(p);
    if result.diagnostics.has_errors() {
        p.fail(format!("{name}:eval"), "cannot evaluate a source that did not admit");
        return BTreeMap::new();
    }
    let given = probe_inputs(&result.package, 0);
    match eval_definitions_values(
        &result.package,
        &result.package.declarations[0],
        &given,
        &BTreeMap::new(),
    ) {
        Ok(values) => values,
        Err(fault) => {
            p.fail(format!("{name}:eval"), format!("text surface must evaluate: {fault:?}"));
            BTreeMap::new()
        }
    }
}

/// Reachability mask from a checked graph source.
fn reachability_mask(p: &mut Probe, name: &str, source: &str) -> Vec<f64> {
    let values = text_values(p, name, source);
    match values.get("r") {
        Some(Value::Vector(mask)) => mask.clone(),
        other => {
            p.fail(format!("{name}:mask"), format!("reachability must return a vector, got {other:?}"));
            Vec::new()
        }
    }
}

/// `new_mask[P[i]] = old_mask[i]` for a permutation `P` (old vertex to new vertex).
fn permute_mask(p: &[usize], mask: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; mask.len()];
    for (i, &v) in mask.iter().enumerate() {
        out[p[i]] = v;
    }
    out
}

const MAP_COMPOSITION_SOURCE: &str = "emath function consumer:\n    inputs:\n        opt: Option<Float64>\n    outputs:\n        maybe: Option<Float64>\n    definitions:\n        maybe = if option_is_some(opt) : option_some(2.0 * option_unwrap_or(opt, 0.0)) else : option_none()\n";

const RESULT_MAP_SOURCE: &str = "emath function rconsumer:\n    inputs:\n        r: Result<Float64, Float64>\n    outputs:\n        mapped: Result<Float64, Float64>\n        projected: Option<Float64>\n    definitions:\n        mapped = if result_is_ok(r) : result_ok(2.0 * result_unwrap_or(r, 0.0)) else : r\n        projected = result_error_of(r)\n";

#[test]
fn option_result_graph_field_types_ops_and_laws() {
    boot();
    let mut p = Probe::new("Option/Result/Graph/Field admit as executable types with value-carrying ops");
    p.case("admit", |p| {
        let result = Source::from_str("opt-float", &fn_with_inputs("        x: Option<Float64>\n")).must_admit(p);
        p.eq(
            "option-node",
            first_input_type(&result),
            Some(emath_ir::TypeNode::OptionType(Box::new(emath_ir::TypeNode::Float64))),
        );
        let result = Source::from_str("result-int-bool", &fn_with_inputs("        x: Result<Int, Bool>\n")).must_admit(p);
        p.eq(
            "result-node",
            first_input_type(&result),
            Some(emath_ir::TypeNode::Result {
                ok: Box::new(emath_ir::TypeNode::Int),
                error: Box::new(emath_ir::TypeNode::Bool),
            }),
        );
        Source::from_str(
            "graph-output",
            "emath function graph_probe:\n    inputs:\n        n: Int\n\n    outputs:\n        g: Graph\n    definitions:\n        g = graph { 0, 1; 0 --> 1 }\n",
        )
        .must_admit(p);
        let result = Source::from_str(
            "field-output",
            "emath function field_probe:\n    inputs:\n        n: Int\n\n    outputs:\n        f: Field<7>\n    definitions:\n        f = 7\n",
        )
        .must_admit(p);
        p.eq(
            "field-node",
            first_output_type(&result),
            Some(emath_ir::TypeNode::FieldPrime { modulus: 7 }),
        );
        let result = Source::from_str(
            "gf-distinct",
            "emath function gf_probe:\n    inputs:\n        x: Int\n    outputs:\n        v: GF<7>\n    definitions:\n        v = x\n",
        )
        .must_admit(p);
        match first_output_type(&result) {
            Some(node) => {
                p.demand("gf-not-int", !matches!(node, emath_ir::TypeNode::Int), format!("GF<7> must be distinct, not collapsed to Int; got {node:#?}"));
                p.eq("gf-node", node, emath_ir::TypeNode::FieldPrime { modulus: 7 });
            }
            None => {
                p.fail("gf-node", "gf_probe must carry a declared output type");
            }
        }
        let (_tree, diagnostics) = parse_str(
            "emath function t:\n    outputs:\n        a: Option<Float64>\n        b: Result<Int, String>\n        c: Graph\n        d: Field\n        e: GF<7>\n",
        );
        let parse_errors = diagnostics.errors().collect::<Vec<_>>();
        p.demand("syntax-spellings", parse_errors.is_empty(), format!("syntax must parse generic type spellings, got {parse_errors:#?}"));
    });
    p.case("recursion-identity", |p| {
        for (name, inputs) in [
            ("nested-opt", "        x: Option<Option<Int>>\n"),
            ("result-opt", "        x: Result<Int, Option<Float64>>\n"),
            ("opt-result", "        x: Option<Result<Int, Bool>>\n"),
        ] {
            let result = Source::from_str(name, &fn_with_inputs(inputs)).must_admit(p);
            p.demand(format!("{name}-typed"), first_input_type(&result).is_some(), "recursive spelling must carry an admitted TypeNode");
        }
        let result = Source::from_str("nested-structure", &fn_with_inputs("        x: Option<Option<Int>>\n")).must_admit(p);
        p.eq(
            "two-level",
            first_input_type(&result),
            Some(emath_ir::TypeNode::OptionType(Box::new(
                emath_ir::TypeNode::OptionType(Box::new(emath_ir::TypeNode::Int)),
            ))),
        );
        let result = Source::from_str("same-spelling", &fn_with_inputs("        x: Option<Float64>\n        y: Option<Float64>\n")).must_admit(p);
        let nodes = input_types(&result);
        p.eq("two-inputs", nodes.len(), 2);
        if nodes.len() == 2 {
            p.eq("identical-node", nodes[0].clone(), nodes[1].clone());
        }
        let result = Source::from_str("distinct-spellings", &fn_with_inputs("        a: Option<Float64>\n        b: Option<Int>\n        c: Result<Int, Bool>\n")).must_admit(p);
        let nodes = input_types(&result);
        p.eq("three-inputs", nodes.len(), 3);
        if nodes.len() == 3 {
            p.ne("opt-float-vs-int", nodes[0].clone(), nodes[1].clone());
            p.ne("opt-vs-result", nodes[0].clone(), nodes[2].clone());
            p.ne("int-vs-result", nodes[1].clone(), nodes[2].clone());
        }
        Source::from_str("opt-arity", &fn_with_inputs("        x: Option<Int, Float64>\n")).must_refuse(p, &["E-TYPE-010"]);
        Source::from_str("result-under", &fn_with_inputs("        x: Result<Int>\n")).must_refuse(p, &["E-TYPE-010"]);
        Source::from_str("result-over", &fn_with_inputs("        x: Result<Int, Bool, Float64>\n")).must_refuse(p, &["E-TYPE-010"]);
    });
    p.case("graph-alias", |p| {
        let result = Source::from_str("graph-matrix", &fn_with_inputs("        x: Graph\n")).must_admit(p);
        p.eq(
            "matrix-carrier",
            first_input_type(&result),
            Some(emath_ir::TypeNode::Matrix {
                element: Box::new(emath_ir::TypeNode::Float64),
                rows: None,
                cols: None,
            }),
        );
        let values = text_values(p, "reachability", "emath function gp:\n    inputs:\n        n: Int\n\n    outputs:\n        g: Graph\n        r: Vector<Float64>\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        r = reachability(g, 0)\n");
        p.eq("reachable", values.get("r"), Some(&Value::Vector(vec![1.0, 1.0, 1.0, 1.0])));
        let values = text_values(p, "out-degrees", "emath function gp:\n    inputs:\n        n: Int\n\n    outputs:\n        g: Graph\n        d: Vector<Float64>\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        d = out_degrees(g)\n");
        p.eq("degrees", values.get("d"), Some(&Value::Vector(vec![2.0, 1.0, 1.0, 0.0])));
        let result = Source::from_str(
            "matrix-interchange",
            "emath function mp:\n    inputs:\n        n: Int\n\n    outputs:\n        m: Matrix<Float64>\n        r: Vector<Float64>\n        d: Vector<Float64>\n    definitions:\n        m = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        r = reachability(m, 0)\n        d = out_degrees(m)\n",
        )
        .must_admit(p);
        p.eq(
            "matrix-node",
            first_output_type(&result),
            Some(emath_ir::TypeNode::Matrix {
                element: Box::new(emath_ir::TypeNode::Float64),
                rows: None,
                cols: None,
            }),
        );
        let values = text_values(p, "matrix-ops", "emath function mp:\n    inputs:\n        n: Int\n\n    outputs:\n        m: Matrix<Float64>\n        r: Vector<Float64>\n        d: Vector<Float64>\n    definitions:\n        m = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        r = reachability(m, 0)\n        d = out_degrees(m)\n");
        p.eq("via-matrix-reach", values.get("r"), Some(&Value::Vector(vec![1.0, 1.0, 1.0, 1.0])));
        p.eq("via-matrix-deg", values.get("d"), Some(&Value::Vector(vec![2.0, 1.0, 1.0, 0.0])));
        for name in ["graph-arity-1", "graph-arity-2", "graph-arity-3"] {
            let inputs = match name {
                "graph-arity-1" => "        x: Graph<Int>\n",
                "graph-arity-2" => "        x: Graph<Float64>\n",
                _ => "        x: Graph<Int, Float64>\n",
            };
            Source::from_str(name, &fn_with_inputs(inputs)).must_refuse(p, &["E-TYPE-010"]);
        }
    });
    p.case("field-prime", |p| {
        for (name, inputs) in [
            ("field8", "        x: Field<8>\n"),
            ("gf4", "        x: GF<4>\n"),
            ("gf9", "        x: GF<9>\n"),
        ] {
            Source::from_str(name, &fn_with_inputs(inputs)).must_refuse(p, &["E-TYPE-010"]);
        }
        for (name, inputs) in [
            ("gf-int", "        x: GF<Int>\n"),
            ("gf-float", "        x: GF<Float64>\n"),
            ("gf-name", "        x: GF<n>\n"),
        ] {
            Source::from_str(name, &fn_with_inputs(inputs)).must_refuse(p, &["E-TYPE-010"]);
        }
        for (name, inputs) in [
            ("bare-field", "        x: Field\n"),
            ("bare-gf", "        x: GF\n"),
            ("field72", "        x: Field<7, 2>\n"),
            ("gf72", "        x: GF<7, 2>\n"),
        ] {
            Source::from_str(name, &fn_with_inputs(inputs)).must_refuse(p, &["E-TYPE-010"]);
        }
        let result = Source::from_str("prime-identity", &fn_with_inputs("        a: GF<7>\n        b: GF<5>\n        c: Int\n        d: Field<7>\n")).must_admit(p);
        let nodes = input_types(&result);
        p.eq("four-inputs", nodes.len(), 4);
        if nodes.len() == 4 {
            p.ne("prime-identity", nodes[0].clone(), nodes[1].clone());
            p.ne("no-collapse", nodes[0].clone(), nodes[2].clone());
            p.eq("alias-same", nodes[0].clone(), nodes[3].clone());
            p.eq("display-7", nodes[0].display_name(), "Field<7>".to_string());
            p.eq("display-5", nodes[1].display_name(), "Field<5>".to_string());
            p.ne("display-int", nodes[2].display_name(), "Field<7>".to_string());
        }
        Source::from_str("gf2", &fn_with_inputs("        x: GF<2>\n")).must_admit(p);
        for (name, inputs) in [
            ("gf1", "        x: GF<1>\n"),
            ("gf0", "        x: GF<0>\n"),
            ("gf-huge", "        x: GF<2147483648>\n"),
        ] {
            Source::from_str(name, &fn_with_inputs(inputs)).must_refuse(p, &["E-TYPE-010"]);
        }
        let seven = Source::from_str("canon-7", &fn_with_inputs("        x: GF<7>\n")).must_admit(p);
        let five = Source::from_str("canon-5", &fn_with_inputs("        x: GF<5>\n")).must_admit(p);
        let int = Source::from_str("canon-int", &fn_with_inputs("        x: Int\n")).must_admit(p);
        p.ne("canon-primes", canonical_package(&seven.package), canonical_package(&five.package));
        p.ne("canon-int", canonical_package(&seven.package), canonical_package(&int.package));
    });
    p.case("text-builtins", |p| {
        let values = text_values(p, "opt-builtins", "emath function o:\n    definitions:\n        s = option_is_some(option_some(1.0))\n        n = option_is_some(option_none())\n        u1 = option_unwrap_or(option_none(), 9.0)\n        u2 = option_unwrap_or(option_some(2.0), 9.0)\n");
        p.eq("some", values.get("s"), Some(&Value::Bool(true)));
        p.eq("none", values.get("n"), Some(&Value::Bool(false)));
        p.eq("default", values.get("u1"), Some(&Value::F64(9.0)));
        p.eq("payload", values.get("u2"), Some(&Value::F64(2.0)));
        let values = text_values(p, "result-builtins", "emath function r:\n    definitions:\n        ok = result_is_ok(result_ok(3.5))\n        bad = result_is_ok(result_err(7.0))\n        u = result_unwrap_or(result_err(7.0), 9.0)\n        w = result_unwrap_or(result_ok(2.0), 9.0)\n");
        p.eq("ok", values.get("ok"), Some(&Value::Bool(true)));
        p.eq("err", values.get("bad"), Some(&Value::Bool(false)));
        p.eq("err-default", values.get("u"), Some(&Value::F64(9.0)));
        p.eq("ok-payload", values.get("w"), Some(&Value::F64(2.0)));
        let values = text_values(p, "error-of", "emath function e:\n    definitions:\n        r = option_unwrap_or(result_error_of(result_err(7.0)), -1.0)\n        n = option_unwrap_or(result_error_of(result_ok(1.0)), -1.0)\n        tag = option_is_some(result_error_of(result_err(9.0)))\n");
        p.eq("projects", values.get("r"), Some(&Value::F64(7.0)));
        p.eq("ok-none", values.get("n"), Some(&Value::F64(-1.0)));
        p.eq("tag", values.get("tag"), Some(&Value::Bool(true)));
        let values = text_values(p, "mod-inv", "emath function f:\n    definitions:\n        x = field_inv(3, 7)\n        y = mod_inv(3, 7)\n");
        p.demand("field-inv", matches!(values.get("x"), Some(Value::I64(5))), format!("field_inv(3, 7) must be the exact modular inverse 5, got {:?}", values.get("x")));
        p.demand("mod-inv", matches!(values.get("y"), Some(Value::I64(5))), format!("mod_inv(3, 7) must be the exact modular inverse 5, got {:?}", values.get("y")));
        let values = text_values(p, "opt-int-output", "emath function oi:\n    inputs:\n        n: Int\n\n    outputs:\n        o: Option<Int>\n    definitions:\n        o = option_some(5)\n");
        p.demand("carrier", matches!(values.get("o"), Some(Value::Option(Some(payload))) if matches!(&**payload, Value::I64(5) | Value::F64(5.0))), format!("option_some(5) must build an Option<Int> carrier holding 5, got {:?}", values.get("o")));
        let values = text_values(p, "nested-some", "emath function ns:\n    definitions:\n        sso = option_unwrap_or(option_unwrap_or(option_some(option_some(5.0)), option_none()), 9.0)\n        inner_is_some = option_is_some(option_unwrap_or(option_some(option_some(5.0)), option_none()))\n");
        p.eq("flatten", values.get("sso"), Some(&Value::F64(5.0)));
        p.eq("inner-some", values.get("inner_is_some"), Some(&Value::Bool(true)));
        let values = text_values(p, "some-none", "emath function sn:\n    definitions:\n        outer_is_some = option_is_some(option_some(option_none()))\n        inner = option_unwrap_or(option_unwrap_or(option_some(option_none()), option_none()), 42.0)\n");
        p.eq("outer-tag", values.get("outer_is_some"), Some(&Value::Bool(true)));
        p.eq("sentinel", values.get("inner"), Some(&Value::F64(42.0)));
        let values = text_values(p, "no-hidden-zero", "emath function hz:\n    definitions:\n        s = option_is_some(option_some(0.0))\n        u = option_unwrap_or(option_some(0.0), 9.0)\n");
        p.eq("zero-some", values.get("s"), Some(&Value::Bool(true)));
        p.eq("zero-payload", values.get("u"), Some(&Value::F64(0.0)));
    });
    p.case("map-composition", |p| {
        let some = text_values_at(p, "map-some", MAP_COMPOSITION_SOURCE, 0, BTreeMap::from([("opt".to_string(), Value::Option(Some(Box::new(Value::F64(3.0)))))]));
        p.demand("some-doubled", matches!(some.get("maybe"), Some(Value::Option(Some(payload))) if matches!(payload.as_ref(), Value::F64(6.0))), format!("option_map(Some(3), double) must be Some(6), got {:?}", some.get("maybe")));
        let none = text_values_at(p, "map-none", MAP_COMPOSITION_SOURCE, 0, BTreeMap::from([("opt".to_string(), Value::Option(None))]));
        p.demand("none-stays", matches!(none.get("maybe"), Some(Value::Option(None))), format!("option_map(None, double) must be None, got {:?}", none.get("maybe")));
        let ok = text_values_at(p, "map-ok", RESULT_MAP_SOURCE, 0, BTreeMap::from([("r".to_string(), Value::Result { ok: true, payload: Box::new(Value::F64(3.0)) })]));
        p.demand("ok-doubled", matches!(ok.get("mapped"), Some(Value::Result { ok: true, payload }) if matches!(payload.as_ref(), Value::F64(6.0))), format!("result_map(Ok(3), double) must be Ok(6), got {:?}", ok.get("mapped")));
        let err = text_values_at(p, "map-err", RESULT_MAP_SOURCE, 0, BTreeMap::from([("r".to_string(), Value::Result { ok: false, payload: Box::new(Value::F64(7.0)) })]));
        p.demand("err-kept", matches!(err.get("mapped"), Some(Value::Result { ok: false, payload }) if matches!(payload.as_ref(), Value::F64(7.0))), format!("result_map(Err(7), double) must stay Err(7), got {:?}", err.get("mapped")));
        p.demand("err-projected", matches!(err.get("projected"), Some(Value::Option(Some(payload))) if matches!(payload.as_ref(), Value::F64(7.0))), format!("error_of(Err(7)) must be Some(7), got {:?}", err.get("projected")));
    });
    p.case("field-data", |p| {
        for (a, b, want) in [(3i64, 4i64, 0), (6, 5, 4), (5, 5, 3)] {
            let values = text_values_at(p, "field-add", "emath function field7_add:\n    inputs:\n        a: Int\n        b: Int\n    outputs:\n        c: Field<7>\n    definitions:\n        c = int_rem(a + b, 7)\n", 0, BTreeMap::from([("a".into(), Value::I64(a)), ("b".into(), Value::I64(b))]));
            p.eq(format!("add-{a}-{b}"), values.get("c"), Some(&Value::I64(want)));
        }
        for (a, b, want) in [(3i64, 4i64, 5), (3, 5, 1), (5, 5, 4)] {
            let values = text_values_at(p, "field-mul", "emath function field7_mul:\n    inputs:\n        a: Int\n        b: Int\n    outputs:\n        c: Field<7>\n    definitions:\n        c = int_rem(a * b, 7)\n", 0, BTreeMap::from([("a".into(), Value::I64(a)), ("b".into(), Value::I64(b))]));
            p.eq(format!("mul-{a}-{b}"), values.get("c"), Some(&Value::I64(want)));
        }
        for (a, want) in [(3i64, 5), (5, 3)] {
            let values = text_values_at(p, "field-inv", "emath function field7_inv:\n    inputs:\n        a: Int\n    outputs:\n        c: Field<7>\n    definitions:\n        c = field_inv(a, 7)\n", 0, BTreeMap::from([("a".into(), Value::I64(a))]));
            p.eq(format!("inv-{a}"), values.get("c"), Some(&Value::I64(want)));
        }
        let values = text_values_at(p, "int-rem-sign", "emath function irs:\n    inputs:\n        a: Int\n        m: Int\n    outputs:\n        c: Int\n    definitions:\n        c = int_rem(a, m)\n", 0, BTreeMap::from([("a".into(), Value::I64(-1)), ("m".into(), Value::I64(7))]));
        p.eq("euclidean", values.get("c"), Some(&Value::I64(6)));
        let result = Source::from_str("zero-modulus", "emath function iz:\n    inputs:\n        a: Int\n    outputs:\n        c: Int\n    definitions:\n        c = int_rem(a, 0)\n").must_admit(p);
        let err = eval_definitions_values(&result.package, &result.package.declarations[0], &BTreeMap::from([("a".into(), Value::I64(5))]), &BTreeMap::new()).err();
        match err {
            Some(fault) => {
                p.contains("positive-modulus", &fault.to_string(), "modulus must be positive");
            }
            None => {
                p.fail("faults", "int_rem(5, 0) must be a typed fault, not a silent result");
            }
        }
    });
    p.case("refusal-wall", |p| {
        for (name, src, code, frag) in [
            ("gf8", "emath function r:\n    inputs:\n        x: GF<8>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "prime"),
            ("field8", "emath function r:\n    inputs:\n        x: Field<8>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "prime"),
            ("gf-type", "emath function r:\n    inputs:\n        x: GF<Int>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "LITERAL"),
            ("gf-arity", "emath function r:\n    inputs:\n        x: GF<7, 2>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "exactly one"),
            ("gf-overflow", "emath function r:\n    inputs:\n        x: Field<99999999999999999999>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "exceeds the maximum"),
            ("opt-arity", "emath function r:\n    inputs:\n        x: Option<Int, String>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "exactly one"),
            ("result-arity", "emath function r:\n    inputs:\n        x: Result<Int>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "exactly two"),
            ("graph-arity", "emath function r:\n    inputs:\n        x: Graph<Int>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "admits no type arguments"),
        ] {
            let result = Source::from_str(name, src).check();
            let texts = errors_of(&result);
            p.demand(format!("{name}-typed"), texts.iter().any(|e| e.contains(code) && e.contains(frag)), format!("must refuse ({code} / `{frag}`), got {texts:#?}"));
        }
        for (name, src, carrier) in [
            ("some-scalar", "emath function c:\n    definitions:\n        s = option_is_some(5.0)\n", "Option carrier"),
            ("error-of-opt", "emath function c:\n    definitions:\n        e = result_error_of(option_some(1.0))\n", "Result carrier"),
            ("unwrap-kind", "emath function c:\n    definitions:\n        u = result_unwrap_or(result_ok(1.0), option_some(2.0))\n", "kind-matched"),
        ] {
            let result = Source::from_str(name, src).check();
            let texts = errors_of(&result);
            p.demand(format!("{name}-kind"), texts.iter().any(|e| e.contains("E-TYPE-012") && e.contains(carrier)), format!("carrier misuse must refuse (E-TYPE-012 / `{carrier}`), got {texts:#?}"));
        }
        let result = Source::from_str("int-rem-float", "emath function c:\n    inputs:\n        a: Float64\n        m: Int\n    outputs:\n        o: Float64\n    definitions:\n        o = int_rem(a, m)\n").must_admit(p);
        let err = eval_definitions_values(&result.package, &result.package.declarations[0], &BTreeMap::from([("a".into(), Value::F64(5.5)), ("m".into(), Value::I64(2))]), &BTreeMap::new()).err();
        match err {
            Some(TestVerdict::Fault {
                fault: EvalFault::CapabilityRefused { code, .. },
            }) => {
                p.contains("whole-i64", &code, "E-TYPE-012");
            }
            Some(other) => {
                p.fail("whole-i64", format!("expected a typed capability refusal, got {other:?}"));
            }
            None => {
                p.fail("whole-i64", "int_rem(5.5, 2) must be a typed runtime fault, not a silent result");
            }
        }
        let result = Source::from_str("int-rem-arity", "emath function c:\n    definitions:\n        o = int_rem(5)\n").check();
        let texts = errors_of(&result);
        p.demand("arity", texts.iter().any(|e| e.contains("int_rem") && e.contains("argument")), format!("int_rem(5) must refuse as an arity error, got {texts:#?}"));
        let result = Source::from_str("float-into-field", "emath function f:\n    inputs:\n        n: Int\n\n    outputs:\n        c: Field<7>\n    definitions:\n        c = 1.5\n").check();
        let texts = errors_of(&result);
        p.demand("exact-element", texts.iter().any(|e| e.contains("E-TYPE-012") && e.contains("exact integer field element")), format!("float into Field<7> must refuse, got {texts:#?}"));
        let values = text_values(p, "int-rem-field", "emath function f:\n    inputs:\n        n: Int\n\n    outputs:\n        c: Field<7>\n    definitions:\n        c = int_rem(3 + 4, 7)\n");
        p.eq("exact-zero", values.get("c"), Some(&Value::I64(0)));
        text_values(p, "literal-field", "emath function f:\n    inputs:\n        n: Int\n\n    outputs:\n        c: Field<7>\n    definitions:\n        c = 3\n");
    });
    p.case("relabel-law", |p| {
        let original = "emath function gp:\n    inputs:\n        n: Int\n\n    outputs:\n        g: Graph\n        r: Vector<Float64>\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 1 --> 3, 2 --> 3 }\n        r = reachability(g, 0)\n";
        let mask_orig = reachability_mask(p, "orig", original);
        p.eq("orig-mask", mask_orig.clone(), vec![1.0, 1.0, 0.0, 1.0]);
        let relabeled = "emath function gp:\n    inputs:\n        n: Int\n\n    outputs:\n        g: Graph\n        r: Vector<Float64>\n    definitions:\n        g = graph { 0, 1, 2, 3; 1 --> 2, 2 --> 0, 3 --> 0 }\n        r = reachability(g, 1)\n";
        let mask_perm = reachability_mask(p, "relabeled", relabeled);
        p.eq("equivariant", mask_perm.clone(), permute_mask(&[1, 2, 3, 0], &mask_orig));
        p.ne("discriminating", mask_perm, mask_orig);
    });
    p.case("distribution-law", |p| {
        for (a, b, c, want) in [(3i64, 4, 5, 0i64), (1, 6, 2, 0), (5, 5, 3, 2)] {
            let lhs = text_values_at(p, "dist-lhs", "emath function fa:\n    inputs:\n        a: Int\n        b: Int\n        c: Int\n    outputs:\n        l: Field<7>\n    definitions:\n        l = int_rem(int_rem(a + b, 7) * c, 7)\n", 0, BTreeMap::from([("a".into(), Value::I64(a)), ("b".into(), Value::I64(b)), ("c".into(), Value::I64(c))]));
            let rhs = text_values_at(p, "dist-rhs", "emath function fb:\n    inputs:\n        a: Int\n        b: Int\n        c: Int\n    outputs:\n        r: Field<7>\n    definitions:\n        r = int_rem(int_rem(a * c, 7) + int_rem(b * c, 7), 7)\n", 0, BTreeMap::from([("a".into(), Value::I64(a)), ("b".into(), Value::I64(b)), ("c".into(), Value::I64(c))]));
            p.eq(format!("distributes-{a}-{b}-{c}"), lhs.get("l"), rhs.get("r"));
            p.eq(format!("exact-{a}-{b}-{c}"), lhs.get("l"), Some(&Value::I64(want)));
        }
    });
    p.finish();
}
