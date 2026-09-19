//! Constructor carrier: wide Int/Rat, Rat pair projection, sibling `use`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use emath_exec_ir::constructor_emir::{cvalue_to_emir, lower_constructor_function, values_equal};
use emath_exec_ir::constructor_layer::{
    admit_tree, evaluate_code_at, evaluate_function, evaluate_function_at,
    evaluate_function_budgeted_at, evaluate_tree, evaluate_tree_at, parse_constructor_scalar, CValue,
};
use emath_exec_ir::exact_int::ExactInt;
use emath_exec_ir::interp::{evaluate, Value};
use emath_syntax::parse_str;
use emath_test_harness::{Probe, boot};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/constructor")
        .join(name)
}

fn parse_file(path: &PathBuf) -> emath_core::tree::SyntaxTree {
    let source = std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let (tree, diagnostics) = parse_str(&source);
    assert!(
        !diagnostics.has_errors(),
        "{} parse errors: {diagnostics:?}",
        path.display()
    );
    tree
}

#[test]
fn constructor_carrier() {
    boot();
    // Deep native recursion (the 256-frame depth-fault margin) with
    // large per-frame values needs more than the default 2 MiB test
    // thread stack; run the cases on a dedicated 64 MiB stack.
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(run_cases)
        .expect("spawn carrier thread");
    handle.join().expect("carrier thread panicked");
}

#[test]
fn sequence_clone_is_shallow() {
    // COW sequence representation: the CPS kont
    // bookkeeping clones argument values at every engine step, so a
    // deep-copy clone made every big-sequence call O(len) per step
    // (17.4s wall for ~13k indexed reads whose unit cost is
    // sub-second). Cloning a sequence must share the backing storage.
    // Failure-first: with Sequence(Vec<CValue>) there is no shared
    // storage to compare — this test cannot even be expressed.
    use std::sync::Arc;
    let items: Vec<CValue> = (0..100_000)
        .map(|i| CValue::Int(ExactInt::from(i)))
        .collect();
    let a = CValue::Sequence(Arc::new(items));
    let b = a.clone();
    let (CValue::Sequence(left), CValue::Sequence(right)) = (&a, &b) else {
        unreachable!("both sides are sequences")
    };
    assert!(Arc::ptr_eq(left, right), "clone must share backing storage");
    assert_eq!(a, b, "structural equality is unchanged by sharing");
}

fn run_cases() {
    let mut probe = Probe::new(
        "Constructor Int/Rat stay exact past i128, Rat.numer/denom project, and package sibling use loads.",
    );

    probe.case("rat_fields_project", |p| {
        let path = fixture("rat_fields.emath");
        let tree = parse_file(&path);
        let report = evaluate_tree(&tree).expect("rat_fields evaluates");
        p.demand("1", !report.tests.is_empty(), "rat_fields produced no authored tests");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("rat_fields failed: {:?}", report.tests),
        );
    });

    probe.case("wide_int_product", |p| {
        let path = fixture("wide_product.emath");
        let tree = parse_file(&path);
        let report = evaluate_tree(&tree).expect("wide_product evaluates");
        p.demand("1", !report.tests.is_empty(), "wide_product produced no authored tests");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("wide_product failed: {:?}", report.tests),
        );
    });

    probe.case("memo_record_bodies_distinct", |p| {
        let path = fixture("memo_record_bodies.emath");
        let tree = parse_file(&path);
        let report = evaluate_tree(&tree).expect("memo record bodies evaluate");
        p.demand("1", report.tests.len() == 2, "memo bodies expected two authored tests");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("memo record bodies failed: {:?}", report.tests),
        );
    });

    probe.case("multi_output_tail_call_packs_record", |p| {
        let path = fixture("tail_call_multi_output.emath");
        let tree = parse_file(&path);
        let report = evaluate_tree(&tree).expect("tail call multi-output evaluates");
        p.demand(
            "1",
            report.tests.len() == 2,
            "tail call multi-output expected two authored tests",
        );
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("tail call multi-output failed: {:?}", report.tests),
        );
    });

    probe.case("l1_stale_dependency_refuses", |p| {
        // L1 dependency pinning: a quote captured against dependency v1,
        // evaluated against dependency v2 (same name, different body),
        // refuses stale_dependency — never a silent re-resolve.
        let capture_source = "\
emath function dep:
    inputs:
        unused: Int
    outputs:
        result: Rat
    definitions:
        result = 1 / 1

emath function make_quote:
    inputs:
        unused: Int
    outputs:
        result: Code
    definitions:
        result = quote(dep(0) + dep(0))
";
        let eval_source = "\
emath function dep:
    inputs:
        unused: Int
    outputs:
        result: Rat
    definitions:
        result = 2 / 1
";
        let (capture_tree, capture_diag) = parse_str(capture_source);
        assert!(!capture_diag.has_errors(), "capture source parse errors");
        let (eval_tree, eval_diag) = parse_str(eval_source);
        assert!(!eval_diag.has_errors(), "eval source parse errors");
        let code = evaluate_function_at(
            &capture_tree,
            "make_quote",
            &BTreeMap::from([("unused".to_string(), CValue::Int(ExactInt::zero()))]),
            None,
        )
        .expect("quote capture evaluates");
        let CValue::Code(code) = code else {
            panic!("make_quote returned non-code");
        };
        // On the capture tree the dependency matches: 1/1 + 1/1 (the
        // exact value canonicalizes to the Int display 2).
        let same = evaluate_code_at(&capture_tree, &code, &BTreeMap::new(), &[], None);
        p.demand(
            "1",
            same.as_ref().map(|v| v.to_string()).as_deref() == Ok("2"),
            format!("same-tree evaluation: {same:?}"),
        );
        // On the changed tree the stamp differs: stale_dependency.
        let stale = evaluate_code_at(&eval_tree, &code, &BTreeMap::new(), &[], None);
        let stale_code = stale.expect_err("changed dependency must refuse").code;
        p.demand(
            "2",
            stale_code == "stale_dependency",
            format!("expected stale_dependency, got {stale_code}"),
        );
    });

    probe.case("l1_fuel_bounds_nested_evaluation", |p| {
        // L1 fuel: a quoted program's nested evaluation shares the
        // engine work budget — a small budget suspends with
        // budget_exhausted instead of running unbounded.
        let source = "\
emath function spin:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = if n <= 0: 0 else: spin(n - 1) + 1

emath function make_call:
    inputs:
        unused: Int
    outputs:
        result: Code
    definitions:
        result = quote(spin(100))

emath function drive:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        made = make_call(0)
        result = quote.evaluate(made)
";
        let (tree, diagnostics) = parse_str(source);
        assert!(!diagnostics.has_errors(), "fuel source parse errors");
        let inputs = BTreeMap::from([("unused".to_string(), CValue::Int(ExactInt::zero()))]);
        let bounded = evaluate_function_budgeted_at(
            &tree,
            "drive",
            &inputs,
            20,
            None,
            "l1_fuel_probe",
            None,
            "",
        );
        let (err, _checkpoint) = bounded.expect_err("small budget must suspend");
        p.demand(
            "1",
            err.code == "budget_exhausted",
            format!("expected budget_exhausted, got {}", err.code),
        );
        let unbounded = evaluate_function_at(&tree, "drive", &inputs, None);
        p.demand(
            "2",
            unbounded.as_ref().map(|v| v.to_string()).as_deref() == Ok("100"),
            format!("default-budget evaluation: {unbounded:?}"),
        );
    });

    probe.case("l1_type_mismatch_names_refusal", |p| {
        // L1 type mismatch: applying closed function code to a Bool
        // input refuses with the NAMED code type_mismatch (the receipt
        // diagnostic, verbatim).
        let source = "\
emath query TypeMismatch:
    inputs:
        arg: Bool
    definitions:
        target = quote(function x in Rat: x * x)
    question:
        target = target
        scope = arg
        assumptions = []
    using:
        method = closed_code
    answer:
        form = value
        accept = unchecked
";
        let (tree, diagnostics) = parse_str(source);
        assert!(!diagnostics.has_errors(), "type source parse errors");
        let receipt = emath_exec_ir::constructor_layer::evaluate_query_at(
            &tree,
            "TypeMismatch",
            &BTreeMap::from([("arg".to_string(), CValue::Bool(true))]),
            None,
        )
        .expect("query evaluates to a receipt");
        p.eq("1", receipt.fulfillment.as_str(), "unmet");
        p.demand(
            "2",
            receipt.diagnostic_code.as_deref() == Some("type_mismatch"),
            format!(
                "expected diagnostic type_mismatch, got {:?} (remaining {:?})",
                receipt.diagnostic_code, receipt.remaining
            ),
        );
    });

    probe.case("nested_record_path_projects", |p| {
        let source = "\
emath object Inner:
    representation:
        y: sequence(Rat)

emath object Outer:
    representation:
        next: Inner
        alt: Inner

emath function MakeOuter:
    inputs:
        x: Rat
    outputs:
        result: Outer
    definitions:
        result = Outer: {next: Inner: {y: [x]}, alt: Inner: {y: [x + x]}}

emath function DeepReader:
    inputs:
        unused: Int
    outputs:
        result: Rat
        deep: Rat
    definitions:
        stepped = MakeOuter(3 / 1)
        result = stepped.next.y[0]
        deep = stepped.alt.y[0]
    tests:
        example <three_segment_paths>:
            given unused = 0
            expect result == 3 / 1
            expect deep == 6 / 1
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let report = evaluate_tree(&tree).expect("nested paths evaluate");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("nested paths failed: {:?}", report.tests),
        );
    });

    probe.case("wide_rat_sum", |p| {
        let path = fixture("wide_rat_sum.emath");
        let tree = parse_file(&path);
        let report = evaluate_tree(&tree).expect("wide_rat_sum evaluates");
        p.demand("1", !report.tests.is_empty(), "wide_rat_sum produced no authored tests");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("wide_rat_sum failed: {:?}", report.tests),
        );
    });

    probe.case("package_sibling_use", |p| {
        let path = fixture("pkg/main.emath");
        let tree = parse_file(&path);
        let report = evaluate_tree_at(&tree, Some(&path)).expect("pkg main evaluates");
        p.demand("1", !report.tests.is_empty(), "pkg sibling produced no authored tests");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("pkg sibling failed: {:?}", report.tests),
        );
    });

    probe.case("machine_int_ops_admit", |p| {
            let source = "\
emath function MachineOps:
    inputs:
        n: Int
        q: Int
    outputs:
        result: Int
    definitions:
        result = int_quot(n, q) * 100 + int_rem(n, q) * 10 + int_root(n, q)
    tests:
        example <ten_over_three>:
            given n = 10
            given q = 3
            expect result == 312
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let admitted = admit_tree(&tree);
        p.demand(
            "2",
            admitted.is_ok(),
            format!("admit refused machine int ops: {admitted:?}"),
        );
        let report = evaluate_tree(&tree).expect("machine ops evaluate");
        p.demand("3", !report.tests.is_empty(), "machine ops produced no authored tests");
        p.demand(
            "4",
            report.tests.iter().all(|test| test.passed),
            format!("machine ops failed: {:?}", report.tests),
        );
    });

    probe.case("machine_int_ops_refuse_rat", |p| {
        let source = "\
emath function NeedsInt:
    inputs:
        x: Rat
    outputs:
        result: Int
    definitions:
        result = int_quot(x, 2)
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let admitted = admit_tree(&tree);
        p.eq(
            "2",
            admitted.as_ref().err().map(|err| err.code.as_str()).unwrap_or(""),
            "type",
        );
    });

    probe.case("set_rat_is_reduced", |p| {
        let value = parse_constructor_scalar(" 6/4 ");
        p.eq(
            "1",
            value,
            CValue::Rat {
                num: ExactInt::from(3i128),
                den: ExactInt::from(2i128),
            },
        );
        p.demand(
            "2",
            !matches!(parse_constructor_scalar("1/0"), CValue::Rat { .. }),
            "1/0 must not become a Rat",
        );
        p.eq("3", parse_constructor_scalar("+2"), CValue::Int(ExactInt::from(2i128)));
    });

    probe.case("wide_integer_root", |p| {
        let path = fixture("wide_root.emath");
        let tree = parse_file(&path);
        let report = evaluate_tree_at(&tree, Some(&path)).expect("wide_root evaluates");
        p.demand("1", !report.tests.is_empty(), "wide_root produced no authored tests");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("wide_root failed: {:?}", report.tests),
        );
    });

    probe.case("missing_sibling_is_pkg_050", |p| {
        let source = "package demo\n\nuse demo.missing\n\nemath function F:\n    inputs:\n        unused: Int\n    outputs:\n        result: Int\n    definitions:\n        result = unused\n";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let path = fixture("pkg/main.emath");
        let err = evaluate_tree_at(&tree, Some(&path)).expect_err("missing sibling must refuse");
        p.eq("2", err.code.as_str(), "E-PKG-050");
    });

    probe.case("euclidean_quot_rem_identity", |p| {
        let source = "\
emath function Euclid:
    inputs:
        unused: Int
    outputs:
        result: Bool
    definitions:
        n = 0 - 17
        q = int_quot(n, 5)
        r = int_rem(n, 5)
        result = q == 0 - 4 and r == 3 and q * 5 + r == n
    tests:
        example <neg_seventeen>:
            given unused = 0
            expect result == true
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let report = evaluate_tree(&tree).expect("euclidean identity evaluates");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("euclidean identity failed: {:?}", report.tests),
        );
    });

    probe.case("machine_gcd_and_fact", |p| {
        let source = "\
emath function WideGcdFact:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        g = int_gcd(100000000000000000000, 1000000000000000)
        result = g + int_fact(10)
    tests:
        example <gcd_and_ten_fact>:
            given unused = 0
            expect result == 1000000003628800
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let report = evaluate_tree(&tree).expect("gcd/fact evaluates");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("gcd/fact failed: {:?}", report.tests),
        );
    });

    probe.case("machine_pow_and_suffix_sum", |p| {
        let source = "\
emath function PowAndSuffix:
    inputs:
        unused: Int
    outputs:
        result: Int
    definitions:
        result = int_pow(2, 10) + int_sum_from([1, 2, 3, 4], 2)
    tests:
        example <ten_and_tail>:
            given unused = 0
            expect result == 1031
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let report = evaluate_tree(&tree).expect("pow/suffix evaluates");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("pow/suffix failed: {:?}", report.tests),
        );
    });

    probe.case("machine_int_domains_and_weighted_prod", |p| {
        let source = "\
emath function Domain:
    inputs:
        unused: Int
    outputs:
        result: Bool
    definitions:
        fact_neg = int_fact(0 - 1) == 0
        zeroth = int_root(9, 0) == 0
        first_root = int_root(9, 1) == 9
        ham = int_hamming([1], [1, 2]) == 1
        inv = int_powmod(2, 0 - 1, 5) == 3
        weighted = int_weighted_prod([2, 3], [3, 1], 0, 1) == 24
        rising_neg = int_rising(3, 0 - 1) == 0
        dfact_neg = int_double_fact(0 - 1) == 0
        result = fact_neg and zeroth and first_root and ham and inv and weighted and rising_neg and dfact_neg
    tests:
        example <holes>:
            given unused = 0
            expect result == true
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let report = evaluate_tree(&tree).expect("domain identity evaluates");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("domain identity failed: {:?}", report.tests),
        );
    });

    probe.case("machine_binom_egcd_totient", |p| {
        let source = "\
emath function Neighbors:
    inputs:
        unused: Int
    outputs:
        result: Bool
    definitions:
        triple = int_egcd(12, 18)
        binom = int_binom(5, 2) == 10
        empty = int_binom(10, 0) == 1
        past = int_binom(5, 7) == 0
        phi = int_totient(12) == 4
        one = int_totient(1) == 1
        bezout = triple[0] == 6 and triple[1] * 12 + triple[2] * 18 == triple[0]
        result = binom and empty and past and phi and one and bezout
    tests:
        example <neighbors>:
            given unused = 0
            expect result == true
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let report = evaluate_tree(&tree).expect("binom/egcd/totient evaluates");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("binom/egcd/totient failed: {:?}", report.tests),
        );
    });

    probe.case("exact_integer_modules", |p| {
        for name in [
            "exact/integers.emath",
            "discrete/combinatorics.emath",
        ] {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../language/modules")
                .join(name);
            let tree = parse_file(&path);
            let report = evaluate_tree_at(&tree, Some(&path))
                .unwrap_or_else(|err| panic!("{name} evaluates: {err:?}"));
            p.demand(
                format!("{name}/tests"),
                report.tests.iter().all(|test| test.passed),
                format!("{name} failed: {:?}", report.tests),
            );
        }
    });

    probe.case("set_parses_sequences", |p| {
        p.eq(
            "ints",
            parse_constructor_scalar("[1, 2, 3]"),
            CValue::Sequence(std::sync::Arc::new(vec![
                CValue::Int(ExactInt::from(1i128)),
                CValue::Int(ExactInt::from(2i128)),
                CValue::Int(ExactInt::from(3i128)),
            ])),
        );
        p.eq(
            "rats",
            parse_constructor_scalar("[1/2, 6/4]"),
            CValue::Sequence(std::sync::Arc::new(vec![
                CValue::Rat {
                    num: ExactInt::from(1i128),
                    den: ExactInt::from(2i128),
                },
                CValue::Rat {
                    num: ExactInt::from(3i128),
                    den: ExactInt::from(2i128),
                },
            ])),
        );
        p.eq(
            "empty",
            parse_constructor_scalar("[]"),
            CValue::Sequence(std::sync::Arc::new(vec![])),
        );
        p.demand(
            "malformed-stays-record",
            matches!(parse_constructor_scalar("[1, 2"), CValue::Record { .. }),
            "unclosed sequence must not parse as Sequence",
        );
        let source = "\
emath function TraceLen:
    inputs:
        observed: sequence(Int)
    outputs:
        result: Int
    definitions:
        result = length(observed)
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("parse", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "observed".into(),
            parse_constructor_scalar("[1, 2, 3]"),
        );
        let value = evaluate_function(&tree, "TraceLen", &inputs).expect("TraceLen evaluates");
        p.eq("len", value, CValue::Int(ExactInt::from(3i128)));
    });

    probe.case("experiment_kit_modules", |p| {
        for name in [
            "numerics/experiment.emath",
            "numerics/euler.emath",
            "fold/reduce.emath",
        ] {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../language/modules")
                .join(name);
            let tree = parse_file(&path);
            let report = evaluate_tree_at(&tree, Some(&path))
                .unwrap_or_else(|err| panic!("{name} evaluates: {err:?}"));
            p.demand(
                format!("{name}/tests"),
                report.tests.iter().all(|test| test.passed),
                format!("{name} failed: {:?}", report.tests),
            );
        }
        let heat = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../language/examples/objects/heat-rod-sim.emath");
        let tree = parse_file(&heat);
        let report = evaluate_tree_at(&tree, Some(&heat))
            .unwrap_or_else(|err| panic!("heat-rod-sim evaluates: {err:?}"));
        p.demand(
            "heat-rod/tests",
            report.tests.iter().all(|test| test.passed),
            format!("heat-rod-sim failed: {:?}", report.tests),
        );
    });

    probe.case("wrapping_int_rem_hits_machine_leaf", |p| {
        let source = "\
emath function int_rem:
    inputs:
        value: Int
        modulus: Int
    outputs:
        result: Int
    definitions:
        result = if modulus <= 0: 0 else: int_rem(value, modulus)
    tests:
        example <seventeen_five>:
            given value = 17
            given modulus = 5
            expect result == 2
        example <nonpositive_modulus>:
            given value = 17
            given modulus = 0
            expect result == 0
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let admitted = admit_tree(&tree);
        p.demand(
            "2",
            admitted.is_ok(),
            format!("wrapper int_rem admit: {admitted:?}"),
        );
        let report = evaluate_tree(&tree).expect("wrapping int_rem must hit the machine leaf");
        p.demand("3", !report.tests.is_empty(), "wrapper int_rem produced no tests");
        p.demand(
            "4",
            report.tests.iter().all(|test| test.passed),
            format!("wrapper int_rem recursed or failed: {:?}", report.tests),
        );
    });

    probe.case("quote_open_binder_admits", |p| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../language/modules/quote/walk.emath");
        let tree = parse_file(&path);
        let admitted = admit_tree(&tree);
        p.demand(
            "1",
            admitted.is_ok(),
            format!("quote.open check-path must admit: {admitted:?}"),
        );
    });

    probe.case("given_rat_on_int_is_type", |p| {
        let source = "\
emath function NeedsInt:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n
    tests:
        example <half>:
            given n = 1/2
            expect result == 0
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let admitted = admit_tree(&tree);
        p.eq(
            "2",
            admitted.as_ref().err().map(|err| err.code.as_str()).unwrap_or(""),
            "type",
        );
    });

    probe.case("values_equal_faults_on_missing_carrier", |p| {
        match values_equal(&Value::I64(1), &CValue::Unit) {
            Ok(equal) => p.demand("1", false, format!("swallowed convert as {equal}")),
            Err(err) => p.demand("1", err.contains("no EMIR carrier"), err),
        };
    });

    probe.case("wide_int_emir_round_trip", |p| {
        let path = fixture("wide_product.emath");
        let tree = parse_file(&path);
        let vm = evaluate_function(
            &tree,
            "WideSquare",
            &BTreeMap::from([("unused".into(), CValue::Int(ExactInt::from(0i128)))]),
        )
        .expect("wide product vm");
        match cvalue_to_emir(&vm) {
            Ok(converted) => p.demand(
                "1",
                converted != Value::I64(0),
                format!("wide product collapsed: {converted:?}"),
            ),
            Err(err) => p.fail("1", err),
        };
        match lower_constructor_function(&tree, "WideSquare") {
            Ok(lowered) => match evaluate(&lowered.program, &[Value::I64(0)], &[]) {
                Ok(emitted) => match values_equal(&emitted, &vm) {
                    Ok(equal) => p.demand("2", equal, format!("{emitted:?} vs {vm:?}")),
                    Err(err) => p.fail("2", err),
                },
                Err(err) => p.fail("2", format!("{err:?}")),
            },
            Err(err) => p.fail("2", err),
        };
    });

    probe.case("tail_call_reuses_frame", |p| {
        let source = "\
emath function CountDown:
    inputs:
        n: Int
        acc: Int
    outputs:
        result: Int
    definitions:
        result = if n <= 0: acc else: CountDown(n - 1, acc + 1)
    tests:
        example <four_hundred>:
            given n = 400
            given acc = 0
            expect result == 400
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let report = evaluate_tree(&tree).expect("countdown evaluates");
        p.demand(
            "2",
            report.tests.iter().all(|test| test.passed),
            format!("countdown failed: {:?}", report.tests),
        );
    });

    probe.case("nontail_recursion_does_not_abort", |p| {
        let source = "\
emath function Fact:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = if n <= 1: 1 else: Fact(n - 1) * n
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        match evaluate_function(
            &tree,
            "Fact",
            &BTreeMap::from([("n".into(), CValue::Int(ExactInt::from(300i128)))]),
        ) {
            Ok(_) => {
                p.demand("2", false, "fact 300 must refuse at the depth bound");
            }
            Err(err) => {
                p.eq("2", err.code.as_str(), "recursion_depth_exceeded");
                p.demand(
                    "3",
                    err.message.contains("256") && err.message.contains("Fact"),
                    format!("depth fault must name the limit and the frame: {}", err.message),
                );
            }
        };
    });

    probe.case("solve_and_bounded_modules", |p| {
        for name in [
            "algebra/solve.emath",
            "optimization/bounded.emath",
        ] {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../language/modules")
                .join(name);
            let tree = parse_file(&path);
            let report = evaluate_tree_at(&tree, Some(&path))
                .unwrap_or_else(|err| panic!("{name} evaluates: {err:?}"));
            p.demand(
                format!("{name}/tests"),
                report.tests.iter().all(|test| test.passed),
                format!("{name} failed: {:?}", report.tests),
            );
        }
    });

    probe.case("solve_rat_irrational_root_fault", |p| {
        let source = "\
use algebra.solve

emath function Irrational:
    inputs:
        unused: Int
    outputs:
        result: Rat
    definitions:
        result = solve_rat(function x in Rat: x * x - 2 / 1)
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let module = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../language/modules/algebra/solve.emath");
        match evaluate_function_at(
            &tree,
            "Irrational",
            &BTreeMap::from([("unused".into(), CValue::Int(ExactInt::from(0i128)))]),
            Some(&module),
        ) {
            Ok(value) => {
                p.demand("2", false, format!("x*x-2 must refuse, got {value}"));
            }
            Err(err) => {
                p.eq("2", err.code.as_str(), "irrational_root");
                p.demand(
                    "3",
                    err.message.contains("mathematical method refused: irrational_root"),
                    format!("refusal message: {}", err.message),
                );
            }
        };
    });

    probe.case("maximize_rat_empty_domain_fault", |p| {
        let source = "\
use optimization.bounded

emath function Empty:
    inputs:
        unused: Int
    outputs:
        arg: Int
        value: Rat
    definitions:
        arg = maximize_rat(5, 4, function n in Int: n / 1).arg
        value = 0 / 1
";
        let (tree, diagnostics) = parse_str(source);
        p.demand("1", !diagnostics.has_errors(), format!("parse: {diagnostics:?}"));
        let module = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../language/modules/optimization/bounded.emath");
        match evaluate_function_at(
            &tree,
            "Empty",
            &BTreeMap::from([("unused".into(), CValue::Int(ExactInt::from(0i128)))]),
            Some(&module),
        ) {
            Ok(value) => p.demand("2", false, format!("empty domain must refuse, got {value}")),
            Err(err) => p.eq("2", err.code.as_str(), "empty_domain"),
        };
    });

    probe.finish();
}
