//! emath-option-result-graph-field-aj8d — Pass 1 (failure-first RED):
//! Option/Result/Graph/Field as EXECUTABLE .emath declaration types.
//!
//! The compute layer for Option/Result already landed (adjacent slice):
//! `Value::Option(Option<Box<Value>>)` + `Value::Result{ok,payload}`
//! with 9 total ops (option-some/none/is-some/unwrap-or,
//! result-ok/err/is-ok/unwrap-or/error-of), proven GREEN in
//! `tests/emath-ir/tests/option_result_values.rs`. Graph compute ops and
//! the Int-backed GF<p> modular op family are complete (masa/rymw).
//!
//! The GAP this pass pins: these are NOT yet executable declaration
//! TYPES. `emath-sema/src/admit/types.rs` `map_type` explicitly REFUSES
//! every one of `Option` / `Result` / `Graph` / `Field` with
//! E_UNSUPPORTED_TYPE ("outside the Phase 1 subset"), and collapses
//! `GF<p>` to a plain `TypeNode::Int` while silently dropping the prime.
//! `TypeNode::{Result, OptionType}` exist but are never reachable from
//! user source, and the strict-f64 rust backend refuses every option op
//! typed.
//!
//! Every test below asserts the CONTRACT that these types should admit
//! with distinct, value-carrying semantics. All are written
//! failure-first against the current tree: they must FAIL (admission
//! refuses without them). Their Red is the evidence the next passes
//! implement against.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::eval_definitions_values;
use emath_sema::{CheckResult, CompilerSession};
use emath_syntax::parse_str;
use std::collections::BTreeMap;

/// Admit one `.emath` source and return the full checked result.
fn check(source: &str) -> CheckResult {
    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned("aj8d-surface.emath", source)
}

fn errors_of(result: &CheckResult) -> Vec<String> {
    result
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
}

/// First declared output field's semantic TypeNode. `None` when the
/// declaration has no outputs or its first output carried no type.
fn first_output_type(result: &CheckResult) -> Option<emath_ir::TypeNode> {
    let package = &result.package;
    let field = package.declarations.first()?.outputs.first()?;
    package.types.get(field.ty.index()).cloned()
}

/// First declared input field's semantic TypeNode. `None` when the
/// declaration has no inputs or its first input carried no type.
fn first_input_type(result: &CheckResult) -> Option<emath_ir::TypeNode> {
    let package = &result.package;
    let field = package.declarations.first()?.inputs.first()?;
    package.types.get(field.ty.index()).cloned()
}

/// All declared input TypeNodes of the first declaration, in order.
/// Panics if any declared input carries no admitted type.
fn input_types(result: &CheckResult) -> Vec<emath_ir::TypeNode> {
    let package = &result.package;
    let field = package
        .declarations
        .first()
        .expect("at least one declaration");
    field
        .inputs
        .iter()
        .map(|f| {
            package
                .types
                .get(f.ty.index())
                .cloned()
                .expect("declared input carries an admitted semantic type")
        })
        .collect()
}

/// Build a minimal admitting `function` carrying the given `inputs:` field
/// lines (each already indented 8 spaces) plus a stub `definitions:`
/// section. Pass 1 sources put the type on an `outputs:` field, but an
/// output demands a conforming definition (E-NAME-023) and Phase 1 has no
/// Option/Result VALUE syntax to write one — so this pass carries the type
/// on an INPUT, which exercises the identical `map_type` admission path
/// without inventing value syntax that belongs to a later pass. The stub
/// `definitions:` satisfies `function`'s mandatory `definitions` section
/// (E-KIND-011).
fn fn_with_inputs(inputs: &str) -> String {
    format!(
        "emath function probe:\n    inputs:\n{inputs}\n    definitions:\n        t = 1.0\n"
    )
}

/// The PASSING contract these tests assert: the Option/Result spellings may
/// not be refused. Currently each refuses, so every assert! here would fail
/// on the unedited tree → RED.
#[test]
fn aj8d_float_option_declaration_admits() {
    let source = fn_with_inputs("        x: Option<Float64>\n");
    let result = check(&source);
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "an Option<Float64> declaration type must ADMIT; got: {errors:#?}"
    );
    assert_eq!(
        first_input_type(&result)
            .expect("Option<Float64> input must carry an admitted TypeNode"),
        emath_ir::TypeNode::OptionType(Box::new(emath_ir::TypeNode::Float64))
    );
}

#[test]
fn aj8d_result_declaration_admits() {
    let source = fn_with_inputs("        x: Result<Int, Bool>\n");
    let result = check(&source);
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "a Result<Int, Bool> declaration type must ADMIT; got: {errors:#?}"
    );
    assert_eq!(
        first_input_type(&result)
            .expect("Result<Int, Bool> input must carry an admitted TypeNode"),
        emath_ir::TypeNode::Result {
            ok: Box::new(emath_ir::TypeNode::Int),
            error: Box::new(emath_ir::TypeNode::Bool),
        }
    );
}

#[test]
fn aj8d_graph_declaration_admits() {
    // Graph compute ops exist; only the Graph TYPE spelling is missing.
    // A conforming definition removes the unrelated mandatory-definitions
    // error (E-KIND-011) so the ONLY gate being tested is type admission.
    let result = check(
        "emath function graph_probe:\n    outputs:\n        g: Graph\n    definitions:\n        g = graph { 0, 1; 0 --> 1 }\n",
    );
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "a Graph output type must ADMIT; got: {errors:#?}"
    );
}

#[test]
fn aj8d_field_declaration_admits() {
    // Field and GF are one prime-field spelling (NAMING.md canonicalizes
    // `GF<p>`; `Field` is the bead's declared alias). `Field<7>` must
    // ADMIT as an executable declaration type carrying its modulus. The
    // output needs a conforming definition (E-NAME-023), so the ONLY gate
    // under test is type admission.
    let result = check(
        "emath function field_probe:\n    outputs:\n        f: Field<7>\n    definitions:\n        f = 7\n",
    );
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "a `Field<7>` output type must ADMIT; got: {errors:#?}"
    );
    assert_eq!(
        first_output_type(&result).expect("Field<7> output carries an admitted TypeNode"),
        emath_ir::TypeNode::FieldPrime { modulus: 7 },
        "Field<7> lowers to the distinct prime-field node carrying its modulus"
    );
}

/// GF<7> admits today: `map_type` maps `"GF" => TypeNode::Int`, so the
/// type line never hits the Phase-1 refusal (unlike Option/Result/
/// Graph/Field). The contract: GF<p> is a DISTINCT executable field
/// type carrying its modulus. Asserting the output's semantic TypeNode
/// is NOT a plain Int is the RED that proves the silent collapse.
/// (Pass 8: was `v = x + 0`; Int+Int widens to F64, which the new
/// FieldPrime-exactness rule now refuses as a float into an exact field
/// type — the honest integer element form is `v = x`, which preserves the
/// distinct-node intent.)
#[test]
fn aj8d_gf_prime_is_distinct_type() {
    let result = check(
        "emath function gf_probe:\n    inputs:\n        x: Int\n    outputs:\n        v: GF<7>\n    definitions:\n        v = x\n",
    );
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "GF<7> must admit cleanly (no Phase-1 type refusal); got: {errors:#?}"
    );
    let node = first_output_type(&result)
        .expect("gf_probe must carry a declared output type");
    assert!(
        !matches!(node, emath_ir::TypeNode::Int),
        "GF<7> must be a DISTINCT prime-field type, not silently \
         collapsed to plain Int; got node: {node:#?}"
    );
    assert_eq!(
        node,
        emath_ir::TypeNode::FieldPrime { modulus: 7 },
        "GF<7> lowers to the distinct prime-field node carrying 7"
    );
}

#[test]
fn aj8d_parser_accepts_type_spellings() {
    // Parse-level evidence: the SYNTAX layer already admits these type
    // spellings as generics. Refusal happens at SEMA (map_type), not in
    // the parser — this locates the gap precisely. This test documents
    // the parse-level admission; it may pass on the current tree and is
    // not an execution-type claim.
    let (_tree, diag) = parse_str(
        "emath function t:\n    outputs:\n        a: Option<Float64>\n        b: Result<Int, String>\n        c: Graph\n        d: Field\n        e: GF<7>\n",
    );
    let errors = diag.errors().collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "syntax layer must parse these generic type spellings; got: {errors:#?}"
    );
}

// --- Pass 2: recursive admission + identity (emath-option-result-graph-field-aj8d) ---
//
// map_type must admit Option/Result as executable declaration types with
// EXACTLY one/two generic args, recursing into the argument types, yielding
// identical TypeNodes for identical spellings and distinct nodes for
// distinct spellings. All asserted failure-first against the current tree
// (Option/Result refuse with E-TYPE-010).

#[test]
fn aj8d_recursive_option_result_admits() {
    // Three nested spellings must each admit cleanly (recursion descends).
    let sources = [
        fn_with_inputs("        x: Option<Option<Int>>\n"),
        fn_with_inputs("        x: Result<Int, Option<Float64>>\n"),
        fn_with_inputs("        x: Option<Result<Int, Bool>>\n"),
    ];
    for source in sources {
        let result = check(&source);
        let errors = errors_of(&result);
        assert!(
            errors.is_empty(),
            "recursive Option/Result spelling must ADMIT; got: {errors:#?} for `{source}`"
        );
        assert!(
            first_input_type(&result).is_some(),
            "recursive spelling must carry an admitted TypeNode; `{source}`"
        );
    }
}

#[test]
fn aj8d_nested_node_structure() {
    // `Option<Option<Int>>` must lower to a two-level Option node, proving
    // recursion descends rather than collapsing to a single shell.
    let source = fn_with_inputs("        x: Option<Option<Int>>\n");
    let result = check(&source);
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "nested Option must ADMIT; got: {errors:#?}"
    );
    let node = first_input_type(&result)
        .expect("nested Option must carry an admitted TypeNode");
    assert_eq!(
        node,
        emath_ir::TypeNode::OptionType(Box::new(emath_ir::TypeNode::OptionType(
            Box::new(emath_ir::TypeNode::Int)
        )))
    );
}

#[test]
fn aj8d_same_spelling_identical_node() {
    // Two inputs with the identical spelling must yield the identical node.
    let source = fn_with_inputs(
        "        x: Option<Float64>\n        y: Option<Float64>\n",
    );
    let result = check(&source);
    assert!(
        errors_of(&result).is_empty(),
        "identity probe must ADMIT; got: {:?}",
        errors_of(&result)
    );
    let nodes = input_types(&result);
    assert_eq!(nodes.len(), 2, "two declared Option<Float64> inputs");
    assert_eq!(
        nodes[0], nodes[1],
        "same spelling must map to identical TypeNode"
    );
}

#[test]
fn aj8d_distinct_spellings_distinct_node() {
    // Different spellings must map to distinct nodes.
    let source = fn_with_inputs(
        "        a: Option<Float64>\n        b: Option<Int>\n        c: Result<Int, Bool>\n",
    );
    let result = check(&source);
    assert!(
        errors_of(&result).is_empty(),
        "identity probe must ADMIT; got: {:?}",
        errors_of(&result)
    );
    let nodes = input_types(&result);
    assert_eq!(nodes.len(), 3, "three declared inputs");
    assert_ne!(nodes[0], nodes[1], "Option<Float64> ≠ Option<Int>");
    assert_ne!(nodes[0], nodes[2], "Option<Float64> ≠ Result<Int, Bool>");
    assert_ne!(nodes[1], nodes[2], "Option<Int> ≠ Result<Int, Bool>");
}

#[test]
fn aj8d_option_wrong_arity_refused() {
    // Option admits EXACTLY one generic arg; two must refuse with the
    // E-TYPE-010 arity message naming the type.
    let source = fn_with_inputs("        x: Option<Int, Float64>\n");
    let result = check(&source);
    let errs = errors_of(&result);
    assert!(
        errs.iter()
            .any(|e| e.contains("E-TYPE-010") && e.contains("requires exactly one type argument")),
        "`Option<Int, Float64>` must refuse with an E-TYPE-010 arity message; got: {errs:#?}"
    );
}

#[test]
fn aj8d_result_wrong_arity_refused() {
    // Result admits EXACTLY two generic args; 1 and 3 must refuse.
    for source in [
        fn_with_inputs("        x: Result<Int>\n"),
        fn_with_inputs("        x: Result<Int, Bool, Float64>\n"),
    ] {
        let result = check(&source);
        let errs = errors_of(&result);
        assert!(
            errs.iter().any(|e| e.contains("E-TYPE-010")
                && e.contains("requires exactly two type arguments")),
            "under/over-arg `Result` must refuse with an E-TYPE-010 arity message; got: {errs:#?} for `{source}`"
        );
    }
}

// --- Pass 6: Graph type admission (emath-option-result-graph-field-aj8d) ---
//
// The graph COMPUTE surface is complete and closed (reachability,
// bfs_order, shortest_distances, out_degrees, graph_laplacian,
// graph_symmetrize, bellman_ford, sparse_triplets, sparse_from_triplets),
// all matrix-carrier based. The GAP this pass pins is the Graph TYPE
// spelling. Decision (b): `Graph` is an ALIAS for the dense
// `Matrix<Float64>` adjacency carrier that the graph ops already consume —
// the graph ops check SHAPES (ParamShape::Matrix), not TypeNode equality,
// and a distinct TypeNode::Graph would mis-map to Infer::F64 in
// `infer_from_node`, breaking both conformance and the op call flow.

/// The bare `Graph` spelling must admit as a declaration type, mapping to
/// the dense `Matrix<Float64>` adjacency carrier.
#[test]
fn aj8d_graph_maps_to_matrix_alias() {
    let source = fn_with_inputs("        x: Graph\n");
    let result = check(&source);
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "a bare `Graph` declaration type must ADMIT; got: {errors:#?}"
    );
    assert_eq!(
        first_input_type(&result).expect("Graph input must carry an admitted TypeNode"),
        emath_ir::TypeNode::Matrix {
            element: Box::new(emath_ir::TypeNode::Float64),
            rows: None,
            cols: None,
        },
        "Graph is the dense Matrix<Float64> adjacency alias (decision b)"
    );
}

/// A Graph-typed field must FEED the existing graph op surface end to end:
/// the declaration admits, the graph literal conforms to the `Graph`
/// output field, and reachability evaluates through it.
#[test]
fn aj8d_graph_field_feeds_reachability_op() {
    let source = "emath function gp:\n    outputs:\n        g: Graph\n        r: Vector<Float64>\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        r = reachability(g, 0)\n";
    let result = check(source);
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "Graph field must admit and lower; got: {errors:#?}"
    );
    let values = eval_definitions_values(
        &result.package,
        &result.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("Graph field must evaluate through reachability: {fault}"));
    let Value::Vector(mask) = values.get("r").expect("reachability result") else {
        panic!("reachability must return a vector, got: {values:?}");
    };
    assert_eq!(
        mask,
        &[1.0, 1.0, 1.0, 1.0],
        "reachability through a Graph-typed field must match the matrix surface"
    );
}

/// Pass 5: out_degrees drives through the SAME Graph-typed field (adjacency
/// degree), complementing reachability — one Graph declaration drives both
/// adjacency-degree AND reachability. Exact values discriminate: the probe
/// graph (0->1, 0->2, 1->3, 2->3) must yield out_degrees = [2,1,1,0]; a
/// missing/wrong out_degrees admission yields anything else.
#[test]
fn aj8d_graph_field_drives_out_degrees() {
    let source = "emath function gp:\n    outputs:\n        g: Graph\n        d: Vector<Float64>\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        d = out_degrees(g)\n";
    let result = check(source);
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "Graph field must admit and drive out_degrees; got: {errors:#?}"
    );
    let values = eval_definitions_values(
        &result.package,
        &result.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("Graph field must evaluate out_degrees: {fault}"));
    let Value::Vector(degrees) = values.get("d").expect("out_degrees result") else {
        panic!("out_degrees must return a vector, got: {values:?}");
    };
    assert_eq!(
        degrees,
        &[2.0, 1.0, 1.0, 0.0],
        "out_degrees through a Graph-typed field must match the matrix surface"
    );
}

/// Pass 5: the alias is BIDIRECTIONAL. A graph value admitted into a
/// `Matrix<Float64>`-typed field (the adjacency spells into a Matrix position)
/// must flow out of THAT field into a graph op unchanged — proving Matrix and
/// Graph are the same carrier node and interchange freely in both directions.
#[test]
fn aj8d_matrix_field_interchanges_with_graph_value() {
    let source = "emath function mp:\n    outputs:\n        m: Matrix<Float64>\n        r: Vector<Float64>\n        d: Vector<Float64>\n    definitions:\n        m = graph { 0, 1, 2, 3; 0 --> 1, 0 --> 2, 1 --> 3, 2 --> 3 }\n        r = reachability(m, 0)\n        d = out_degrees(m)\n";
    let result = check(source);
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "graph literal must conform to Matrix<Float64> and drive ops; got: {errors:#?}"
    );
    // The Matrix field carries the SAME dense adjacency node as Graph.
    assert_eq!(
        first_output_type(&result).expect("m output carries an admitted TypeNode"),
        emath_ir::TypeNode::Matrix {
            element: Box::new(emath_ir::TypeNode::Float64),
            rows: None,
            cols: None,
        },
        "m: Matrix<Float64> must be (graph-)identical to Graph's node"
    );
    let values = eval_definitions_values(
        &result.package,
        &result.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("Matrix field must evaluate both graph ops: {fault}"));
    let Value::Vector(mask) = values.get("r").expect("reachability result") else {
        panic!("reachability must return a vector, got: {values:?}");
    };
    let Value::Vector(degrees) = values.get("d").expect("out_degrees result") else {
        panic!("out_degrees must return a vector, got: {values:?}");
    };
    assert_eq!(mask, &[1.0, 1.0, 1.0, 1.0], "reachability via Matrix field");
    assert_eq!(
        degrees,
        &[2.0, 1.0, 1.0, 0.0],
        "out_degrees via Matrix field"
    );
}

/// `Graph<T>` (any generic count) is a typed arity refusal naming "Graph".
/// The assertion depends on the NEW message so it cannot pass prepayment.
#[test]
fn aj8d_graph_generic_refused() {
    for source in [
        fn_with_inputs("        x: Graph<Int>\n"),
        fn_with_inputs("        x: Graph<Float64>\n"),
        fn_with_inputs("        x: Graph<Int, Float64>\n"),
    ] {
        let result = check(&source);
        let errs = errors_of(&result);
        assert!(
            errs.iter().any(|e| e.contains("E-TYPE-010")
                && e.contains("Graph")
                && e.contains("admits no type arguments")),
            "`Graph<T>` must refuse with an E-TYPE-010 arity message naming Graph; got: {errs:#?} for `{source}`"
        );
    }
}

// --- Pass 7: Field/GF<p> prime type (emath-option-result-graph-field-aj8d) ---
//
// The bead's prime-field contract: GF<7> is a DISTINCT prime-field type
// (NOT the silent `TypeNode::Int` collapse flagged in Pass 1 — the prime
// is dropped there), the prime is a TYPE-LEVEL constant, and the value
// layer is exact i64 modular arithmetic (tests/emath-ir/tests/
// option_result_values.rs). Sema admits `Field<p>` / `GF<p>` for exactly
// ONE PRIME INTEGER LITERAL argument. Every refusal below is an
// E-TYPE-010 message naming the spelling and the constraint, never a
// panic. All are failure-first: before the Pass 7 `map_type` arm, every
// wrong-prime spelling silently mapped to `TypeNode::Int` (or refused for
// the bare `Field` name) and every assert here FAILED.

#[test]
fn aj8d_field_prime_non_prime_refused() {
    for source in [
        fn_with_inputs("        x: Field<8>\n"),
        fn_with_inputs("        x: GF<4>\n"),
        fn_with_inputs("        x: GF<9>\n"),
    ] {
        let result = check(&source);
        let errs = errors_of(&result);
        assert!(
            errs.iter()
                .any(|e| e.contains("E-TYPE-010") && e.contains("prime")),
            "a non-prime field modulus must refuse with an E-TYPE-010 constraint message naming the prime; got: {errs:#?} for `{source}`"
        );
    }
}

#[test]
fn aj8d_field_prime_literal_required() {
    // The modulus must be an integer LITERAL — a fixed field is a
    // type-level constant, not a value-level expression or another type.
    for source in [
        fn_with_inputs("        x: GF<Int>\n"),
        fn_with_inputs("        x: GF<Float64>\n"),
        fn_with_inputs("        x: GF<n>\n"),
    ] {
        let result = check(&source);
        let errs = errors_of(&result);
        assert!(
            errs.iter()
                .any(|e| e.contains("E-TYPE-010") && e.contains("LITERAL")),
            "a non-literal field modulus must refuse with an E-TYPE-010 literal-requirement message; got: {errs:#?} for `{source}`"
        );
    }
}

#[test]
fn aj8d_field_arity_refused() {
    // Exactly ONE generic arg: bare `Field`, bare `GF`, and two args all
    // refuse with an E-TYPE-010 arity message containing "requires
    // exactly one".
    for source in [
        fn_with_inputs("        x: Field\n"),
        fn_with_inputs("        x: GF\n"),
        fn_with_inputs("        x: Field<7, 2>\n"),
        fn_with_inputs("        x: GF<7, 2>\n"),
    ] {
        let result = check(&source);
        let errs = errors_of(&result);
        assert!(
            errs.iter()
                .any(|e| e.contains("E-TYPE-010") && e.contains("requires exactly one")),
            "wrong-arity `Field`/`GF` must refuse with an E-TYPE-010 arity message; got: {errs:#?} for `{source}`"
        );
    }
}

#[test]
fn aj8d_field_prime_identity_distinct() {
    // GF<7>, GF<5>, and Int are three DISTINCT types; the field prime is
    // type-level identity, so different primes never collapse and display
    // carries the modulus.
    let source = fn_with_inputs(
        "        a: GF<7>\n        b: GF<5>\n        c: Int\n        d: Field<7>\n",
    );
    let result = check(&source);
    assert!(
        errors_of(&result).is_empty(),
        "distinct prime-field spellings must ADMIT; got: {:?}",
        errors_of(&result)
    );
    let nodes = input_types(&result);
    assert_eq!(nodes.len(), 4, "four declared inputs");
    assert_ne!(nodes[0], nodes[1], "GF<7> ≠ GF<5> (the prime is identity)");
    assert_ne!(nodes[0], nodes[2], "GF<7> ≠ Int (no silent collapse)");
    assert_eq!(
        nodes[0], nodes[3],
        "GF<7> and Field<7> are the SAME prime-field spelling"
    );
    assert_eq!(
        nodes[0].display_name(),
        "Field<7>",
        "display is sane for GF<7>"
    );
    assert_eq!(nodes[1].display_name(), "Field<5>", "display carries the prime");
    assert_ne!(nodes[2].display_name(), "Field<7>", "Int display stays Int");
}

#[test]
fn aj8d_field_prime_boundary() {
    // p = 2 is the smallest prime and must admit; p < 2 and p above the
    // i32::MAX cap refuse.
    let result = check(&fn_with_inputs("        x: GF<2>\n"));
    assert!(
        errors_of(&result).is_empty(),
        "GF<2> (the smallest prime) must ADMIT; got: {:?}",
        errors_of(&result)
    );
    for source in [
        fn_with_inputs("        x: GF<1>\n"),
        fn_with_inputs("        x: GF<0>\n"),
        fn_with_inputs("        x: GF<2147483648>\n"),
    ] {
        let result = check(&source);
        let errs = errors_of(&result);
        assert!(
            errs.iter()
                .any(|e| e.contains("E-TYPE-010") && e.contains("prime")),
            "out-of-range field modulus must refuse with an E-TYPE-010 prime-constraint message; got: {errs:#?} for `{source}`"
        );
    }
}

#[test]
fn aj8d_field_prime_canonical_identity() {
    // The canonical `field:<p>` encoding (canonical.rs, schema
    // `emath.sir`) keeps GF<7>, GF<5>, and Int as DISTINCT package
    // identities — the encode arm must not collapse primes.
    use emath_ir::canonical::canonical_package;
    let seven = check(&fn_with_inputs("        x: GF<7>\n"));
    assert!(errors_of(&seven).is_empty(), "GF<7> admits for canonical");
    let five = check(&fn_with_inputs("        x: GF<5>\n"));
    assert!(errors_of(&five).is_empty(), "GF<5> admits for canonical");
    let int = check(&fn_with_inputs("        x: Int\n"));
    assert!(errors_of(&int).is_empty(), "Int admits for canonical");
    let id7 = canonical_package(&seven.package);
    let id5 = canonical_package(&five.package);
    let id_int = canonical_package(&int.package);
    assert_ne!(id7, id5, "GF<7> and GF<5> have distinct canonical identity");
    assert_ne!(id7, id_int, "GF<7> and Int have distinct canonical identity");
}

// --- Pass 8: hardened typed refusals (emath-option-result-graph-field-aj8d) ---
//
// Every malformed composite-type spelling must refuse with a TYPED
// E-TYPE-010 message naming the spelling and the exact constraint —
// never a silent collapse, never a panic, never an unrelated generic
// diagnostic. Each row is a discriminating content assertion: a
// pre-existing but different refusal (wrong code, or a message that
// names the wrong constraint) fails the row.

/// Table of mismatched-generic spellings across Option/Result/Graph/Field
/// (and a nested malformed inner generic). Each must refuse with the
/// E-TYPE-010 arity/constraint code — no row may fall through silent.
#[test]
fn aj8d_refuse_mismatched_generics_matrix() {
    let rows = [
        "        x: Option<Int, Float64>\n",   // Option arity 2
        "        x: Result<Int>\n",            // Result under-armed
        "        x: Result<Int, Bool, Float64>\n", // Result over-armed
        "        x: Option<Option<Int, Float64, Bool>>\n", // nested malformed inner Option
        "        x: Graph<Int>\n",             // Graph takes no args
        "        x: Field<7, 2>\n",            // Field arity 2
        "        x: GF<>\n",                   // GF explicit-empty generic
        "        x: GF<7, 2>\n",               // GF arity 2
    ];
    for input in rows {
        let result = check(&fn_with_inputs(input));
        let errs = errors_of(&result);
        assert!(
            errs.iter().any(|e| e.contains("E-TYPE-010")),
            "mismatched generic `{input}` must refuse with an E-TYPE-010 typed message; got: {errs:#?}"
        );
        assert!(
            !errs.iter().any(|e| e.contains("panic")),
            "no row may admit or fall through silently; `{input}` → {errs:#?}"
        );
    }
}

/// A malformed INNER generic must be refused by the recursion itself with
/// the inner constraint named — the outer shell may not silently swallow
/// it or fall back to a generic "unknown type".
#[test]
fn aj8d_nested_malformed_inner_generic_names_inner_constraint() {
    let result = check(&fn_with_inputs("        x: Option<Option<Int, Float64, Bool>>\n"));
    let errs = errors_of(&result);
    assert!(
        errs.iter().any(|e| e.contains("E-TYPE-010")
            && e.contains("Option")
            && e.contains("exactly one type argument")),
        "nested malformed inner generic must name the inner Option arity rule; got: {errs:#?}"
    );
}

/// A nested TYPE argument (`GF<GF<7>>`, `GF<Option<Int>>`) is not an
/// integer literal — the field prime is a type-level constant, so these
/// must refuse with the literal-requirement message, never silently.
#[test]
fn aj8d_nested_field_type_arg_refused_literal() {
    for input in [
        "        x: GF<GF<7>>\n",
        "        x: GF<Option<Int>>\n",
        "        x: Field<GF<7>>\n",
    ] {
        let result = check(&fn_with_inputs(input));
        let errs = errors_of(&result);
        assert!(
            errs.iter().any(|e| e.contains("E-TYPE-010") && e.contains("LITERAL")),
            "nested type arg `{input}` must refuse with an E-TYPE-010 literal-requirement message; got: {errs:#?}"
        );
    }
}

/// A prime integer literal too large to even fit i64 is a RANGE refusal
/// naming the bound — never mis-typed as "not a literal", never parsed
/// through a lossy path. (RED against the pre-pass-8 wording, which said
/// only "requires ... LITERAL".)
#[test]
fn aj8d_field_literal_overflow_refused_typed() {
    let result = check(&fn_with_inputs("        x: GF<99999999999999999999999>\n"));
    let errs = errors_of(&result);
    assert!(
        errs.iter()
            .any(|e| e.contains("E-TYPE-010") && e.contains("exceeds the maximum supported field modulus")),
        "an overlarge prime literal must refuse with an E-TYPE-010 range message; got: {errs:#?}"
    );
    assert!(
        !errs.iter().any(|e| e.contains("requires a prime integer LITERAL modulus")
            && !e.contains("exceeds")),
        "an overlarge literal is still a literal; the refusal must not mis-say it is not one; got: {errs:#?}"
    );
}

/// A negative modulus (`GF<-3>`) never reaches sema: the surface grammar
/// rejects `-` in a type-argument position (E-SYN) before `map_type` sees
/// it. It is therefore a refusal either way — never a silent admission
/// that flakes to a fake field. This pins that boundary (audit verdict: OK,
/// refused at parse; not an E-TYPE-010 because the spelling never parses).
#[test]
fn aj8d_field_negative_modulus_refused() {
    let result = check(&fn_with_inputs("        x: GF<-3>\n"));
    assert!(
        !errors_of(&result).is_empty(),
        "a negative field modulus must never admit; got empty diagnostics"
    );
    assert!(
        first_input_type(&result).is_none(),
        "a negative field modulus must not carry an admitted type node"
    );
}

/// An empty generic list on a composite type that REQUIRES arguments is
/// an arity refusal, and a literal-but-outside-[2, i32::MAX] prime is a
/// range refusal — `GF<>` (arity) vs `GF<2147483648>` (range) must not
/// collide onto one imprecise message.
#[test]
fn aj8d_empty_generic_is_arity_not_range() {
    let empty = check(&fn_with_inputs("        x: GF<>\n"));
    let empty_errs = errors_of(&empty);
    assert!(
        empty_errs.iter().any(|e| e.contains("E-TYPE-010") && e.contains("requires exactly one")),
        "`GF<>` is an arity refusal, got: {empty_errs:#?}"
    );

    let over = check(&fn_with_inputs("        x: GF<2147483648>\n"));
    let over_errs = errors_of(&over);
    assert!(
        over_errs.iter().any(|e| e.contains("E-TYPE-010") && e.contains("requires a prime modulus 2 ≤ p")),
        "`GF<2147483648>` is a range refusal, got: {over_errs:#?}"
    );
}

// --- Pass 9: metamorphic Graph relabel (emath-option-result-graph-field-aj8d) ---
//
// TEST-ONLY pass (no production edits). Graph IS the dense Matrix<Float64>
// adjacency alias; RELABEL = permute rows AND cols of the adjacency and
// rename the endpoint. The LAW, driven through the real `.emath`
// reachability surface (check + eval_definitions_values):
//   reachability(A', P(src)) == P ⊳ reachability(A, src)
// where P maps old vertex → new vertex and "P ⊳ v" permutes a per-vertex
// vector (new[P[i]] = old[i]). The relabel PIECE is a real permutation
// (rotation), so the permuted mask DIFFERS from the original (this is a
// discriminating metamorphic value, not a coincidental fixpoint).

/// Report the reachability mask vector from a checked source (read-only
/// helper for the relabel law).
fn reachability_mask(source: &str) -> Vec<f64> {
    let result = check(source);
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "relabel probe must admit and lower; got: {errors:#?}"
    );
    let values = eval_definitions_values(
        &result.package,
        &result.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("relabel probe must evaluate: {fault}"));
    let Value::Vector(mask) = values.get("r").expect("reachability result") else {
        panic!("reachability must return a vector, got: {values:?}");
    };
    mask.clone()
}

/// new_mask[P[i]] = old_mask[i] for a permutation P (old → new).
fn permute_mask(p: &[usize], mask: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; mask.len()];
    for (i, &v) in mask.iter().enumerate() {
        out[p[i]] = v;
    }
    out
}

#[test]
fn aj8d_meta_graph_relabel_reachability_equivariance() {
    // Original: edges 0->1, 1->3, 2->3. Vertex 2 is NOT reachable from 0,
    // so the mask is not all-ones and relabeling the endpoints changes it.
    let original = "emath function gp:\n    outputs:\n        g: Graph\n        r: Vector<Float64>\n    definitions:\n        g = graph { 0, 1, 2, 3; 0 --> 1, 1 --> 3, 2 --> 3 }\n        r = reachability(g, 0)\n";
    let mask_orig = reachability_mask(original);
    assert_eq!(
        mask_orig,
        &[1.0, 1.0, 0.0, 1.0],
        "orig reachability(g,0) must be [1,1,0,1]"
    );

    // Rotation P (old->new): 0->1, 1->2, 2->3, 3->0. Relabel the literal
    // (permute endpoints of each edge) and the source P(src)=P(0)=1.
    let p: &[usize] = &[1, 2, 3, 0];
    let relabeled = "emath function gp:\n    outputs:\n        g: Graph\n        r: Vector<Float64>\n    definitions:\n        g = graph { 0, 1, 2, 3; 1 --> 2, 2 --> 0, 3 --> 0 }\n        r = reachability(g, 1)\n";
    let mask_perm = reachability_mask(relabeled);

    // The metamorphic law: reachability(A', P(src)) == P ⊳ reachability(A, src).
    let expected = permute_mask(p, &mask_orig);
    assert_eq!(
        mask_perm, expected,
        "relabel P={p:?}: reachability(A', 1) must equal P ⊳ reachability(A, 0)"
    );
    // Discrimination: the relabel genuinely moves the unreachable vertex 2
    // to a new position, so the permuted mask differs from the original.
    assert_ne!(
        mask_perm, mask_orig,
        "the relabel must permute the mask, not coincide with it"
    );
}

// --- Pass 10: Option/Result/field builtins callable from .emath TEXT ---
// (emath-option-result-graph-field-aj8d).
//
// The interpreter + emitter + term_compile surfaces for these names are
// proven in `tests/emath-ir/tests/option_result_values.rs` via the API
// helper. THIS section proves the USER surface: the same names written
// as ordinary `.emath` calls in definitions admit through sema and
// evaluate through the reference VM — closing the E-TYPE-003 gate that
// previously refused every one of them from text.
//
// Cleaner: full source -> sema -> emitter -> interp -> values.
fn text_values(source: &str) -> BTreeMap<String, Value> {
    let result = check(source);
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "text surface must admit; got: {errors:#?}\nsource:\n{source}"
    );
    eval_definitions_values(
        &result.package,
        &result.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("text surface must evaluate: {fault}"))
}

/// Option constructor, predicate, and unwrap-or from text: some/none
/// discriminate and unwrap yields the injected default or payload.
#[test]
fn aj8d_text_option_predicates_and_unwrap() {
    let values = text_values(
        "emath function o:\n    definitions:\n        s = option_is_some(option_some(1.0))\n        n = option_is_some(option_none())\n        u1 = option_unwrap_or(option_none(), 9.0)\n        u2 = option_unwrap_or(option_some(2.0), 9.0)\n",
    );
    assert_eq!(values.get("s"), Some(&Value::Bool(true)));
    assert_eq!(values.get("n"), Some(&Value::Bool(false)));
    assert_eq!(values.get("u1"), Some(&Value::F64(9.0)));
    assert_eq!(values.get("u2"), Some(&Value::F64(2.0)));
}

/// Result constructor and is-ok predicate from text: ok vs err
/// discriminate, and unwrap_or injects the default on the Err side.
#[test]
fn aj8d_text_result_predicates_and_unwrap() {
    let values = text_values(
        "emath function r:\n    definitions:\n        ok = result_is_ok(result_ok(3.5))\n        bad = result_is_ok(result_err(7.0))\n        u = result_unwrap_or(result_err(7.0), 9.0)\n        w = result_unwrap_or(result_ok(2.0), 9.0)\n",
    );
    assert_eq!(values.get("ok"), Some(&Value::Bool(true)));
    assert_eq!(values.get("bad"), Some(&Value::Bool(false)));
    assert_eq!(values.get("u"), Some(&Value::F64(9.0)));
    assert_eq!(values.get("w"), Some(&Value::F64(2.0)));
}

/// result_error_of projects the Err payload as an Option (Some(error));
/// Ok projects to none. Unwrapping the projection recovers the value or
/// falls back to the default.
#[test]
fn aj8d_text_error_of_projection() {
    let values = text_values(
        "emath function e:\n    definitions:\n        r = option_unwrap_or(result_error_of(result_err(7.0)), -1.0)\n        n = option_unwrap_or(result_error_of(result_ok(1.0)), -1.0)\n        tag = option_is_some(result_error_of(result_err(9.0)))\n",
    );
    assert_eq!(values.get("r"), Some(&Value::F64(7.0)));
    assert_eq!(values.get("n"), Some(&Value::F64(-1.0)));
    assert_eq!(values.get("tag"), Some(&Value::Bool(true)));
}

/// field_inv and mod_inv are the same modular-inverse call from text:
/// field_inv(3, 7) == mod_inv(3, 7) == 5 (exact i64), proving the
/// Int-backed prime-field surface.
#[test]
fn aj8d_text_field_mod_inv() {
    let values = text_values(
        "emath function f:\n    definitions:\n        x = field_inv(3.0, 7.0)\n        y = mod_inv(3.0, 7.0)\n",
    );
    assert!(
        matches!(values.get("x"), Some(Value::I64(5))),
        "field_inv(3, 7) must be the exact modular inverse 5, got {:?}",
        values.get("x")
    );
    assert!(
        matches!(values.get("y"), Some(Value::I64(5))),
        "mod_inv(3, 7) must be the exact modular inverse 5, got {:?}",
        values.get("y")
    );
}

/// A declared `Option<Int>` OUTPUT whose definition builds option_some(...)
/// proves the new carrier Inference conforms to a declared OptionType via
/// the conforms arm — the carrier flows, payload included.
#[test]
fn aj8d_text_option_int_output_carrier() {
    let values = text_values(
        "emath function oi:\n    outputs:\n        o: Option<Int>\n    definitions:\n        o = option_some(5)\n",
    );
    // Payload may materialize as i64 or f64 depending on literal lowering;
    // the contract is that the Option carrier is present and holds 5.
    assert!(
        matches!(
            values.get("o"),
            Some(Value::Option(Some(payload)))
                if matches!(&**payload, Value::I64(5) | Value::F64(5.0))
        ),
        "option_some(5) must build an Option<Int> carrier holding 5, got {:?}",
        values.get("o")
    );
}

// --- Pass 3: nested payloads + no hidden zero from .emath text ---
// (emath-option-result-graph-field-aj8d). Nested construction now
// COMPILES from text (pass 3 lifted carrier-in-payload at the term
// layer) and every unwrap below is total (unwrap_or — NO panicking
// unwrap exists at this layer).

/// Nested Some(Some(...)) composes from text without a carrier default:
/// outer unwrap_or picks the inner carrier (not a flattened none), and
/// chained unwrap_or recovers the payload.
#[test]
fn aj8d_text_nested_option_some_some() {
    // outer = Some(Some(5)); outer.unwrap_or(option_none()) = Some(5);
    //   .unwrap_or(9) = 5. The nested carrier survives both unwraps.
    let values = text_values(
        "emath function ns:\n    definitions:\n        sso = option_unwrap_or(option_unwrap_or(option_some(option_some(5.0)), option_none()), 9.0)\n        inner_is_some = option_is_some(option_unwrap_or(option_some(option_some(5.0)), option_none()))\n",
    );
    assert_eq!(values.get("sso"), Some(&Value::F64(5.0)));
    assert_eq!(values.get("inner_is_some"), Some(&Value::Bool(true)));
}

/// Some(None): outer is_some is TRUE (tag carries a nested none, not a
/// flattened empty), and unwrap_or recovers a none whose unwrap_or hits
/// the sentinel default.
#[test]
fn aj8d_text_nested_some_none() {
    let values = text_values(
        "emath function sn:\n    definitions:\n        outer_is_some = option_is_some(option_some(option_none()))\n        inner = option_unwrap_or(option_unwrap_or(option_some(option_none()), option_none()), 42.0)\n",
    );
    assert_eq!(values.get("outer_is_some"), Some(&Value::Bool(true)));
    assert_eq!(values.get("inner"), Some(&Value::F64(42.0)));
}

/// No hidden zero: payload 0.0 is a REAL value, distinct from none —
/// is_some(Some(0.0)) = true and unwrap_or picks 0.0, not the default.
/// A hidden-zero bug (none repurposed as 0) returns the 9 default here.
#[test]
fn aj8d_text_no_hidden_zero() {
    let values = text_values(
        "emath function hz:\n    definitions:\n        s = option_is_some(option_some(0.0))\n        u = option_unwrap_or(option_some(0.0), 9.0)\n",
    );
    assert_eq!(values.get("s"), Some(&Value::Bool(true)));
    assert_eq!(values.get("u"), Some(&Value::F64(0.0)));
}

// --- Pass 3: map via declared-call composition (NO new EmirOp) ---
// (emath-option-result-graph-field-aj8d). The user-mandated form:
// Option/Result `map` is expressed as a TEXT composition over the
// declared builtins + a user helper function — `if cond : then else :
// else` (EBNF surface.ebnf:111). No function-valued args; the helper
// `double` is a plain declared function called by name inside the
// branches.

/// Evaluate a declaration (by index) of an admitted package over a
/// binding map — drives the map-by-composition consumer whose Option
/// input the CLI eval lane cannot bind (Float64/Vector only).
fn text_values_at(source: &str, index: usize, bindings: BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    let result = check(source);
    let errors = errors_of(&result);
    assert!(
        errors.is_empty(),
        "text surface must admit; got: {errors:#?}\nsource:\n{source}"
    );
    let declaration = result
        .package
        .declarations
        .get(index)
        .expect("declaration index in bounds");
    eval_definitions_values(
        &result.package,
        declaration,
        &bindings,
        &BTreeMap::new(),
    )
    .unwrap_or_else(|fault| panic!("declaration {index} must evaluate: {fault}"))
}

/// The map-by-declared-composition SOURCE: a pure capability cell
/// `std.map.double` (the declared-call spine, ExprNode::Apply) + a
/// consumer that composes `map` from text with `if cond : then else :
/// else`. No new EmirOp, no function-valued arg — the helper cell is
/// called by dotted name inside the branches and its full identity is
/// its own package/declaration data.
/// The map-by-declared-composition SOURCE: a consumer that composes
/// `map` from text with `if cond : then else : else` (EBNF surface.ebnf:
/// 111) over the option builtins. The mapped function is an inline value
/// transform (`2.0 * x`) — a real executable builtin path. NOTE (gap):
/// a user-defined helper FUNCTION (`double(x)`) is NOT callable from a
/// definition (E-TYPE-003, lowering.rs generic builtin table), and a
/// bare pure CAPABILITY cell admits but has no local reference semantics
/// (eval faults "apply-capability: no local reference semantics for this
/// pure cell"; capability `definitions` is not admitted — biform
/// inputs/outputs/spec/algorithm only). So a named helper cell needs
/// registry reference semantics (handoff to the capability owner); the
/// composition structure itself is proven here with executable
/// arithmetic.
const MAP_COMPOSITION_SOURCE: &str = "emath function consumer:\n    inputs:\n        opt: Option<Float64>\n    outputs:\n        maybe: Option<Float64>\n    definitions:\n        maybe = if option_is_some(opt) : option_some(2.0 * option_unwrap_or(opt, 0.0)) else : option_none()\n";

/// option_map-by-composition from text: Some(3) → Some(6), None → None.
/// The consumer (declaration index 1) is evaluated over an Option payload
/// the CLI bind cannot supply, so it is bound directly here.
#[test]
fn aj8d_map_option_by_declared_composition() {
    let some_three = text_values_at(
        MAP_COMPOSITION_SOURCE,
        0,
        BTreeMap::from([(
            "opt".to_string(),
            Value::Option(Some(Box::new(Value::F64(3.0)))),
        )]),
    );
    assert!(
        matches!(
            some_three.get("maybe"),
            Some(Value::Option(Some(payload)))
                if matches!(payload.as_ref(), Value::F64(6.0))
        ),
        "option_map(Some(3), double) must be Some(6), got {:?}",
        some_three.get("maybe")
    );

    let none = text_values_at(
        MAP_COMPOSITION_SOURCE,
        0,
        BTreeMap::from([("opt".to_string(), Value::Option(None))]),
    );
    assert!(
        matches!(none.get("maybe"), Some(Value::Option(None))),
        "option_map(None, double) must be None, got {:?}",
        none.get("maybe")
    );
}

/// result_map-by-composition from text: Ok(3) → Ok(6), Err(7) stays
/// Err(7) — the error payload is untouched (observable via error_of). The
/// else branch returns the ORIGINAL `r` carrier intact.
const RESULT_MAP_SOURCE: &str = "emath function rconsumer:\n    inputs:\n        r: Result<Float64, Float64>\n    outputs:\n        mapped: Result<Float64, Float64>\n        projected: Option<Float64>\n    definitions:\n        mapped = if result_is_ok(r) : result_ok(2.0 * result_unwrap_or(r, 0.0)) else : r\n        projected = result_error_of(r)\n";

#[test]
fn aj8d_map_result_by_declared_composition() {
    let ok_three = text_values_at(
        RESULT_MAP_SOURCE,
        0,
        BTreeMap::from([(
            "r".to_string(),
            Value::Result {
                ok: true,
                payload: Box::new(Value::F64(3.0)),
            },
        )]),
    );
    assert!(
        matches!(
            ok_three.get("mapped"),
            Some(Value::Result { ok: true, payload })
                if matches!(payload.as_ref(), Value::F64(6.0))
        ),
        "result_map(Ok(3), double) must be Ok(6), got {:?}",
        ok_three.get("mapped")
    );

    let err_seven = text_values_at(
        RESULT_MAP_SOURCE,
        0,
        BTreeMap::from([(
            "r".to_string(),
            Value::Result {
                ok: false,
                payload: Box::new(Value::F64(7.0)),
            },
        )]),
    );
    assert!(
        matches!(
            err_seven.get("mapped"),
            Some(Value::Result { ok: false, payload })
                if matches!(payload.as_ref(), Value::F64(7.0))
        ),
        "result_map(Err(7), double) must stay Err(7), got {:?}",
        err_seven.get("mapped")
    );
    // error_of still projects the untouched error payload.
    assert!(
        matches!(
            err_seven.get("projected"),
            Some(Value::Option(Some(payload)))
                if matches!(payload.as_ref(), Value::F64(7.0))
        ),
        "error_of(Err(7)) must be Some(7), got {:?}",
        err_seven.get("projected")
    );
}

// --- Pass 6: field +/*/inverse as .emath DATA over int_rem (aj8d pass 6) ---
//
// The user authorizes ONE universal primitive, `int_rem` = exact-Euclidean
// `a.rem_euclid(m)` on i64. Field arithmetic is then expressed as
// CAPABILITY-CELL DATA in `.emath` — user functions named field7_add etc.
// compose `+`/`*`/`field_inv` with `int_rem`. NO field-named EmirOp, parser
// branch, or backend branch exists: int_rem is universal, the function NAMES
// are user data. Outputs are `Field<7>` (Int→FieldPrime conformance
// end-to-end). Every test executes through the reference VM.

/// field7_add over the Field<7> prime: `c = int_rem(a + b, 7)`.
/// (3,4)→0, (6,5)→4, (5,5)→3.
#[test]
fn aj8d_field7_addition_from_data() {
    let src = "emath function field7_add:\n    inputs:\n        a: Int\n        b: Int\n    outputs:\n        c: Field<7>\n    definitions:\n        c = int_rem(a + b, 7)\n";
    for (a, b, want) in [(3i64, 4i64, 0), (6, 5, 4), (5, 5, 3)] {
        let v = text_values_at(
            src,
            0,
            BTreeMap::from([("a".into(), Value::I64(a)), ("b".into(), Value::I64(b))]),
        );
        assert_eq!(
            v.get("c"),
            Some(&Value::I64(want)),
            "field7_add({a},{b}) via the Field<7> prime must be {want}, got {:?}",
            v.get("c")
        );
    }
}

/// field7_mul: `c = int_rem(a * b, 7)`. (3,4)→5, (3,5)→1, (5,5)→4.
#[test]
fn aj8d_field7_multiplication_from_data() {
    let src = "emath function field7_mul:\n    inputs:\n        a: Int\n        b: Int\n    outputs:\n        c: Field<7>\n    definitions:\n        c = int_rem(a * b, 7)\n";
    for (a, b, want) in [(3i64, 4i64, 5), (3, 5, 1), (5, 5, 4)] {
        let v = text_values_at(
            src,
            0,
            BTreeMap::from([("a".into(), Value::I64(a)), ("b".into(), Value::I64(b))]),
        );
        assert_eq!(
            v.get("c"),
            Some(&Value::I64(want)),
            "field7_mul({a},{b}) must be {want}, got {:?}",
            v.get("c")
        );
    }
}

/// field7_inverse: `c = field_inv(a, 7)` (the already-callable modular
/// inverse from pass 2, exact i64). 3→5, 5→3.
#[test]
fn aj8d_field7_inverse_from_data() {
    let src = "emath function field7_inv:\n    inputs:\n        a: Int\n    outputs:\n        c: Field<7>\n    definitions:\n        c = field_inv(a, 7)\n";
    for (a, want) in [(3i64, 5), (5, 3)] {
        let v = text_values_at(
            src,
            0,
            BTreeMap::from([("a".into(), Value::I64(a))]),
        );
        assert_eq!(
            v.get("c"),
            Some(&Value::I64(want)),
            "field7_inv({a}) must be {want}, got {:?}",
            v.get("c")
        );
    }
}

/// int_rem sign law from text: Euclidean remainder is ALWAYS non-negative,
/// so int_rem(-1, 7) == 6 (not the -1 a truncated `%` would give). This is
/// the test the backend remainder-sign mutation (truncated %) must kill.
#[test]
fn aj8d_int_rem_sign_law_from_text() {
    let src = "emath function irs:\n    inputs:\n        a: Int\n        m: Int\n    outputs:\n        c: Int\n    definitions:\n        c = int_rem(a, m)\n";
    let v = text_values_at(
        src,
        0,
        BTreeMap::from([("a".into(), Value::I64(-1)), ("m".into(), Value::I64(7))]),
    );
    assert_eq!(
        v.get("c"),
        Some(&Value::I64(6)),
        "int_rem(-1, 7) must be the Euclidean 6, got {:?}",
        v.get("c")
    );
}

/// int_rem typed refusal: modulus `m <= 0` is a typed EvalFault, never a
/// panic and never a silent result.
#[test]
fn aj8d_int_rem_zero_modulus_faults() {
    let r = check(
        "emath function iz:\n    inputs:\n        a: Int\n    outputs:\n        c: Int\n    definitions:\n        c = int_rem(a, 0)\n",
    );
    let e = errors_of(&r);
    assert!(e.is_empty(), "int_rem with m=0 must ADMIT (fault at runtime); got: {e:?}");
    let err = eval_definitions_values(
        &r.package,
        &r.package.declarations[0],
        &BTreeMap::from([("a".into(), Value::I64(5))]),
        &BTreeMap::new(),
    )
    .expect_err("int_rem(5,0) must be a typed fault, not a panic");
    assert!(
        err.to_string().contains("modulus must be positive"),
        "int_rem(5,0) must fault with the positive-modulus message, got: {err}"
    );
}

// --- Pass 7: TEXT-path refusal wall (aj8d pass 7) ---
// Every row is REAL .emath source through check()/eval — the path users
// hit — asserting the TYPED diagnostic. Sema refusals assert E-TYPE-010/
// E-TYPE-012; runtime faults assert the exact EvalFault string. Never
// weakened, never turned into a pass.

/// Refuse wrong/literal/arity/overflow Field/GF primes from TEXT.
#[test]
fn aj8d_text_field_prime_refusals() {
    let rows: &[(&str, &str, &str)] = &[
        ("emath function r:\n    inputs:\n        x: GF<8>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "prime"),
        ("emath function r:\n    inputs:\n        x: Field<8>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "prime"),
        ("emath function r:\n    inputs:\n        x: GF<Int>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "LITERAL"),
        ("emath function r:\n    inputs:\n        x: GF<7, 2>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "exactly one"),
        ("emath function r:\n    inputs:\n        x: Field<99999999999999999999>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "exceeds the maximum"),
    ];
    for (src, code, frag) in rows {
        let errs = errors_of(&check(src));
        assert!(
            errs.iter().any(|e| e.contains(code) && e.contains(frag)),
            "text row `{src:?}` must refuse ({code} / `{frag}`); got: {errs:#?}"
        );
    }
}

/// Refuse wrong-arity composite types from TEXT (Option over/+Operation,
/// Result under-armed, Graph generic).
#[test]
fn aj8d_text_composite_arity_refusals() {
    let rows: &[(&str, &str, &str)] = &[
        ("emath function r:\n    inputs:\n        x: Option<Int, String>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "exactly one"),
        ("emath function r:\n    inputs:\n        x: Result<Int>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "exactly two"),
        ("emath function r:\n    inputs:\n        x: Graph<Int>\n    definitions:\n        t = 1.0\n", "E-TYPE-010", "admits no type arguments"),
    ];
    for (src, code, frag) in rows {
        let errs = errors_of(&check(src));
        assert!(
            errs.iter().any(|e| e.contains(code) && e.contains(frag)),
            "text row `{src:?}` must refuse ({code} / `{frag}`); got: {errs:#?}"
        );
    }
}

/// Carrier misuse is refused at SEMA (E-TYPE-012): option_is_some on an
/// F64 scalar, result_error_of on an Option carrier, and a mismatched
/// carrier kind in an unwrap_or default slot.
#[test]
fn aj8d_text_carrier_misuse_refused() {
    // option_is_some(5.0): the argument is a Float64 scalar, not an
    // Option carrier → typed sema refusal.
    let src = "emath function c:\n    definitions:\n        s = option_is_some(5.0)\n";
    let errs = errors_of(&check(src));
    assert!(
        errs.iter().any(|e| e.contains("E-TYPE-012") && e.contains("Option carrier")),
        "option_is_some(5.0) must refuse at sema (E-TYPE-012), got: {errs:#?}"
    );
    // result_error_of applied to an Option carrier → typed sema refusal.
    let src = "emath function c:\n    definitions:\n        e = result_error_of(option_some(1.0))\n";
    let errs = errors_of(&check(src));
    assert!(
        errs.iter().any(|e| e.contains("E-TYPE-012") && e.contains("Result carrier")),
        "result_error_of(option_some(..)) must refuse at sema, got: {errs:#?}"
    );
    // Mismatched carrier kinds in unwrap_or: an Option carrier used as
    // the default of a Result unwrap_or → typed kind-confusion refusal.
    let src = "emath function c:\n    definitions:\n        u = result_unwrap_or(result_ok(1.0), option_some(2.0))\n";
    let errs = errors_of(&check(src));
    assert!(
        errs.iter().any(|e| e.contains("E-TYPE-012") && e.contains("kind-matched")),
        "result_unwrap_or with an Option default must refuse (kind-matched), got: {errs:#?}"
    );
}

/// int_rem misuse: right arity but non-whole f64 divisor faults TYPED at
/// interp (finite_whole_i64 → TypeConfusion, never a panic); wrong arity
/// refuses at sema.
#[test]
fn aj8d_text_int_rem_misuse_refusals() {
    // int_rem(5.5, 2): sema admits (both F64), the runtime i64_of refuses
    // the non-whole 5.5 as a typed TypeConfusion — never a silent 5 or a
    // panic.
    let src = "emath function c:\n    inputs:\n        a: Float64\n        m: Int\n    outputs:\n        o: Float64\n    definitions:\n        o = int_rem(a, m)\n";
    let r = check(src);
    assert!(
        errors_of(&r).is_empty(),
        "int_rem(5.5, 2) must admit at sema (fault is runtime); got: {:?}",
        errors_of(&r)
    );
    let err = eval_definitions_values(
        &r.package,
        &r.package.declarations[0],
        &BTreeMap::from([("a".into(), Value::F64(5.5)), ("m".into(), Value::I64(2))]),
        &BTreeMap::new(),
    )
    .expect_err("int_rem(5.5, 2) must be a typed runtime fault, not a panic");
    assert!(
        err.to_string().contains("type confusion"),
        "int_rem(5.5, 2) must fault with the i64 TypeConfusion message, got: {err}"
    );
    // int_rem(5) arity → sema arity refusal.
    let src = "emath function c:\n    definitions:\n        o = int_rem(5)\n";
    let errs = errors_of(&check(src));
    assert!(
        errs.iter().any(|e| e.contains("int_rem") && e.contains("argument")),
        "int_rem(5) must refuse as an arity error, got: {errs:#?}"
    );
}

// NOTE (aj8d pass 7): a `Field<7>` output fed a FLOAT definition (c = 1.5)
// currently ADMITS — F64 numerically widens to the Int that FieldPrime
// infers as. Pinning a refusal here would require TypeNode access at the
// output-conformance site (crates/emath-sema/src/admit/declaration.rs,
// non-reserved this pass), so this is reported as a real gap, not tested
// with a lying assertion.

// --- Pass 8: FieldPrime float-exactness conformance (aj8d pass 8) ---
// declaration.rs now guards the FieldPrime output against a FLOAT
// definition (F64 must not numerically widen into an exact integer field
// type), while plain Int keeps the legacy F64→Int widening. Three
// discriminating rows: FLOAT definition refuses; int_rem composition and
// an integer literal both ADMIT (valid exact field elements).
#[test]
fn aj8d_text_field_prime_exactness_conformance() {
    // Row 1 — float definition into Field<7> output: typed refusal.
    let src = "emath function f:\n    outputs:\n        c: Field<7>\n    definitions:\n        c = 1.5\n";
    let errs = errors_of(&check(src));
    assert!(
        errs.iter().any(|e| e.contains("E-TYPE-012") && e.contains("exact integer field element")),
        "a float definition in a Field<7> output must refuse (E-TYPE-012), got: {errs:#?}"
    );

    // Row 2 — int_rem composition into Field<7> output: ADMITS, exact 0.
    let src = "emath function f:\n    outputs:\n        c: Field<7>\n    definitions:\n        c = int_rem(3 + 4, 7)\n";
    let r = check(src);
    assert!(
        errors_of(&r).is_empty(),
        "int_rem(3+4,7) into Field<7> must ADMIT; got: {:?}",
        errors_of(&r)
    );
    let values = eval_definitions_values(
        &r.package,
        &r.package.declarations[0],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("Field<7> int_rem definition must evaluate");
    assert_eq!(values.get("c"), Some(&Value::I64(0)), "int_rem(3+4,7) = 0");

    // Row 3 — integer literal into Field<7> output: ADMITS (valid element).
    let src = "emath function f:\n    outputs:\n        c: Field<7>\n    definitions:\n        c = 3\n";
    let r = check(src);
    assert!(
        errors_of(&r).is_empty(),
        "integer literal 3 into Field<7> must ADMIT; got: {:?}",
        errors_of(&r)
    );
}

/// Metamorphic: field7 multiplication DISTRIBUTES over field7 addition in
/// GF<7> (capability-data field ops compose algebraically). For a,b,c:
///   (a+b)*c mod 7 == (a*c + b*c) mod 7
/// driven through real .emath declarations and the reference VM.
#[test]
fn aj8d_meta_field7_distribution_law() {
    let rows = [(3i64, 4, 5, 0i64), (1, 6, 2, 0), (5, 5, 3, 2)];
    for (a, b, c, want) in rows {
        let lhs = "emath function fa:\n    inputs:\n        a: Int\n        b: Int\n        c: Int\n    outputs:\n        l: Field<7>\n    definitions:\n        l = int_rem(int_rem(a + b, 7) * c, 7)\n";
        let l = text_values_at(
            lhs,
            0,
            BTreeMap::from([
                ("a".into(), Value::I64(a)),
                ("b".into(), Value::I64(b)),
                ("c".into(), Value::I64(c)),
            ]),
        );
        let rhs = "emath function fb:\n    inputs:\n        a: Int\n        b: Int\n        c: Int\n    outputs:\n        r: Field<7>\n    definitions:\n        r = int_rem(int_rem(a * c, 7) + int_rem(b * c, 7), 7)\n";
        let r = text_values_at(
            rhs,
            0,
            BTreeMap::from([
                ("a".into(), Value::I64(a)),
                ("b".into(), Value::I64(b)),
                ("c".into(), Value::I64(c)),
            ]),
        );
        let lv = l.get("l");
        let rv = r.get("r");
        assert_eq!(
            lv, rv,
            "distribution: (a+b)*c mod 7 must equal a*c+b*c mod 7 for a={a},b={b},c={c}; got {lv:?} vs {rv:?}"
        );
        assert_eq!(
            lv,
            Some(&Value::I64(want)),
            "row {a},{b},{c} must be I64-exact {want}"
        );
    }
}
