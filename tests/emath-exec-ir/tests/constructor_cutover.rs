//! Constructor-layer acceptance pack T1–T18 and seam checks.
//!
//! Each fixture states that it installed no topic identity. These tests
//! discriminate the constitution observations, not historical FeatureIDs.

use std::collections::BTreeMap;
use std::path::PathBuf;

use emath_exec_ir::constructor_emir::{cvalue_to_emir, lower_constructor_function, values_equal};
use emath_exec_ir::constructor_layer::{
    evaluate_function, evaluate_function_budgeted, evaluate_query, evaluate_tree,
    evaluate_tree_at, CValue,
};
use emath_exec_ir::interp::{evaluate, Value};
use emath_exec_ir::language_image::{compile_language_directory, write_language_distribution};
use emath_schema::rewrite_declared_semantic_hashes;
use emath_syntax::parse_str;
use emath_test_harness::Probe;

fn rewrite_constructor_hashes(spec: &std::path::Path) -> Result<(), String> {
    let mut paths = Vec::new();
    collect_emath(spec, &mut paths)?;
    paths.sort();
    for path in paths {
        let text = std::fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))?;
        let (rewritten, _) = rewrite_declared_semantic_hashes(&text)
            .map_err(|issue| format!("{}:{}: {}", path.display(), issue.line, issue.detail))?;
        if rewritten != text {
            std::fs::write(&path, rewritten).map_err(|err| format!("{}: {err}", path.display()))?;
        }
    }
    Ok(())
}

fn collect_emath(root: &std::path::Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(root).map_err(|err| format!("{}: {err}", root.display()))? {
        let path = entry.map_err(|err| err.to_string())?.path();
        if path.is_dir() {
            collect_emath(&path, output)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("emath") {
            output.push(path);
        }
    }
    Ok(())
}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/constructor")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn parse_ok(name: &str) -> emath_core::tree::SyntaxTree {
    let source = fixture(name);
    let (tree, diagnostics) = parse_str(&source);
    assert!(
        !diagnostics.has_errors(),
        "{name} parse errors: {diagnostics:?}"
    );
    tree
}

fn all_passed(name: &str) {
    let tree = parse_ok(name);
    let report = evaluate_tree(&tree).unwrap_or_else(|err| panic!("{name}: {err}"));
    for test in &report.tests {
        assert!(
            test.passed,
            "{name} example `{}` failed: {}",
            test.label, test.detail
        );
    }
    assert!(
        !report.tests.is_empty(),
        "{name} produced no authored tests"
    );
}

fn int(n: i128) -> CValue {
    CValue::Int(n.into())
}

fn rat(num: i128, den: i128) -> CValue {
    CValue::Rat {
        num: num.into(),
        den: den.into(),
    }
}

fn rec_field(value: CValue, name: &str) -> CValue {
    match value {
        CValue::Record { fields, .. } => fields.get(name).cloned().unwrap_or(CValue::Absent),
        other => other,
    }
}

fn unused_inputs() -> BTreeMap<String, CValue> {
    BTreeMap::from([("unused".into(), int(0))])
}

fn demand_values_equal(
    p: &mut Probe,
    name: impl Into<String>,
    emitted: &Value,
    expected: &CValue,
) {
    let name = name.into();
    match values_equal(emitted, expected) {
        Ok(equal) => {
            p.demand(name, equal, format!("{emitted:?} vs {expected:?}"));
        }
        Err(err) => {
            p.fail(name, err);
        }
    }
}

fn demand_emission_parity(
    p: &mut Probe,
    tree: &emath_core::tree::SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
) {
    let vm = match evaluate_function(tree, name, inputs) {
        Ok(value) => value,
        Err(err) => {
            p.fail(format!("{name}-vm"), err.to_string());
            return;
        }
    };
    match lower_constructor_function(tree, name) {
        Ok(lowered) => {
            p.demand(
                format!("{name}-runnable"),
                lowered.runnable,
                format!("{:?}", lowered.unresolved),
            );
            if !lowered.runnable {
                return;
            }
            let emir_inputs: Result<Vec<_>, _> = lowered
                .inputs
                .iter()
                .map(|input| {
                    let value = inputs.get(input).ok_or_else(|| format!("missing input `{input}`"))?;
                    cvalue_to_emir(value)
                })
                .collect();
            let emir_inputs = match emir_inputs {
                Ok(values) => values,
                Err(err) => {
                    p.fail(format!("{name}-inputs"), err);
                    return;
                }
            };
            match evaluate(&lowered.program, &emir_inputs, &[]) {
                Ok(emitted) => {
                    demand_values_equal(p, format!("{name}-vm-vs-emir"), &emitted, &vm);
                }
                Err(err) => {
                    p.fail(format!("{name}-emir"), format!("{err:?}"));
                }
            }
        }
        Err(err) => {
            p.fail(format!("{name}-lower"), err);
        }
    }
}

fn demand_emission_unresolved(
    p: &mut Probe,
    tree: &emath_core::tree::SyntaxTree,
    name: &str,
) {
    match lower_constructor_function(tree, name) {
        Ok(lowered) => {
            p.demand(
                format!("{name}-not-runnable"),
                !lowered.runnable,
                format!("{:?}", lowered.unresolved),
            );
            p.demand(
                format!("{name}-unresolved"),
                !lowered.unresolved.is_empty(),
                "unresolved symbolic code was marked empty",
            );
        }
        Err(err) => {
            p.fail(format!("{name}-lower"), err);
        }
    }
}

#[test]
fn fixtures_and_seams() {
    let mut probe = Probe::new(
        "constructor layer: T1–T18 compute constitution observations; no topic FeatureIDs",
    );
    probe.case("quotient-four-classes-and-failed-descent", |p| {
        all_passed("quotient.emath");
        let tree = parse_ok("quotient.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "class-count",
            evaluate_function(&tree, "class_count", &unused).unwrap(),
            int(4),
        );
        p.eq(
            "descent",
            evaluate_function(&tree, "descent_fails", &unused).unwrap(),
            CValue::Bool(true),
        );
        let receipt = evaluate_query(&tree, "ClassCount", &unused).unwrap();
        p.eq("partial", receipt.fulfillment.as_str(), "partial");
    });

    probe.case("conditioning-posterior-six-sevenths", |p| {
        all_passed("conditioning.emath");
        let tree = parse_ok("conditioning.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "posterior",
            evaluate_function(&tree, "posterior_second", &unused).unwrap(),
            rat(6, 7),
        );
        let receipt = evaluate_query(&tree, "ZeroNormalizer", &unused).unwrap();
        p.eq(
            "zero-normalizer",
            receipt.diagnostic_code.as_deref(),
            Some("method_unavailable"),
        );
    });

    probe.case("random-function-shared-versus-independent", |p| {
        all_passed("random_function.emath");
        let tree = parse_ok("random_function.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "shared",
            evaluate_function(&tree, "shared_agree", &unused).unwrap(),
            int(1),
        );
        p.eq(
            "independent",
            evaluate_function(&tree, "independent_agree", &unused).unwrap(),
            rat(1, 2),
        );
    });

    probe.case("integral-enclosure-contains-third", |p| {
        all_passed("integral.emath");
        let tree = parse_ok("integral.emath");
        let n = BTreeMap::from([("n".into(), int(100))]);
        let enclosure = evaluate_function(&tree, "enclosure", &n).unwrap();
        p.eq("lo", rec_field(enclosure.clone(), "lo"), rat(6567, 20000));
        p.eq("hi", rec_field(enclosure.clone(), "hi"), rat(6767, 20000));
        p.eq("width", rec_field(enclosure, "width"), rat(1, 100));
        let receipt = evaluate_query(&tree, "CertifiedSquare", &n).unwrap();
        p.eq("certified-partial", receipt.fulfillment.as_str(), "partial");
    });

    probe.case("singular-ordinary-refused-pv-partial", |p| {
        all_passed("singular.emath");
        let tree = parse_ok("singular.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "ordinary",
            evaluate_query(&tree, "OrdinaryIntegral", &unused)
                .unwrap()
                .fulfillment
                .as_str(),
            "unmet",
        );
        p.eq(
            "pv",
            evaluate_query(&tree, "PrincipalValue", &unused)
                .unwrap()
                .fulfillment
                .as_str(),
            "partial",
        );
    });

    probe.case("represented-real-equality-undecided", |p| {
        all_passed("represented_real.emath");
        let tree = parse_ok("represented_real.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "unmet",
            evaluate_query(&tree, "Equality", &unused)
                .unwrap()
                .fulfillment
                .as_str(),
            "unmet",
        );
    });

    probe.case("conjecture-two-witnesses-refute-uniqueness", |p| {
        all_passed("conjecture.emath");
        let tree = parse_ok("conjecture.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "witness-count",
            evaluate_function(&tree, "witnesses", &unused).unwrap(),
            int(2),
        );
        p.eq(
            "start0",
            evaluate_function(&tree, "start0", &unused).unwrap(),
            CValue::Bool(true),
        );
        p.eq(
            "start2",
            evaluate_function(&tree, "start2", &unused).unwrap(),
            CValue::Bool(true),
        );
    });

    probe.case("shape-changing-length", |p| {
        all_passed("shape.emath");
        let tree = parse_ok("shape.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "n",
            evaluate_function(&tree, "split_once", &unused).unwrap(),
            int(2),
        );
        p.eq(
            "abstract-open",
            evaluate_function(&tree, "unseal", &unused).unwrap(),
            int(4),
        );
        p.eq(
            "mismatch",
            evaluate_function(&tree, "mismatch", &unused)
                .unwrap_err()
                .code
                .as_str(),
            "invalid_index",
        );
    });

    probe.case("boundary-squared-zero", |p| {
        all_passed("boundary.emath");
        let tree = parse_ok("boundary.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "square",
            rec_field(evaluate_function(&tree, "boundary_square", &unused).unwrap(), "c0"),
            int(0),
        );
        p.eq(
            "broken",
            rec_field(evaluate_function(&tree, "broken_boundary", &unused).unwrap(), "c0"),
            int(-2),
        );
    });

    probe.case("jacobian-at-point", |p| {
        all_passed("jacobian.emath");
        let tree = parse_ok("jacobian.emath");
        let xy = BTreeMap::from([("x".into(), int(2)), ("y".into(), int(3))]);
        let fmap = evaluate_function(&tree, "fmap", &xy).unwrap();
        p.eq("f-u", rec_field(fmap.clone(), "u"), int(6));
        p.eq("f-v", rec_field(fmap, "v"), int(11));
        let jac = evaluate_function(&tree, "linearization", &xy).unwrap();
        p.eq("j00", rec_field(jac.clone(), "j00"), int(3));
        p.eq("j01", rec_field(jac.clone(), "j01"), int(2));
        p.eq("j10", rec_field(jac.clone(), "j10"), int(1));
        p.eq("j11", rec_field(jac, "j11"), int(6));
    });

    probe.case("opaque-keeps-code", |p| {
        all_passed("opaque.emath");
        let tree = parse_ok("opaque.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        let receipt = evaluate_query(&tree, "OpaqueTransform", &unused).unwrap();
        p.eq("code", receipt.representation.as_str(), "code");
        p.ne("not-inferred", receipt.fulfillment.as_str(), "satisfied-by-name");
        p.eq(
            "rule",
            evaluate_function(&tree, "try_diff", &unused)
                .unwrap_err()
                .code
                .as_str(),
            "transformation_rule_unavailable",
        );
    });

    probe.case("transport-states", |p| {
        all_passed("transport.emath");
        let tree = parse_ok("transport.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        let transport = evaluate_function(&tree, "two_steps", &unused).unwrap();
        p.eq("first-after-one", rec_field(transport.clone(), "a1"), rat(3, 4));
        p.eq("second-after-one", rec_field(transport.clone(), "a2"), rat(1, 4));
        p.eq("first-after-two", rec_field(transport.clone(), "b1"), rat(9, 16));
        p.eq("second-after-two", rec_field(transport.clone(), "b2"), rat(7, 16));
        p.eq("mass", rec_field(transport, "total"), int(1));
    });

    probe.case("queue-completions", |p| {
        all_passed("queue.emath");
        let tree = parse_ok("queue.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        let queue = evaluate_function(&tree, "completions", &unused).unwrap();
        p.eq("first-job", rec_field(queue.clone(), "c0"), int(2));
        p.eq("second-job", rec_field(queue.clone(), "c1"), int(3));
        p.eq("throughput", rec_field(queue, "throughput"), rat(2, 3));
    });

    probe.case("edges-empty-and-order", |p| {
        all_passed("edges.emath");
        let tree = parse_ok("edges.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "forall",
            evaluate_function(&tree, "empty_forall", &unused).unwrap(),
            CValue::Bool(true),
        );
        p.eq(
            "exists",
            evaluate_function(&tree, "empty_exists", &unused).unwrap(),
            CValue::Bool(false),
        );
        p.eq(
            "left-sub",
            evaluate_function(&tree, "left_sub", &unused).unwrap(),
            int(7),
        );
    });

    probe.case("scalar-carrier-joins", |p| {
        // Ordering is a checked carrier operation on every numeric carrier:
        // Float64 `<`/`<=`/`>`/`>=` must COMPUTE (the reference ships them),
        // and mixed exact/Float64 comparisons join exactly — a finite
        // binary64 IS an exact rational, so `1 / 2 == 0.5` is true and never
        // a rounded or representation-strict comparison.
        let ordering = r#"
emath function FltOrder:
    inputs:
        unused: Int
    outputs:
        a: Bool
        b: Bool
        c: Bool
        d: Bool
    definitions:
        a = (1.0) > (2.0)
        b = (2.0) > (1.0)
        c = (1.0) <= (1.0)
        d = (1.0e+300) > (1.0)
"#;
        let (tree, diagnostics) = parse_str(ordering);
        p.demand("flt-order-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        let vm = evaluate_function(&tree, "FltOrder", &unused).unwrap();
        p.eq("flt-gt-false", rec_field(vm.clone(), "a"), CValue::Bool(false));
        p.eq("flt-gt-true", rec_field(vm.clone(), "b"), CValue::Bool(true));
        p.eq("flt-le-equal", rec_field(vm.clone(), "c"), CValue::Bool(true));
        p.eq("flt-gt-scale", rec_field(vm, "d"), CValue::Bool(true));

        let joins = r#"
emath function CarrierJoins:
    inputs:
        unused: Int
    outputs:
        eq_ir: Bool
        eq_if: Bool
        eq_rf: Bool
        ne_ir: Bool
        ne_if: Bool
    definitions:
        eq_ir = 2 == 2 / 1
        eq_if = 2 == 2.0
        eq_rf = 1 / 2 == 0.5
        ne_ir = 2 == 3 / 1
        ne_if = 2 == 3.5
"#;
        let (tree, diagnostics) = parse_str(joins);
        p.demand("joins-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let vm = evaluate_function(&tree, "CarrierJoins", &unused).unwrap();
        p.eq("eq-int-rat", rec_field(vm.clone(), "eq_ir"), CValue::Bool(true));
        p.eq("eq-int-float", rec_field(vm.clone(), "eq_if"), CValue::Bool(true));
        p.eq("eq-rat-float", rec_field(vm.clone(), "eq_rf"), CValue::Bool(true));
        p.eq("ne-int-rat", rec_field(vm.clone(), "ne_ir"), CValue::Bool(false));
        p.eq("ne-int-float", rec_field(vm, "ne_if"), CValue::Bool(false));
    });

    probe.case("rat-annotation-exact-decimals", |p| {
        // Reference (types chapter): "A rational annotation selects `Rat`.
        // A decimal literal without a floating annotation denotes its exact
        // decimal rational." Under a `Rat` annotation `0.1 + 0.2` is exactly
        // `3/10` — never the binary64 sum. Suffixed literals keep Float64
        // and refuse the Rat output. The rewrite is annotation-directed: a
        // `Float64` annotation keeps the rounded carrier.
        let dec = r#"
emath function DecRat:
    inputs:
        unused: Int
    outputs:
        a: Rat
        b: Rat
        c: Rat
        d: Rat
        e: Rat
    definitions:
        a = 1.5
        b = 0.1 + 0.2
        c = 1.5 * 3
        d = 1.5e+2
        e = -2.5
"#;
        let (tree, diagnostics) = parse_str(dec);
        p.demand("decrat-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        match evaluate_function(&tree, "DecRat", &unused_inputs()) {
            Ok(vm) => {
                p.eq("one-point-five", rec_field(vm.clone(), "a"), rat(3, 2));
                p.eq("tenths-exact", rec_field(vm.clone(), "b"), rat(3, 10));
                p.eq("times-three", rec_field(vm.clone(), "c"), rat(9, 2));
                p.eq("exponent-exact", rec_field(vm.clone(), "d"), rat(150, 1));
                p.eq("negated", rec_field(vm, "e"), rat(-5, 2));
            }
            Err(err) => {
                p.fail("decrat-eval", err.to_string());
            }
        }

        // A suffixed literal under a Rat output is a typed refusal: the
        // floating annotation is explicit, so no silent coercion.
        let suffixed = r#"
emath function DecSuffix:
    inputs:
        unused: Int
    outputs:
        r: Rat
    definitions:
        r = 1.5f64
"#;
        let (tree, diagnostics) = parse_str(suffixed);
        p.demand("suffix-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let refused = emath_exec_ir::constructor_layer::admit_tree(&tree);
        p.demand(
            "suffix-refused",
            refused.is_err(),
            format!("f64 literal under Rat output admitted: {refused:?}"),
        );

        // A Float64 annotation is untouched: bare decimals keep the
        // binary64 carrier (0.1 + 0.2 rounds).
        let f64_lane = r#"
emath function DecF64:
    inputs:
        unused: Int
    outputs:
        r: Float64
    definitions:
        r = 0.1 + 0.2
"#;
        let (tree, diagnostics) = parse_str(f64_lane);
        p.demand("decf64-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        match evaluate_function(&tree, "DecF64", &unused_inputs()) {
            Ok(vm) => {
                p.eq("f64-rounds", rec_field(vm, "r"), CValue::Float64(0.1 + 0.2));
            }
            Err(err) => {
                p.fail("decf64-eval", err.to_string());
            }
        }
    });

    probe.case("user-bound-names-win", |p| {
        // A name the user binds is the user's: a local closure named like a
        // module method (`partial`, `derivative`, `sin`) resolves to the
        // user's closure at admission AND evaluation. The reserved-recipe
        // refusal exists for UNBOUND name UX, not to seize user spellings.
        let bound = r#"
emath function BoundNames:
    inputs:
        x: Rat
    outputs:
        a: Rat
        b: Rat
        c: Rat
    definitions:
        partial = function y in Rat: y * 2
        derivative = function y in Rat: y + 1
        sin = function y in Rat: y
        a = partial(x)
        b = derivative(x)
        c = sin(x)
"#;
        let (tree, diagnostics) = parse_str(bound);
        p.demand("bound-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let x_two = BTreeMap::from([("x".into(), rat(2, 1))]);
        match evaluate_function(&tree, "BoundNames", &x_two) {
            Ok(vm) => {
                // Integral Rat x Int arithmetic canonicalizes to Int (the
                // machine's exact join); the point here is NAME ownership.
                p.eq("partial-is-users", rec_field(vm.clone(), "a"), int(4));
                p.eq("derivative-is-users", rec_field(vm.clone(), "b"), int(3));
                p.eq("sin-is-users", rec_field(vm.clone(), "c"), rat(2, 1));
            }
            Err(err) => {
                p.fail("bound-eval", err.to_string());
            }
        }
        // Admission agrees: the declaration must admit (an unbound recipe
        // call still refuses method_unavailable — pinned in the corpus).
        let admitted = emath_exec_ir::constructor_layer::admit_tree(&tree);
        p.demand(
            "bound-admitted",
            admitted.is_ok(),
            format!("user-bound recipe names refused: {admitted:?}"),
        );
    });

    probe.case("recur-depth-faults-not-crashes", |p| {
        // The call-depth budget (256) must fire as `recursion_depth_exceeded`
        // BEFORE native stack exhaustion: at the observed ~50 KiB per authored
        // call the default 8 MiB stack dies near 200 frames. The pin evaluates
        // on a 64 MiB stack exactly like the CLI worker thread, so a mutant
        // that drops the depth guard crashes this pin instead of passing it.
        // The self-call is deliberately NON-tail (nested inside an addition):
        // tail-position closure self-calls reuse the frame and are bounded by
        // WORK instead (see `recur-tail-scales-nontail-caps`), so the depth
        // cap is pinned on the shape that still nests.
        let depth = r#"
emath function DepthFault:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = (recur f in Int -> Int: function k in Int: if k <= 0: 0 else: f(k + 1) + 0)(n)
"#;
        let (tree, diagnostics) = parse_str(depth);
        p.demand("depth-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let worker = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                evaluate_function(
                    &tree,
                    "DepthFault",
                    &BTreeMap::from([("n".into(), int(1))]),
                )
            })
            .expect("spawn depth probe thread");
        match worker.join().expect("join depth probe thread") {
            Ok(_) => {
                p.fail("depth-fault", "growing self-call returned a value");
            }
            Err(err) => {
                p.demand(
                    "depth-fault",
                    err.code == "recursion_depth_exceeded",
                    format!("growing self-call faulted `{}`", err.code),
                );
            }
        }
    });

    probe.case("library-depth-faults-not-crashes", |p| {
        // Library callers embed the engine on ordinary threads — no 64 MiB
        // worker to hide behind. The growth redline must exceed one
        // authored call's native frame cost (~50-100 KiB across the
        // K-machine frames between growth points); with the old 64 KiB
        // redline a default 2 MiB thread hit the guard page at ~40 levels
        // and aborted the process. The fault, not the crash, is the
        // contract. As above, the self-call is deliberately NON-tail —
        // tail-position closure self-calls are work-bounded instead.
        let depth = r#"
emath function DepthFault:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = (recur f in Int -> Int: function k in Int: if k <= 0: 0 else: f(k + 1) + 0)(n)
"#;
        let (tree, diagnostics) = parse_str(depth);
        p.demand("library-depth-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let worker = std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || {
                evaluate_function(
                    &tree,
                    "DepthFault",
                    &BTreeMap::from([("n".into(), int(1))]),
                )
            })
            .expect("spawn library depth probe thread");
        match worker.join().expect("join library depth probe thread") {
            Ok(_) => {
                p.fail(
                    "library-depth-fault",
                    "growing self-call returned a value on a 2 MiB thread",
                );
            }
            Err(err) => {
                p.demand(
                    "library-depth-fault",
                    err.code == "recursion_depth_exceeded",
                    format!("growing self-call faulted `{}`", err.code),
                );
            }
        }
    });

    probe.case("search-fair-finds-sequential-unmet", |p| {
        all_passed("search.emath");
        let tree = parse_ok("search.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "fair",
            evaluate_function(&tree, "fair_finds", &unused).unwrap(),
            CValue::Bool(true),
        );
        p.eq(
            "sequential",
            evaluate_query(&tree, "SequentialFirst", &unused)
                .unwrap()
                .fulfillment
                .as_str(),
            "unmet",
        );
    });

    probe.case("zeno-unfinished", |p| {
        all_passed("zeno.emath");
        let tree = parse_ok("zeno.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "zeno",
            evaluate_query(&tree, "Zeno", &unused)
                .unwrap()
                .fulfillment
                .as_str(),
            "unmet",
        );
        p.eq(
            "prefix",
            evaluate_query(&tree, "FinitePrefix", &unused)
                .unwrap()
                .fulfillment
                .as_str(),
            "unmet",
        );
    });

    probe.case("approx-candidate-is-partial", |p| {
        all_passed("approx.emath");
        let tree = parse_ok("approx.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        let receipt = evaluate_query(&tree, "ApproxNotSolution", &unused).unwrap();
        p.eq("partial", receipt.fulfillment.as_str(), "partial");
        p.demand(
            "global-bound-remaining",
            receipt
                .remaining
                .iter()
                .any(|item| item.contains("global bound")),
            format!("remaining={:?}", receipt.remaining),
        );
    });

    probe.case("code-versus-value", |p| {
        all_passed("code.emath");
        let tree = parse_ok("code.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "code",
            evaluate_query(&tree, "CodeRequest", &unused)
                .unwrap()
                .fulfillment
                .as_str(),
            "satisfied",
        );
        p.eq(
            "value",
            evaluate_query(&tree, "ValueRequest", &unused)
                .unwrap()
                .fulfillment
                .as_str(),
            "unmet",
        );
    });

    probe.case("float64-no-reassociation", |p| {
        let source = r#"
emath function assoc:
    inputs:
        unused: Int
    outputs:
        left: Float64
        right: Float64
    definitions:
        a = 10000000000000000.0f64
        b = -10000000000000000.0f64
        c = 1.0f64
        left = (a + b) + c
        right = a + (b + c)
"#;
        let (tree, diagnostics) = parse_str(source);
        p.demand("parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        let assoc = evaluate_function(&tree, "assoc", &unused).unwrap();
        p.eq("left-group-is-one", rec_field(assoc.clone(), "left"), CValue::Float64(1.0));
        p.eq("right-group-is-zero", rec_field(assoc, "right"), CValue::Float64(0.0));
    });

    probe.case("checkpoint-resume-and-incompatible", |p| {
        let source = r#"
emath function walk:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = if n == 0: 0 else: 1 + walk(n - 1)
"#;
        let (tree, diagnostics) = parse_str(source);
        p.demand("parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let inputs = BTreeMap::from([("n".into(), int(20))]);
        let full = evaluate_function(&tree, "walk", &inputs).unwrap();
        let (err, checkpoint) =
            evaluate_function_budgeted(&tree, "walk", &inputs, 8, None, "walk-v1").unwrap_err();
        p.eq("suspended", err.code.as_str(), "budget_exhausted");
        p.demand(
            "walk-frames",
            !checkpoint.frames.is_empty()
                && checkpoint
                    .frames
                    .iter()
                    .any(|frame| frame.function == "walk"),
            format!("{:?}", checkpoint.frames.iter().map(|frame| &frame.function).collect::<Vec<_>>()),
        );
        p.eq(
            "continuation-abi",
            checkpoint.abi.as_str(),
            emath_exec_ir::constructor_layer::CHECKPOINT_ABI,
        );
        p.eq(
            "image-identity",
            checkpoint.image.as_str(),
            emath_exec_ir::constructor_layer::IMAGE_IDENTITY,
        );
        p.eq(
            "accounting",
            checkpoint.accounting.as_str(),
            emath_exec_ir::constructor_layer::ACCOUNTING_VERSION,
        );
        let packed = checkpoint.encode();
        p.demand("pack-image", packed.contains("image="), packed.clone());
        p.demand("pack-accounting", packed.contains("accounting="), packed.clone());
        p.demand("pack-remaining", packed.contains("remaining="), packed.clone());
        p.demand("pack-source-id", packed.contains("source_id="), packed.clone());
        p.demand("pack-pc", packed.contains(" pc="), packed);
        let resumed =
            evaluate_function_budgeted(&tree, "walk", &inputs, 1_000_000, Some(&checkpoint), "walk-v1")
                .unwrap();
        p.eq("resume-matches", resumed, full);
        let mut bad_image = checkpoint.clone();
        bad_image.image = "old-catalog".into();
        let (image_refuse, _) = evaluate_function_budgeted(
            &tree,
            "walk",
            &inputs,
            1_000_000,
            Some(&bad_image),
            "walk-v1",
        )
        .unwrap_err();
        p.eq(
            "old-image",
            image_refuse.code.as_str(),
            "incompatible_checkpoint",
        );
        let mut old_abi = checkpoint.clone();
        old_abi.abi = "constructor-layer/closure-abi".into();
        let (old_refuse, _) =
            evaluate_function_budgeted(&tree, "walk", &inputs, 1_000_000, Some(&old_abi), "walk-v1")
                .unwrap_err();
        p.eq(
            "old-abi",
            old_refuse.code.as_str(),
            "incompatible_checkpoint",
        );
        let mut bad = checkpoint.clone();
        bad.schema = "forged".into();
        let (refuse, _) =
            evaluate_function_budgeted(&tree, "walk", &inputs, 1_000_000, Some(&bad), "walk-v1")
                .unwrap_err();
        p.eq(
            "incompatible",
            refuse.code.as_str(),
            "incompatible_checkpoint",
        );
    });

    probe.case("checkpoint-nontail-tree", |p| {
        let source = r#"
emath function sum_tree:
    inputs:
        node: sequence
    outputs:
        result: Int
    definitions:
        result = if node.length == 1: node[0] else: sum_tree(node[0]) + sum_tree(node[1])
"#;
        let (tree, diagnostics) = parse_str(source);
        p.demand("parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let leaf = |n: i128| CValue::Sequence(std::sync::Arc::new(vec![int(n)]));
        let branch = |left, right| CValue::Sequence(std::sync::Arc::new(vec![left, right]));
        let node = branch(branch(leaf(1), leaf(2)), branch(leaf(3), leaf(4)));
        let inputs = BTreeMap::from([("node".into(), node)]);
        let full = evaluate_function(&tree, "sum_tree", &inputs).unwrap();
        p.eq("tree-sum", full.clone(), int(10));
        let mut checkpoint = None;
        for budget in (8..=96).step_by(4) {
            match evaluate_function_budgeted(&tree, "sum_tree", &inputs, budget, None, "tree-v1") {
                Err((err, next)) if err.code == "budget_exhausted" => checkpoint = Some(next),
                Ok(_) => break,
                Err((err, _)) => {
                    p.fail("tree-suspended", format!("{err}"));
                    return;
                }
            }
        }
        let Some(checkpoint) = checkpoint else {
            p.fail("tree-suspended", "tree completed before any budget stop");
            return;
        };
        p.demand(
            "tree-suspended",
            checkpoint.work > 0 && !checkpoint.frames.is_empty(),
            format!("work={} frames={}", checkpoint.work, checkpoint.frames.len()),
        );
        p.demand(
            "tree-frames",
            checkpoint.frames.len() >= 2
                && checkpoint
                    .frames
                    .iter()
                    .any(|frame| frame.function == "sum_tree"),
            format!(
                "frames={:?}",
                checkpoint
                    .frames
                    .iter()
                    .map(|frame| format!("{}@{}", frame.function, frame.pc))
                    .collect::<Vec<_>>()
            ),
        );
        let encoded = checkpoint.encode();
        p.demand(
            "tree-frame-bytes",
            encoded.contains("frame function=sum_tree") && encoded.contains("next="),
            encoded.lines().take(16).collect::<Vec<_>>().join("\n"),
        );
        p.demand(
            "tree-next",
            checkpoint.frames.iter().any(|frame| !frame.next.is_empty()),
            format!(
                "next={:?}",
                checkpoint.frames.iter().map(|frame| &frame.next).collect::<Vec<_>>()
            ),
        );
        p.demand(
            "tree-remaining-work-kont",
            checkpoint.frames.iter().any(|frame| {
                frame.kont.iter().any(|kont| {
                    matches!(
                        kont,
                        emath_exec_ir::constructor_layer::Kont::BinRight {
                            left: CValue::Int(_),
                            ..
                        }
                    )
                })
            }),
            format!(
                "kont={:?}",
                checkpoint
                    .frames
                    .iter()
                    .map(|frame| frame.kont.len())
                    .collect::<Vec<_>>()
            ),
        );
        let restored = emath_exec_ir::constructor_layer::Checkpoint::decode(&encoded)
            .expect("decode tree checkpoint");
        let resumed = match evaluate_function_budgeted(
            &tree,
            "sum_tree",
            &inputs,
            1_000_000,
            Some(&restored),
            "tree-v1",
        ) {
            Ok(value) => value,
            Err((err, _)) => {
                p.fail(
                    "tree-resume",
                    format!("{err}\n{}", restored.encode()),
                );
                return;
            }
        };
        p.eq("tree-resume", resumed, full.clone());
        let tight = evaluate_function_budgeted(
            &tree,
            "sum_tree",
            &inputs,
            checkpoint.work + 40,
            Some(&restored),
            "tree-v1",
        );
        p.demand(
            "tree-resume-tight",
            tight.as_ref().ok() == Some(&full),
            format!("{tight:?} work={}", checkpoint.work),
        );
    });

    probe.case("forged-checker-cannot-rewrite-host-receipt", |p| {
        let source = r#"
emath query Forged:
    inputs:
        unused: Int
    definitions:
        receipt = satisfied
        payload = 1
    question:
        target = payload
        scope = unused
        assumptions = []
    using:
        method = missing
    answer:
        form = value
        accept = unchecked
"#;
        let (tree, diagnostics) = parse_str(source);
        p.demand("parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        let receipt = evaluate_query(&tree, "Forged", &unused).unwrap();
        p.ne("host-not-satisfied", receipt.fulfillment.as_str(), "satisfied");
    });

    probe.case("image-is-constructors-only", |p| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let spec = root.join("spec").join("constructors");
        if let Err(err) = rewrite_constructor_hashes(&spec) {
            p.fail("rewrite-hashes", err);
            return;
        }
        match compile_language_directory(&root) {
            Ok(distribution) => {
                if let Err(err) = write_language_distribution(&root, &distribution) {
                    p.fail("write-image", format!("{err:?}"));
                    return;
                }
                let ids: Vec<String> = distribution
                    .capsules
                    .iter()
                    .map(|capsule| capsule.feature_id.to_string())
                    .collect();
                p.eq("capsule-count", ids.len(), 9usize);
                p.demand(
                    "no-recipe-binders",
                    ids.iter().all(|id| {
                        !id.contains("binder.sum")
                            && !id.contains("math.add")
                            && !id.contains("capability.calculus")
                    }),
                    format!("{ids:?}"),
                );
                p.demand(
                    "has-constructor-ids",
                    ["std.kind.object", "std.kind.function", "std.kind.query", "std.syntax.recur", "std.syntax.quote", "std.capability.scalar", "std.world.reference"]
                        .iter()
                        .all(|want| ids.iter().any(|id| id == want)),
                    format!("{ids:?}"),
                );
                let index = std::fs::read_to_string(root.join("generated/feature-index.md"))
                    .unwrap_or_default();
                p.demand(
                    "generated-index-cut",
                    index.contains("`std.kind.object`") && !index.contains("std.binder.sum"),
                    index.chars().take(200).collect::<String>(),
                );
                let tables = std::fs::read_to_string(root.join("generated/runtime-tables.lock"))
                    .unwrap_or_default();
                p.demand(
                    "no-alias-column",
                    !tables.contains("aliases="),
                    tables.lines().take(6).collect::<Vec<_>>().join("\n"),
                );
            }
            Err(err) => {
                p.fail("compile-image", format!("{err:?}"));
            }
        }
    });

    probe.case("emission-parity-and-unresolved-not-runnable", |p| {
        let square = r#"
emath function Square:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = x * x
"#;
        let (tree, diagnostics) = parse_str(square);
        p.demand("square-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let vm = evaluate_function(&tree, "Square", &BTreeMap::from([("x".into(), int(7))])).unwrap();
        p.eq("vm-square", vm.clone(), int(49));
        match lower_constructor_function(&tree, "Square") {
            Ok(lowered) => {
                p.demand("square-runnable", lowered.runnable, format!("{:?}", lowered.unresolved));
                match evaluate(&lowered.program, &[Value::I64(7)], &[]) {
                    Ok(emitted) => {
                        demand_values_equal(p, "vm-vs-emir", &emitted, &vm);
                    }
                    Err(err) => {
                        p.fail("emir-eval", format!("{err:?}"));
                    }
                }
                match emath_rust_backend::emit_constructor_program(&lowered.program, &lowered.inputs)
                {
                    Ok(rust) => {
                        p.demand("checked-mul", rust.contains("checked_mul"), rust.clone());
                        p.demand("no-f64-star", !rust.contains(" as f64"), rust);
                    }
                    Err(err) => {
                        p.fail("emit-rust", err.to_string());
                    }
                }
            }
            Err(err) => {
                p.fail("lower-square", err);
            }
        }

        let half = r#"
emath function Half:
    inputs:
        unused: Int
    outputs:
        result: Rat
    definitions:
        result = 2 / 4
"#;
        let (tree, diagnostics) = parse_str(half);
        p.demand("half-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let vm = evaluate_function(&tree, "Half", &BTreeMap::from([("unused".into(), int(0))])).unwrap();
        p.eq("vm-half", vm.clone(), rat(1, 2));
        match lower_constructor_function(&tree, "Half") {
            Ok(lowered) => match evaluate(&lowered.program, &[Value::I64(0)], &[]) {
                Ok(emitted) => {
                    demand_values_equal(p, "half-vm-vs-emir", &emitted, &vm);
                }
                Err(err) => {
                    p.fail("half-emir", format!("{err:?}"));
                }
            },
            Err(err) => {
                p.fail("lower-half", err);
            }
        }

        let quoted = r#"
emath function Form:
    inputs:
        unused: Int
    outputs:
        result: Code
    definitions:
        result = quote(unused + 1)
"#;
        let (tree, diagnostics) = parse_str(quoted);
        p.demand("quote-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        match lower_constructor_function(&tree, "Form") {
            Ok(lowered) => {
                p.demand("quote-not-runnable", !lowered.runnable, "quoted form was marked runnable");
                p.demand(
                    "quote-unresolved",
                    lowered.unresolved.iter().any(|item| item == "quote"),
                    format!("{:?}", lowered.unresolved),
                );
            }
            Err(err) => {
                p.fail("lower-quote", err);
            }
        }

        let recur = r#"
emath function RecurSum:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = (recur walk in Int -> Int: function k in Int: if k == 0: 0 else: 1 + walk(k - 1))(n)
"#;
        let (tree, diagnostics) = parse_str(recur);
        p.demand("recur-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let vm = evaluate_function(&tree, "RecurSum", &BTreeMap::from([("n".into(), int(5))])).unwrap();
        p.eq("vm-recur", vm.clone(), int(5));
        match lower_constructor_function(&tree, "RecurSum") {
            Ok(lowered) => {
                p.demand("recur-runnable", lowered.runnable, format!("{:?}", lowered.unresolved));
                match evaluate(&lowered.program, &[Value::I64(5)], &[]) {
                    Ok(emitted) => {
                        demand_values_equal(p, "recur-vm-vs-emir", &emitted, &vm);
                    }
                    Err(err) => {
                        p.fail("recur-emir", format!("{err:?}"));
                    }
                }
            }
            Err(err) => {
                p.fail("lower-recur", err);
            }
        }

        let fact = r#"
emath function RecurFactorial:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = (recur walk in Int -> Int: function k in Int: if k == 0: 1 else: k * walk(k - 1))(n)
"#;
        let (tree, diagnostics) = parse_str(fact);
        p.demand("fact-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let vm = evaluate_function(&tree, "RecurFactorial", &BTreeMap::from([("n".into(), int(5))])).unwrap();
        p.eq("vm-fact", vm.clone(), int(120));
        match lower_constructor_function(&tree, "RecurFactorial") {
            Ok(lowered) => {
                p.demand("fact-runnable", lowered.runnable, format!("{:?}", lowered.unresolved));
                match evaluate(&lowered.program, &[Value::I64(5)], &[]) {
                    Ok(emitted) => {
                        demand_values_equal(p, "fact-vm-vs-emir", &emitted, &vm);
                    }
                    Err(err) => {
                        p.fail("fact-emir", format!("{err:?}"));
                    }
                }
            }
            Err(err) => {
                p.fail("lower-fact", err);
            }
        }

        let fib = r#"
emath function Fib:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = if n <= 1: n else: Fib(n - 1) + Fib(n - 2)
"#;
        let (tree, diagnostics) = parse_str(fib);
        p.demand("fib-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "vm-fib",
            evaluate_function(&tree, "Fib", &BTreeMap::from([("n".into(), int(6))])).unwrap(),
            int(8),
        );
        match lower_constructor_function(&tree, "Fib") {
            Ok(lowered) => {
                p.demand("fib-runnable", lowered.runnable, format!("{:?}", lowered.unresolved));
                match evaluate(&lowered.program, &[Value::I64(6)], &[]) {
                    Ok(emitted) => {
                        demand_values_equal(p, "fib-vm-vs-emir", &emitted, &int(8));
                    }
                    Err(err) => {
                        p.fail("fib-emir", format!("{err:?}"));
                    }
                }
            }
            Err(err) => {
                p.fail("lower-fib", err);
            }
        }
    });

    probe.case("fixtures-emission-parity", |p| {
        let unused = unused_inputs();
        let quotient_tree = parse_ok("quotient.emath");
        demand_emission_parity(p, &quotient_tree, "class_count", &unused);
        demand_emission_parity(p, &quotient_tree, "descent_fails", &unused);

        let conditioning_tree = parse_ok("conditioning.emath");
        demand_emission_parity(p, &conditioning_tree, "posterior_second", &unused);

        let random_function_tree = parse_ok("random_function.emath");
        demand_emission_parity(p, &random_function_tree, "shared_agree", &unused);
        demand_emission_parity(p, &random_function_tree, "independent_agree", &unused);

        let integral_tree = parse_ok("integral.emath");
        let n = BTreeMap::from([("n".into(), int(100))]);
        demand_emission_parity(p, &integral_tree, "enclosure", &n);

        let conjecture_tree = parse_ok("conjecture.emath");
        demand_emission_parity(p, &conjecture_tree, "witnesses", &unused);
        demand_emission_parity(p, &conjecture_tree, "start0", &unused);

        let shape_tree = parse_ok("shape.emath");
        demand_emission_parity(p, &shape_tree, "split_once", &unused);
        demand_emission_parity(p, &shape_tree, "unseal", &unused);
        demand_emission_unresolved(p, &shape_tree, "mismatch");

        let boundary_tree = parse_ok("boundary.emath");
        demand_emission_parity(p, &boundary_tree, "boundary_square", &unused);
        demand_emission_parity(p, &boundary_tree, "broken_boundary", &unused);

        let jacobian_tree = parse_ok("jacobian.emath");
        let xy = BTreeMap::from([("x".into(), int(2)), ("y".into(), int(3))]);
        demand_emission_parity(p, &jacobian_tree, "fmap", &xy);
        demand_emission_parity(p, &jacobian_tree, "linearization", &xy);
        demand_emission_parity(p, &jacobian_tree, "at_point", &unused);

        let opaque_tree = parse_ok("opaque.emath");
        demand_emission_unresolved(p, &opaque_tree, "try_diff");

        let transport_tree = parse_ok("transport.emath");
        demand_emission_parity(p, &transport_tree, "two_steps", &unused);

        let queue_tree = parse_ok("queue.emath");
        demand_emission_parity(p, &queue_tree, "completions", &unused);

        let edges_tree = parse_ok("edges.emath");
        demand_emission_parity(p, &edges_tree, "empty_forall", &unused);
        demand_emission_parity(p, &edges_tree, "empty_exists", &unused);
        demand_emission_parity(p, &edges_tree, "left_sub", &unused);

        let search_tree = parse_ok("search.emath");
        demand_emission_parity(p, &search_tree, "fair_finds", &unused);

        let approx_tree = parse_ok("approx.emath");
        demand_emission_parity(p, &approx_tree, "candidate", &unused);
    });

    probe.case("constructor-check-unbound", |p| {
        let (tree, diagnostics) = parse_str(
            "emath function Bad:\n    inputs:\n        x: Int\n    outputs:\n        y: Int\n    definitions:\n        y = missing_name\n",
        );
        p.demand("parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let checked = emath_sema::admit::check_tree(&tree);
        p.demand(
            "unbound",
            checked.diagnostics.errors().any(|d| d.code == "E-TYPE-002"),
            format!("{:?}", checked.diagnostics.errors().collect::<Vec<_>>()),
        );
    });

    probe.case("constructor-check-if-bool", |p| {
        let (tree, diagnostics) = parse_str(
            "emath function BadIf:\n    inputs:\n        x: Int\n    outputs:\n        y: Int\n    definitions:\n        y = if x: 1 else: 0\n",
        );
        p.demand("parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let checked = emath_sema::admit::check_tree(&tree);
        p.demand(
            "if-not-bool",
            checked.diagnostics.errors().any(|d| d.code == "E-TYPE-012"),
            format!("{:?}", checked.diagnostics.errors().collect::<Vec<_>>()),
        );
    });

    probe.case("constructor-fixtures-admit", |p| {
        for name in [
            "quotient.emath",
            "conditioning.emath",
            "random_function.emath",
            "integral.emath",
            "singular.emath",
            "represented_real.emath",
            "conjecture.emath",
            "shape.emath",
            "boundary.emath",
            "jacobian.emath",
            "opaque.emath",
            "transport.emath",
            "queue.emath",
            "edges.emath",
            "search.emath",
            "zeno.emath",
            "approx.emath",
            "code.emath",
            "use_sum.emath",
        ] {
            let tree = parse_ok(name);
            let checked = emath_sema::admit::check_tree(&tree);
            let errors: Vec<_> = checked.diagnostics.errors().map(|d| format!("{} {}", d.code, d.message)).collect();
            p.demand(name, errors.is_empty(), errors.join("; "));
        }
    });

    probe.case("kind-model-is-gone", |p| {
        let source = "emath model Heat:\n    inputs:\n        x: Int\n";
        let (tree, diagnostics) = parse_str(source);
        p.demand("parsed-as-custom", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let err = evaluate_tree(&tree).unwrap_err();
        p.eq("gone", err.code.as_str(), "E-KIND-GONE");
        let (goals_tree, goals_diag) = parse_str(
            "emath function Square:\n    inputs:\n        x: Int\n    outputs:\n        y: Int\n    definitions:\n        y = x * x\n    goals:\n        evaluate <y>:\n            produce rust.library\n",
        );
        p.demand("goals-parsed", !goals_diag.has_errors(), format!("{goals_diag:?}"));
        let goals = evaluate_tree(&goals_tree).unwrap_err();
        p.eq("goals-gone", goals.code.as_str(), "E-SEC-101");
        let (bare, _) = parse_str("y = x * x\n");
        p.demand(
            "parse-no-scratch-wrap",
            !bare.items.iter().any(|item| {
                matches!(
                    item,
                    emath_core::tree::Item::Declaration(decl)
                        if decl.as_kind == "function"
                            || decl.sections().any(|section| section.name == "goals")
                )
            }),
            format!("parse must not synthesize a goals: function from bare text, got {bare:?}"),
        );

        let checked = emath_sema::admit::check_tree(&tree);
        p.demand(
            "sema-model-gone",
            checked.diagnostics.errors().any(|d| d.code == "E-KIND-GONE"),
            format!("{:?}", checked.diagnostics.errors().collect::<Vec<_>>()),
        );
        let (policy_tree, policy_diag) =
            parse_str("emath policy P:\n    inputs:\n        x: Int\n");
        p.demand("policy-parsed", !policy_diag.has_errors(), format!("{policy_diag:?}"));
        let policy = emath_sema::admit::check_tree(&policy_tree);
        p.demand(
            "sema-policy-gone",
            policy.diagnostics.errors().any(|d| d.code == "E-KIND-GONE"),
            format!("{:?}", policy.diagnostics.errors().collect::<Vec<_>>()),
        );
        let (function_tree, function_diag) = parse_str(
            "emath function AddExact:\n    inputs:\n        a: Int\n        b: Int\n    outputs:\n        result: Int\n    definitions:\n        result = a + b\n",
        );
        p.demand("function-parsed", !function_diag.has_errors(), format!("{function_diag:?}"));
        let function = emath_sema::admit::check_tree(&function_tree);
        p.demand(
            "sema-function-stays",
            !function
                .diagnostics
                .errors()
                .any(|d| d.code == "E-KIND-GONE" || d.code == "E-KIND-100"),
            format!("{:?}", function.diagnostics.errors().collect::<Vec<_>>()),
        );
        let add = function
            .package
            .declarations
            .iter()
            .find(|declaration| declaration.name.leaf() == "AddExact");
        p.eq(
            "function-inputs",
            add.map(|declaration| {
                declaration
                    .inputs
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect::<Vec<_>>()
            }),
            Some(vec!["a", "b"]),
        );
        p.eq(
            "function-outputs",
            add.map(|declaration| {
                declaration
                    .outputs
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect::<Vec<_>>()
            }),
            Some(vec!["result"]),
        );
        let (object_tree, object_diag) = parse_str(
            "emath object Point:\n    representation:\n        kind = \"package\"\n",
        );
        p.demand("object-parsed", !object_diag.has_errors(), format!("{object_diag:?}"));
        let object = emath_sema::admit::check_tree(&object_tree);
        p.demand(
            "sema-object-stays",
            !object
                .diagnostics
                .errors()
                .any(|d| d.code == "E-KIND-GONE" || d.code == "E-KIND-100"),
            format!("{:?}", object.diagnostics.errors().collect::<Vec<_>>()),
        );
        p.eq(
            "object-ir-kind",
            object
                .package
                .declarations
                .iter()
                .find(|declaration| declaration.name.leaf() == "Point")
                .map(|declaration| declaration.kind.leaf())
                .unwrap_or(""),
            "object",
        );
        let (query_tree, query_diag) = parse_str(
            "emath query Ask:\n    inputs:\n        unused: Int\n    question:\n        target = unused\n    answer:\n        form = value\n        accept = unchecked\n",
        );
        p.demand("query-parsed", !query_diag.has_errors(), format!("{query_diag:?}"));
        let query = emath_sema::admit::check_tree(&query_tree);
        p.demand(
            "sema-query-stays",
            !query
                .diagnostics
                .errors()
                .any(|d| d.code == "E-KIND-GONE" || d.code == "E-KIND-100"),
            format!("{:?}", query.diagnostics.errors().collect::<Vec<_>>()),
        );
        p.eq(
            "query-ir-kind",
            query
                .package
                .declarations
                .iter()
                .find(|declaration| declaration.name.leaf() == "Ask")
                .map(|declaration| declaration.kind.leaf())
                .unwrap_or(""),
            "query",
        );
        p.eq(
            "core-model-not-a-kind",
            emath_ir::KindSchema::core_model().name(),
            "",
        );
        p.eq(
            "core-policy-not-a-kind",
            emath_ir::KindSchema::core_policy().name(),
            "",
        );
        let (feature_tree, feature_diag) = parse_str(
            "emath feature Tool:\n    schema: \"emath.feature-capsule\"\n    feature_id: \"user.tool\"\n    class: \"capability\"\n    maturity: \"stable\"\n    summary: \"not a constructor\"\n    source: \"test\"\n    surface: \"none\"\n    semantics: \"none\"\n    exactness: \"none\"\n    effects: \"pure\"\n    worlds: \"std.world.reference\"\n    providers: \"n/a\"\n    artifacts: \"source\"\n    reference: \"authored\"\n    conformance: \"none\"\n    migration: \"gone\"\n    authority_target: \"capsule-active\"\n    presentation: \"none\"\n    agent: \"none\"\n",
        );
        p.demand("feature-parsed", !feature_diag.has_errors(), format!("{feature_diag:?}"));
        let feature = emath_sema::admit::check_tree(&feature_tree);
        p.demand(
            "sema-feature-gone",
            feature.diagnostics.errors().any(|d| d.code == "E-KIND-GONE"),
            format!("{:?}", feature.diagnostics.errors().collect::<Vec<_>>()),
        );
        let (reg_tree, reg_diag) = parse_str(
            "emath kind Foo:\n    extends function\nemath Foo Bar:\n    inputs:\n        x: Int\n    outputs:\n        y: Int\n    definitions:\n        y = x\n",
        );
        p.demand("kind-reg-parsed", !reg_diag.has_errors(), format!("{reg_diag:?}"));
        let registered = emath_sema::admit::check_tree(&reg_tree);
        p.demand(
            "kind-registry-gone",
            registered
                .diagnostics
                .errors()
                .any(|d| d.code == "E-KIND-GONE"),
            format!("{:?}", registered.diagnostics.errors().collect::<Vec<_>>()),
        );
        p.demand(
            "kind-app-not-admitted",
            !registered
                .package
                .declarations
                .iter()
                .any(|declaration| declaration.name.leaf() == "Bar"),
            format!("{:?}", registered.package.declarations),
        );
        let (eq_tree, eq_diag) = parse_str(
            "emath function Residual:\n    inputs:\n        x: Int\n    outputs:\n        y: Int\n    definitions:\n        y = x\n    equations:\n        y = x\n",
        );
        p.demand("eq-parsed", !eq_diag.has_errors(), format!("{eq_diag:?}"));
        let equations = emath_sema::admit::check_tree(&eq_tree);
        p.demand(
            "equations-not-constructor",
            equations
                .diagnostics
                .errors()
                .any(|d| d.code == "E-SEC-101"),
            format!("{:?}", equations.diagnostics.errors().collect::<Vec<_>>()),
        );
    });

    probe.case("native-simulate-gone", |p| {
        let package = emath_ir::SemanticPackage::new();
        let declaration = emath_ir::Declaration {
            id: emath_ir::DeclarationId(0),
            name: emath_core::QualifiedName::single("Heat"),
            kind: emath_core::QualifiedName::single("function"),
            kind_label: "function".into(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            state: Vec::new(),
            algebraic: Vec::new(),
            constructors: Vec::new(),
            definitions: BTreeMap::new(),
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: Vec::new(),
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: emath_core::Span::default(),
        };
        let empty = BTreeMap::new();
        let err = emath_exec_ir::simulate_continuous(
            &package,
            &declaration,
            &empty,
            &empty,
            0.0,
            1.0,
            0.1,
            emath_exec_ir::StepMethod::Rk4,
        )
        .unwrap_err();
        p.eq("sim-code", err.as_str(), emath_exec_ir::CONSTRUCTOR_SIMULATE_GONE);
        p.eq(
            "step-code",
            emath_exec_ir::step_continuous_values(
                &package,
                &declaration,
                &empty,
                &empty,
                0.1,
                emath_exec_ir::StepMethod::Euler,
            )
            .unwrap_err()
            .as_str(),
            emath_exec_ir::CONSTRUCTOR_SIMULATE_GONE,
        );
        p.eq(
            "explicit-prog",
            emath_exec_ir::runner::explicit_step_program(&package, &declaration, true, false)
                .unwrap_err()
                .as_str(),
            emath_exec_ir::CONSTRUCTOR_SIMULATE_GONE,
        );
        p.eq(
            "explicit-rate",
            emath_exec_ir::runner::explicit_rate_program(&package, &declaration)
                .unwrap_err()
                .as_str(),
            emath_exec_ir::CONSTRUCTOR_SIMULATE_GONE,
        );
        p.demand(
            "no-calculus-kernel",
            emath_exec_ir::native_kernel::native_kernel(
                "std.capability.calculus.residual-newton-solve",
            )
            .is_none(),
            "calculus kernel still dispatched",
        );
        p.demand(
            "no-gcd-kernel",
            emath_exec_ir::native_kernel::native_kernel("euclidean-remainder").is_none()
                && emath_exec_ir::native_kernel::native_kernel("checked-lcm").is_none(),
            "recipe kernels still dispatched",
        );
    });

    probe.case("leftover-apis-gone", |p| {
        let report = emath_exec_ir::runner::run_package(&emath_ir::SemanticPackage::new());
        p.eq("sir-refused", report.summary.refused, 1);
        p.demand(
            "sir-gone",
            report
                .declarations
                .iter()
                .any(|declaration| declaration.note.as_deref() == Some("E-KIND-GONE")),
            format!("{:?}", report.declarations),
        );
        let package = emath_ir::SemanticPackage::new();
        let backend = emath_rust_backend::BackendInput {
            package: &package,
            crate_name: "gone".into(),
            version: "0.1.0".into(),
        };
        match backend.generate() {
            Err(err) => {
                p.contains("generate-gone", &err.to_string(), "E-KIND-GONE");
            }
            Ok(_) => {
                p.fail("generate", "SIR generate must refuse E-KIND-GONE");
            }
        }
        let expansion = emath_syntax::expand_scratch("y = x * x\n");
        p.demand(
            "scratch-gone",
            expansion.diagnostics.errors().any(|item| item.code == "E-KIND-GONE"),
            format!("{:?}", expansion.diagnostics.errors().collect::<Vec<_>>()),
        );
        p.contains(
            "solve-gone",
            &emath_syntax::apply_solve_candidate("solve x^2 = 2", emath_syntax::SolveWorld::Numeric)
                .unwrap_err(),
            "E-KIND-GONE",
        );
        let (fn_tree, _) = parse_str(
            "emath function Square:\n    inputs:\n        x: Int\n    outputs:\n        y: Int\n    definitions:\n        y = x * x\n",
        );
        let checked = emath_sema::admit::check_tree(&fn_tree);
        let empty = BTreeMap::new();
        match emath_exec_ir::runner::eval_definitions_values(
            &checked.package,
            &checked.package.declarations[0],
            &empty,
            &empty,
        ) {
            Err(verdict) => {
                p.contains("defs-gone", &format!("{verdict:?}"), "E-KIND-GONE");
            }
            Ok(_) => {
                p.fail("defs", "SIR eval_definitions_values must refuse E-KIND-GONE");
            }
        }
        let dummy = emath_ir::ExprId(0);
        p.contains(
            "lower-def-gone",
            &emath_exec_ir::lower_definition(&package, dummy, &[], &[]).unwrap_err(),
            "E-KIND-GONE",
        );
        p.contains(
            "lower-req-gone",
            &emath_exec_ir::lower_requirement(&package, dummy, &[]).unwrap_err(),
            "E-KIND-GONE",
        );
    });

    probe.case("use-loads-ordinary-fold", |p| {
        all_passed("use_sum.emath");
        let tree = parse_ok("use_sum.emath");
        let unused = BTreeMap::from([("unused".into(), int(0))]);
        p.eq(
            "imported-sum",
            evaluate_function(&tree, "Total", &unused).unwrap(),
            int(10),
        );
    });

    probe.case("ordinary-integer-and-matrix-and-rk4-modules", |p| {
        let roots = emath_exec_ir::constructor_layer::module_roots_for(None);
        let gcd_path = emath_exec_ir::constructor_layer::resolve_module_path(
            &["exact".into(), "integers".into()],
            &roots,
        )
        .expect("integers module");
        let source = std::fs::read_to_string(&gcd_path).unwrap();
        let (tree, diagnostics) = parse_str(&source);
        p.demand("integers-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "gcd-48-18",
            evaluate_function(
                &tree,
                "gcd",
                &BTreeMap::from([("a".into(), int(48)), ("b".into(), int(18))]),
            )
            .unwrap(),
            int(6),
        );
        p.eq(
            "factorial-5",
            evaluate_function(&tree, "factorial", &BTreeMap::from([("n".into(), int(5))]))
                .unwrap(),
            int(120),
        );

        let mat_path = emath_exec_ir::constructor_layer::resolve_module_path(
            &["algebra".into(), "matmul".into()],
            &roots,
        )
        .expect("matmul module");
        let source = std::fs::read_to_string(&mat_path).unwrap();
        let (tree, diagnostics) = parse_str(&source);
        p.demand("matmul-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let a = CValue::Sequence(std::sync::Arc::new(vec![
            CValue::Sequence(std::sync::Arc::new(vec![int(1), int(2)])),
            CValue::Sequence(std::sync::Arc::new(vec![int(3), int(4)])),
        ]));
        let b = CValue::Sequence(std::sync::Arc::new(vec![
            CValue::Sequence(std::sync::Arc::new(vec![int(5), int(6)])),
            CValue::Sequence(std::sync::Arc::new(vec![int(7), int(8)])),
        ]));
        p.eq(
            "matmul-2x2",
            evaluate_function(
                &tree,
                "matmul",
                &BTreeMap::from([("a".into(), a), ("b".into(), b)]),
            )
            .unwrap(),
            CValue::Sequence(std::sync::Arc::new(vec![
                CValue::Sequence(std::sync::Arc::new(vec![int(19), int(22)])),
                CValue::Sequence(std::sync::Arc::new(vec![int(43), int(50)])),
            ])),
        );

        let rk_path = emath_exec_ir::constructor_layer::resolve_module_path(
            &["numerics".into(), "rk4".into()],
            &roots,
        )
        .expect("rk4 module");
        let source = std::fs::read_to_string(&rk_path).unwrap();
        let (tree, diagnostics) = parse_str(&source);
        p.demand("rk4-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "rk4-exp-one",
            evaluate_function(&tree, "rk4_exp_one", &BTreeMap::from([("unused".into(), int(0))]))
                .unwrap(),
            rat(65, 24),
        );
    });

    probe.case("checkpoint-roundtrip-and-list-emission", |p| {
        let source = r#"
emath function walk:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = if n == 0: 0 else: 1 + walk(n - 1)
"#;
        let (tree, diagnostics) = parse_str(source);
        p.demand("walk-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let inputs = BTreeMap::from([("n".into(), int(20))]);
        let (_, checkpoint) =
            evaluate_function_budgeted(&tree, "walk", &inputs, 8, None, "walk-v1").unwrap_err();
        let encoded = checkpoint.encode();
        p.demand(
            "checkpoint-header",
            emath_exec_ir::constructor_layer::is_constructor_checkpoint(&encoded),
            encoded.chars().take(80).collect::<String>(),
        );
        p.demand("roundtrip-image", encoded.contains("image=constructor-layer"), encoded.clone());
        p.demand("roundtrip-accounting", encoded.contains("accounting="), encoded.clone());
        p.demand("roundtrip-remaining", encoded.contains("remaining="), encoded.clone());
        p.demand("roundtrip-source-id", encoded.contains("source_id="), encoded.clone());
        p.demand("roundtrip-pc", encoded.contains(" pc="), encoded.clone());
        let restored = emath_exec_ir::constructor_layer::Checkpoint::decode(&encoded)
            .expect("decode checkpoint");
        p.eq("roundtrip-work", restored.work, checkpoint.work);
        let resumed =
            evaluate_function_budgeted(&tree, "walk", &inputs, 1_000_000, Some(&restored), "walk-v1")
                .unwrap();
        p.eq("roundtrip-resume", resumed, int(20));

        let listed = r#"
emath function Wrap:
    inputs:
        unused: Int
    outputs:
        result: sequence(Int)
    definitions:
        result = [1, 2, 3]
"#;
        let (tree, diagnostics) = parse_str(listed);
        p.demand("list-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let vm = evaluate_function(&tree, "Wrap", &BTreeMap::from([("unused".into(), int(0))]))
            .unwrap();
        p.eq(
            "vm-list",
            vm.clone(),
            CValue::Sequence(std::sync::Arc::new(vec![int(1), int(2), int(3)])),
        );
        match lower_constructor_function(&tree, "Wrap") {
            Ok(lowered) => {
                p.demand("list-runnable", lowered.runnable, format!("{:?}", lowered.unresolved));
                match evaluate(&lowered.program, &[Value::I64(0)], &[]) {
                    Ok(emitted) => {
                        demand_values_equal(p, "list-vm-vs-emir", &emitted, &vm);
                    }
                    Err(err) => {
                        p.fail("list-emir", format!("{err:?}"));
                    }
                }
            }
            Err(err) => {
                p.fail("lower-list", err);
            }
        }
    });

    probe.case("function-type-and-match-emission", |p| {
        let source = r#"
emath function apply:
    inputs:
        f: Int -> Int
        x: Int
    outputs:
        result: Int
    definitions:
        result = f(x)

emath function twice:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = apply(function k in Int: k + 1, 3)

emath function RecurTyped:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = (recur walk in Int -> Int: function k in Int: if k == 0: 0 else: 1 + walk(k - 1))(n)

emath function SignAbs:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = match n { 0 => 0, _ => 1 }
"#;
        let (tree, diagnostics) = parse_str(source);
        p.demand("fn-type-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "apply-plus-one",
            evaluate_function(&tree, "twice", &BTreeMap::from([("unused".into(), int(0))])).unwrap(),
            int(4),
        );
        p.eq(
            "fn-type-rejects-int",
            evaluate_function(
                &tree,
                "apply",
                &BTreeMap::from([("f".into(), int(1)), ("x".into(), int(3))]),
            )
            .unwrap_err()
            .code
            .as_str(),
            "type",
        );
        p.eq(
            "recur-arrow-type",
            evaluate_function(&tree, "RecurTyped", &BTreeMap::from([("n".into(), int(4))])).unwrap(),
            int(4),
        );
        p.eq(
            "match-zero",
            evaluate_function(&tree, "SignAbs", &BTreeMap::from([("n".into(), int(0))])).unwrap(),
            int(0),
        );
        p.eq(
            "match-nonzero",
            evaluate_function(&tree, "SignAbs", &BTreeMap::from([("n".into(), int(3))])).unwrap(),
            int(1),
        );
        match lower_constructor_function(&tree, "SignAbs") {
            Ok(lowered) => {
                p.demand("match-runnable", lowered.runnable, format!("{:?}", lowered.unresolved));
                match evaluate(&lowered.program, &[Value::I64(3)], &[]) {
                    Ok(emitted) => {
                        demand_values_equal(p, "match-emir", &emitted, &int(1));
                    }
                    Err(err) => {
                        p.fail("match-emir", format!("{err:?}"));
                    }
                }
            }
            Err(err) => {
                p.fail("lower-match", err);
            }
        }
    });

    probe.case("leftover-recipes-are-not-constructors", |p| {
        let deriv = r#"
emath function BadDeriv:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = derivative(x)
"#;
        let (tree, diagnostics) = parse_str(deriv);
        p.demand("deriv-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "deriv",
            evaluate_function(&tree, "BadDeriv", &BTreeMap::from([("x".into(), int(3))]))
                .unwrap_err()
                .code
                .as_str(),
            "method_unavailable",
        );

        let view = r#"
emath function ViewRecipes:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        dnode = quote.view(quote(derivative(unused)))
        pnode = quote.view(quote(partial(unused)))
        jnode = quote.view(quote(jacobian(unused)))
        snode = quote.view(quote(solve(unused)))
        result = if dnode.kind == Call and pnode.kind == Call and jnode.kind == Call and snode.kind == Call: 1 else: 0
"#;
        let (tree, diagnostics) = parse_str(view);
        p.demand("recipe-call-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "recipes-are-ordinary-calls",
            evaluate_function(&tree, "ViewRecipes", &BTreeMap::from([("unused".into(), int(0))]))
                .unwrap(),
            int(1),
        );

        let nabla = r#"
emath function BadNabla:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = ∇(unused, 1)
"#;
        let (_, diagnostics) = parse_str(nabla);
        p.demand(
            "nabla-not-a-constructor",
            diagnostics.has_errors(),
            format!("{diagnostics:?}"),
        );

        let graph = r#"
emath function BadGraph:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = graph { 1, 2 ; 1 --> 2 }
"#;
        let (_, diagnostics) = parse_str(graph);
        p.demand(
            "graph-not-a-constructor",
            diagnostics.has_errors(),
            format!("{diagnostics:?}"),
        );

        let fit = r#"
emath function BadFit:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        fit a to b:
            unused = unused
        result = 1
"#;
        let (_, diagnostics) = parse_str(fit);
        p.demand(
            "fit-not-a-constructor",
            diagnostics.has_errors(),
            format!("{diagnostics:?}"),
        );

        let fold = r#"
emath function BadSum:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = sum x in [1, 2, 3]: x
"#;
        let (tree, diagnostics) = parse_str(fold);
        p.demand("sum-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "sum-binder",
            evaluate_function(&tree, "BadSum", &BTreeMap::from([("unused".into(), int(0))]))
                .unwrap_err()
                .code
                .as_str(),
            "method_unavailable",
        );
        p.demand(
            "sum-is-callable-binder",
            tree.items.iter().any(|item| {
                matches!(item, emath_core::tree::Item::Declaration(decl) if decl
                    .sections()
                    .any(|section| {
                        section.suite.statements.iter().any(|stmt| {
                            matches!(
                                &stmt.kind,
                                emath_core::tree::StmtKind::Assign { value, .. }
                                    if matches!(value.kind, emath_core::tree::ExprKind::CallableBinder { .. })
                            )
                        })
                    }))
            }),
            "sum x in …: did not elaborate to CallableBinder",
        );

        let domain = r#"
emath function BadRecurTy:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = (recur walk in Int: function k in Int: if k == 0: 0 else: 1 + walk(k - 1))(n)
"#;
        let (tree, diagnostics) = parse_str(domain);
        p.demand("recur-ty-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "recur-domain-is-not-fn-type",
            evaluate_function(&tree, "BadRecurTy", &BTreeMap::from([("n".into(), int(2))]))
                .unwrap_err()
                .code
                .as_str(),
            "type",
        );

        let unguarded = r#"
emath function BadRecurBody:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = (recur walk in Int -> Int: 1)
"#;
        let (tree, diagnostics) = parse_str(unguarded);
        p.demand("unguarded-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "unguarded-recur",
            evaluate_function(&tree, "BadRecurBody", &BTreeMap::from([("unused".into(), int(0))]))
                .unwrap_err()
                .code
                .as_str(),
            "unguarded_recursive_binding",
        );
    });

    probe.case("quote-bind-fresh-tokens", |p| {
        let source = r#"
emath function rebound:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        form = quote.bind x in Int: quote.bind x in Int: x
        outer = quote.view(form)
        inner = quote.view(outer.body)
        result = if outer.param == inner.param: 0 else: 1

emath function subst_preserved:
    # Naming the root binder's OWN parameter substitutes nothing: the
    # parameter's occurrences are bound, not free (L2). The code is
    # preserved — identity is unchanged — not edited.
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        form = quote.bind x in Int: quote.bind y in Int: x + y
        node = quote.view(form)
        replaced = quote.substitute(form, node.param, 2)
        result = if quote.identity(replaced) == quote.identity(form): 1 else: 0

emath function applied_code:
    # Feeding a binder goes through evaluation and call, not
    # substitution: quote.evaluate returns the closure, the call
    # applies the argument.
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        program = quote.evaluate(quote(function j in Int: 5 + j))
        result = program(2)
"#;
        let (tree, diagnostics) = parse_str(source);
        p.demand("bind-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "distinct-tokens",
            evaluate_function(&tree, "rebound", &BTreeMap::from([("unused".into(), int(0))])).unwrap(),
            int(1),
        );
        p.eq(
            "subst-bound-param-preserves-code",
            evaluate_function(&tree, "subst_preserved", &BTreeMap::from([("unused".into(), int(0))])).unwrap(),
            int(1),
        );
        p.eq(
            "apply-idiom",
            evaluate_function(&tree, "applied_code", &BTreeMap::from([("unused".into(), int(0))])).unwrap(),
            int(7),
        );

        let open = r#"
emath function OpenChild:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        form = quote(3 + 4)
        node = quote.view(form)
        rebuilt = quote.open term in node.children[0]: quote.make(quote.view(term))
        result = quote.evaluate(rebuilt)
"#;
        let (tree, diagnostics) = parse_str(open);
        p.demand("open-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "open-literal",
            evaluate_function(&tree, "OpenChild", &BTreeMap::from([("unused".into(), int(0))])).unwrap(),
            int(3),
        );

        let viewed = r#"
emath function ViewBranchClosed:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        form = quote(if 0 == 0: 1 else: 2)
        node = quote.view(form)
        rebuilt = quote.make(node)
        result = quote.evaluate(rebuilt)

emath function ViewBranchOpen:
    # `unused` is a free name in the ambient environment: the guarded
    # L1 evaluator refuses instead of leaking the ambient binding
    # (l1_closed_code pins the same contract with fault demands).
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        form = quote(if unused == 0: 1 else: unused)
        node = quote.view(form)
        rebuilt = quote.make(node)
        result = quote.evaluate(rebuilt)
"#;
        let (tree, diagnostics) = parse_str(viewed);
        p.demand("view-if-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "view-if-closed",
            evaluate_function(&tree, "ViewBranchClosed", &BTreeMap::from([("unused".into(), int(0))])).unwrap(),
            int(1),
        );
        p.eq(
            "view-if-open-refuses",
            evaluate_function(&tree, "ViewBranchOpen", &BTreeMap::from([("unused".into(), int(0))]))
                .unwrap_err()
                .code
                .as_str(),
            "unbound_code",
        );
    });

    probe.case("closure-checkpoint-roundtrip", |p| {
        let source = r#"
emath function apply_twice:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        step = function k in Int: k + 1
        result = step(step(n))
"#;
        let (tree, diagnostics) = parse_str(source);
        p.demand("clos-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let inputs = BTreeMap::from([("n".into(), int(4))]);
        let full = evaluate_function(&tree, "apply_twice", &inputs).unwrap();
        p.eq("full", full.clone(), int(6));
        let (err, checkpoint) =
            evaluate_function_budgeted(&tree, "apply_twice", &inputs, 5, None, "clos-v1").unwrap_err();
        p.eq("suspended", err.code.as_str(), "budget_exhausted");
        let encoded = checkpoint.encode();
        p.demand(
            "encoded-closure",
            encoded.contains("(clos "),
            encoded.chars().take(200).collect::<String>(),
        );
        let restored = emath_exec_ir::constructor_layer::Checkpoint::decode(&encoded)
            .expect("decode closure checkpoint");
        let resumed =
            evaluate_function_budgeted(&tree, "apply_twice", &inputs, 1_000_000, Some(&restored), "clos-v1")
                .unwrap();
        p.eq("resume-closure", resumed, full);
    });

    probe.case("ordinary-combinatorics-and-euler", |p| {
        let roots = emath_exec_ir::constructor_layer::module_roots_for(None);
        let path = emath_exec_ir::constructor_layer::resolve_module_path(
            &["discrete".into(), "combinatorics".into()],
            &roots,
        )
        .expect("combinatorics module");
        let source = std::fs::read_to_string(&path).unwrap();
        let (tree, diagnostics) = parse_str(&source);
        p.demand("combinatorics-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "rising-3-3",
            evaluate_function(
                &tree,
                "rising_pochhammer",
                &BTreeMap::from([("x".into(), int(3)), ("k".into(), int(3))]),
            )
            .unwrap(),
            int(60),
        );
        p.eq(
            "falling-5-3",
            evaluate_function(
                &tree,
                "falling_factorial",
                &BTreeMap::from([("n".into(), int(5)), ("k".into(), int(3))]),
            )
            .unwrap(),
            int(60),
        );
        p.eq(
            "double-7",
            evaluate_function(&tree, "double_factorial", &BTreeMap::from([("n".into(), int(7))]))
                .unwrap(),
            int(105),
        );

        let euler_path = emath_exec_ir::constructor_layer::resolve_module_path(
            &["topology".into(), "euler".into()],
            &roots,
        )
        .expect("euler module");
        let source = std::fs::read_to_string(&euler_path).unwrap();
        let (tree, diagnostics) = parse_str(&source);
        p.demand("euler-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "cube-chi",
            evaluate_function(
                &tree,
                "euler_chi",
                &BTreeMap::from([
                    ("vertices".into(), int(8)),
                    ("edges".into(), int(12)),
                    ("faces".into(), int(6)),
                ]),
            )
            .unwrap(),
            int(2),
        );

        let int_path = emath_exec_ir::constructor_layer::resolve_module_path(
            &["exact".into(), "integers".into()],
            &roots,
        )
        .expect("integers module");
        let source = std::fs::read_to_string(&int_path).unwrap();
        let (tree, diagnostics) = parse_str(&source);
        p.demand("lcm-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "lcm-12-18",
            evaluate_function(
                &tree,
                "lcm",
                &BTreeMap::from([("a".into(), int(12)), ("b".into(), int(18))]),
            )
            .unwrap(),
            int(36),
        );

        let mod_path = emath_exec_ir::constructor_layer::resolve_module_path(
            &["exact".into(), "modular".into()],
            &roots,
        )
        .expect("modular module");
        let source = std::fs::read_to_string(&mod_path).unwrap();
        let (tree, diagnostics) = parse_str(&source);
        p.demand("modular-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "int-rem-10-3",
            evaluate_function(
                &tree,
                "int_rem",
                &BTreeMap::from([("value".into(), int(10)), ("modulus".into(), int(3))]),
            )
            .unwrap(),
            int(1),
        );
        p.eq(
            "mod-inv-3-7",
            evaluate_function(
                &tree,
                "mod_inv",
                &BTreeMap::from([("value".into(), int(3)), ("modulus".into(), int(7))]),
            )
            .unwrap(),
            int(5),
        );
        p.eq(
            "pow-mod-2-8-17",
            evaluate_function(
                &tree,
                "pow_mod",
                &BTreeMap::from([
                    ("base".into(), int(2)),
                    ("exponent".into(), int(8)),
                    ("modulus".into(), int(17)),
                ]),
            )
            .unwrap(),
            int(1),
        );
        p.eq(
            "poly-eval-mod",
            evaluate_function(
                &tree,
                "poly_eval_mod",
                &BTreeMap::from([
                    (
                        "coefficients".into(),
                        CValue::Sequence(std::sync::Arc::new(vec![int(1), int(2), int(3)])),
                    ),
                    ("point".into(), int(2)),
                    ("modulus".into(), int(17)),
                ]),
            )
            .unwrap(),
            int(0),
        );

        let bound_path = emath_exec_ir::constructor_layer::resolve_module_path(
            &["numerics".into(), "bounds".into()],
            &roots,
        )
        .expect("bounds module");
        let source = std::fs::read_to_string(&bound_path).unwrap();
        let (tree, diagnostics) = parse_str(&source);
        p.demand("bounds-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "amdahl-half-double",
            evaluate_function(
                &tree,
                "amdahl_speedup",
                &BTreeMap::from([("improved".into(), rat(1, 2)), ("factor".into(), rat(2, 1))]),
            )
            .unwrap(),
            rat(4, 3),
        );
        p.eq(
            "little-3-4",
            evaluate_function(
                &tree,
                "little_law",
                &BTreeMap::from([("arrival".into(), int(3)), ("sojourn".into(), int(4))]),
            )
            .unwrap(),
            int(12),
        );

        let crypto_path = emath_exec_ir::constructor_layer::resolve_module_path(
            &["cryptology".into(), "roundtrip".into()],
            &roots,
        )
        .expect("roundtrip module");
        let source = std::fs::read_to_string(&crypto_path).unwrap();
        let (tree, diagnostics) = parse_str(&source);
        p.demand("roundtrip-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        p.eq(
            "shift-roundtrip",
            evaluate_function(
                &tree,
                "shift_roundtrip",
                &BTreeMap::from([("message".into(), int(5)), ("key".into(), int(3))]),
            )
            .unwrap(),
            int(5),
        );

        let walk_path = emath_exec_ir::constructor_layer::resolve_module_path(
            &["quote".into(), "walk".into()],
            &roots,
        )
        .expect("walk module");
        let source = std::fs::read_to_string(&walk_path).unwrap();
        let (tree, diagnostics) = parse_str(&source);
        p.demand("walk-mod-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        let walk_src = r#"
use quote.walk

emath function IdWalk:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = quote.evaluate(walk_term(quote(1 + (2 + 3))))
"#;
        let (walk_tree, walk_diag) = parse_str(walk_src);
        p.demand("walk-use-parsed", !walk_diag.has_errors(), format!("{walk_diag:?}"));
        p.eq(
            "walk-nested-sum",
            evaluate_function(&walk_tree, "IdWalk", &BTreeMap::from([("unused".into(), int(0))]))
                .unwrap(),
            int(6),
        );
        let walk_if = r#"
use quote.walk

emath function WalkIfClosed:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = quote.evaluate(walk_term(quote(if 0 == 0: 1 else: 2)))

emath function WalkIfOpen:
    # Same guard as ViewBranchOpen: a free ambient name refuses
    # unbound_code under the guarded L1 evaluator.
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = quote.evaluate(walk_term(quote(if unused == 0: 1 else: unused)))
"#;
        let (if_tree, if_diag) = parse_str(walk_if);
        p.demand("walk-if-parsed", !if_diag.has_errors(), format!("{if_diag:?}"));
        p.eq(
            "walk-if-closed",
            evaluate_function(&if_tree, "WalkIfClosed", &BTreeMap::from([("unused".into(), int(0))]))
                .unwrap(),
            int(1),
        );
        p.eq(
            "walk-if-open-refuses",
            evaluate_function(&if_tree, "WalkIfOpen", &BTreeMap::from([("unused".into(), int(0))]))
                .unwrap_err()
                .code
                .as_str(),
            "unbound_code",
        );
        let walk_match = r#"
use quote.walk

emath function WalkMatchClosed:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = quote.evaluate(walk_term(quote(match 0 { 0 => 4, _ => 5 })))

emath function WalkMatchOpen:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = quote.evaluate(walk_term(quote(match unused { 0 => 4, _ => 5 })))
"#;
        let (match_tree, match_diag) = parse_str(walk_match);
        p.demand("walk-match-parsed", !match_diag.has_errors(), format!("{match_diag:?}"));
        p.eq(
            "walk-match-closed",
            evaluate_function(
                &match_tree,
                "WalkMatchClosed",
                &BTreeMap::from([("unused".into(), int(0))]),
            )
            .unwrap(),
            int(4),
        );
        p.eq(
            "walk-match-open-refuses",
            evaluate_function(
                &match_tree,
                "WalkMatchOpen",
                &BTreeMap::from([("unused".into(), int(0))]),
            )
            .unwrap_err()
            .code
            .as_str(),
            "unbound_code",
        );
        let _ = tree;
    });

    probe.case("use-seam-directory-relative-sibling", |p| {
        // The importer and its target sit side by side in the fixture
        // directory, outside language/modules. `use seam_target` must
        // resolve against the importing file's own directory.
        let importer_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/constructor/use_seam_importer.emath");
        let tree = parse_ok("use_seam_importer.emath");
        match evaluate_tree_at(&tree, Some(&importer_path)) {
            Ok(report) => {
                p.demand(
                    "sibling-import-admitted",
                    !report.tests.is_empty()
                        && report.tests.iter().all(|t| t.passed),
                    format!("{:?}", report.tests),
                );
            }
            Err(err) => {
                p.fail("sibling-import-admitted", err.to_string());
            }
        }
    });

    probe.case("use-seam-unresolvable-still-refuses", |p| {
        // A path that is neither a stdlib module nor a sibling file must
        // keep refusing E-USE-ADMISSION; the seam admits nothing new
        // beyond directory-relative files that exist.
        let importer_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/constructor/use_seam_importer.emath");
        let source = "use seam_no_such_file\n\nemath function unused_fn:\n    inputs:\n        unused: Int\n    outputs:\n        result: Int\n    definitions:\n        result = unused\n    tests:\n        example <pin>:\n            given unused = 0\n            expect result == 0\n";
        let (tree, diagnostics) = parse_str(source);
        p.demand("refusal-source-parsed", !diagnostics.has_errors(), format!("{diagnostics:?}"));
        match evaluate_tree_at(&tree, Some(&importer_path)) {
            Ok(report) => {
                p.fail(
                    "unresolvable-refuses",
                    format!("admitted anyway: {:?}", report.tests),
                );
            }
            Err(err) => {
                p.eq(
                    "unresolvable-refuses",
                    err.code.as_str(),
                    "E-USE-ADMISSION",
                );
            }
        }
    });

    probe.case("use-import-name-collision-refuses", |p| {
        // E-NAME-022 (user decision: collision is an error): an
        // imported declaration may not silently replace a same-named
        // local one. Historical behavior was the clobber — imports
        // installed after locals, so the local function ran the
        // imported body mid-module.
        let importer_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/constructor/use_import_clobber.emath");
        let tree = parse_ok("use_import_clobber.emath");
        match evaluate_tree_at(&tree, Some(&importer_path)) {
            Ok(report) => {
                p.fail(
                    "collision-refuses",
                    format!("admitted anyway: {:?}", report.tests),
                );
            }
            Err(err) => {
                p.eq("collision-refuses", err.code.as_str(), "E-NAME-022");
            }
        }
    });

    probe.case("use-import-same-source-idempotent", |p| {
        // The transitive diamond: dup_a and dup_b both import
        // dup_target, and dup_root imports both. The same source file
        // installs twice under the same bare name — idempotent, NOT a
        // collision (the `use numerics.experiment` -> bounds/powers
        // -> exact.integers shape every stdlib import relies on).
        let importer_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/constructor/dup_root.emath");
        let tree = parse_ok("dup_root.emath");
        match evaluate_tree_at(&tree, Some(&importer_path)) {
            Ok(report) => {
                p.demand(
                    "diamond-admits",
                    !report.tests.is_empty()
                        && report.tests.iter().all(|t| t.passed),
                    format!("{:?}", report.tests),
                );
            }
            Err(err) => {
                p.fail("diamond-admits", err.to_string());
            }
        }
    });

    probe.case("use-import-live-egcd-repro-refuses", |p| {
        // The live seam that bit cryptology.modular: `use
        // exact.integers` exports `egcd` (outputs g, s, t); a local
        // `egcd` must refuse instead of silently running the imported
        // one. Pinned against the real stdlib tree.
        let importer_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/constructor/use_import_clobber.emath");
        let source = "use exact.integers\n\nemath function egcd:\n    inputs:\n        a: Int\n        b: Int\n    outputs:\n        result: Int\n    definitions:\n        result = quot_int(a, b)\n    tests:\n        example <pin>:\n            given a = 7\n            given b = 2\n            expect result == 3\n";
        let (tree, diagnostics) = parse_str(source);
        p.demand(
            "repro-source-parsed",
            !diagnostics.has_errors(),
            format!("{diagnostics:?}"),
        );
        match evaluate_tree_at(&tree, Some(&importer_path)) {
            Ok(report) => {
                p.fail(
                    "live-repro-refuses",
                    format!("admitted anyway: {:?}", report.tests),
                );
            }
            Err(err) => {
                p.eq("live-repro-refuses", err.code.as_str(), "E-NAME-022");
            }
        }
    });

    probe.case("named-fn-closure-arg-coerces", |_p| {
        // A named top-level function in value position coerces to a
        // closure over its inputs: applying the closure calls the
        // function. Historical behavior: the bare name yielded an
        // unapplied closure value (calling it returned the closure
        // itself). The wide two-input shape refuses `arity` at
        // application time — never a silently wrong body.
        // `all_passed` panics with the failing observation on any
        // regression.
        all_passed("named_fn_closure_arg.emath");
        all_passed("named_fn_wide_arg.emath");
    });

    probe.case("recur-tail-scales-nontail-caps", |p| {
        // The tail-call seam for CLOSURE applications: a self-call in
        // tail position reuses the application frame — the call depth
        // stays at the entry value and WORK is the only bound — so a
        // tail-recursive closure builder scales past the 256 depth cap
        // (10_000 steps below; the base value 7 is reachable only by
        // walking every step). A NON-tail self-call (nested inside an
        // operation) keeps the configured depth cap. A GROWING tail
        // loop is bounded by the work budget — a fault, never a crash
        // or a hang — pinned on a 2 MiB library thread so a mutant
        // that reintroduces nesting aborts there instead of passing.
        let source = r#"
emath function TailWalk:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = (recur go in Int -> Int: function k in Int: if k <= 0: 7 else: go(k - 1))(n)

emath function NonTailWalk:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = (recur go in Int -> Int: function k in Int: if k <= 0: 7 else: 1 + go(k - 1))(n)

emath function GrowTail:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = (recur go in Int -> Int: function k in Int: if k <= 0: 7 else: go(k + 1))(n)

emath function NamedTailWalk:
    inputs:
        n: Int
        acc: Int
    outputs:
        result: Int
    definitions:
        result = if n == 0: acc else: NamedTailWalk(n - 1, acc + 1)
"#;
        let (tree, diagnostics) = parse_str(source);
        p.demand(
            "seam-parsed",
            !diagnostics.has_errors(),
            format!("{diagnostics:?}"),
        );
        match evaluate_function(&tree, "TailWalk", &BTreeMap::from([("n".into(), int(10_000))])) {
            Ok(value) => {
                p.eq("tail-scales", value, int(7));
            }
            Err(err) => {
                p.fail("tail-scales", err.to_string());
            }
        }
        match evaluate_function(&tree, "NonTailWalk", &BTreeMap::from([("n".into(), int(300))])) {
            Ok(_) => {
                p.fail("nontail-caps", "300-level non-tail walk returned a value");
            }
            Err(err) => {
                p.demand(
                    "nontail-caps",
                    err.code == "recursion_depth_exceeded",
                    format!("non-tail walk faulted `{}`", err.code),
                );
            }
        }
        match evaluate_function(&tree, "NonTailWalk", &BTreeMap::from([("n".into(), int(250))])) {
            Ok(value) => {
                p.eq("nontail-under-cap-computes", value, int(257));
            }
            Err(err) => {
                p.fail("nontail-under-cap-computes", err.to_string());
            }
        }
        let grow_tree = tree.clone();
        let worker = std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || {
                evaluate_function(&grow_tree, "GrowTail", &BTreeMap::from([("n".into(), int(1))]))
            })
            .expect("spawn grow probe thread");
        match worker.join().expect("join grow probe thread") {
            Ok(_) => {
                p.fail("grow-tail-work-bounded", "growing tail loop returned a value");
            }
            Err(err) => {
                p.demand(
                    "grow-tail-work-bounded",
                    err.code == "budget_exhausted",
                    format!("growing tail loop faulted `{}`", err.code),
                );
            }
        }
        match evaluate_function(
            &tree,
            "NamedTailWalk",
            &BTreeMap::from([("n".into(), int(10_000)), ("acc".into(), int(0))]),
        ) {
            Ok(value) => {
                p.eq("named-tail-still-scales", value, int(10_000));
            }
            Err(err) => {
                p.fail("named-tail-still-scales", err.to_string());
            }
        }
    });

    eprintln!("constructor_cutover checks={}", probe.checks());
    probe.finish();
}
