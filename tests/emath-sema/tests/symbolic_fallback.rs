//! nothing-returns-nothing (Vision law 2): a program whose expressions
//! have no evaluable world returns the symbolic form with its meaning
//! label attached instead of erroring; genuinely impossible math still
//! refuses typed.

use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_test_harness::{boot, Probe, Source};

const UNBOUND_SQUARE: &str = "\
emath function UnboundSquare:
    inputs:
        x: Float64

    outputs:
        y: Float64
        k: Float64

    definitions:
        y = x * x
        k = 2.0
";

#[test]
fn symbolic_fallback_contract() {
    boot();
    let mut p = Probe::new("unevaluable programs return labeled symbolic forms, impossible math refuses");
    p.case("labeled-symbolic", |p| {
        let result = Source::from_str("unbound-square", UNBOUND_SQUARE).must_admit(p);
        let report = run_package(&result.package);
        if report.declarations.is_empty() || report.declarations[0].tests.is_empty() {
            p.fail("run", "the zero-test declaration still runs once".to_string());
            return;
        }
        let test = &report.declarations[0].tests[0];
        p.eq("tests-run", report.declarations[0].tests.len(), 1);
        p.eq("meaning-label", test.verdict.meaning_label(), Some("symbolic-only"));
        p.demand("not-refused", test.verdict.is_refused() == false, format!("{test:?}"));
        match test.verdict.symbolic_forms() {
            Some(forms) => p.eq("form-y", forms.get("y").map(String::as_str), Some("x * x")),
            None => p.fail("forms", "symbolic forms must be attached".to_string()),
        };
        match test.verdict.symbolic_holes() {
            Some(holes) => p.eq("hole-x", holes.get("x").map(String::as_str), Some("Float64")),
            None => p.fail("holes", "symbolic holes must be attached".to_string()),
        };
        p.eq("evaluable-k", test.definitions.get("k"), Some(&Value::F64(2.0)));
        p.eq("output-k", test.outputs.get("k"), Some(&Value::F64(2.0)));
        p.demand(
            "y-not-naked",
            test.outputs.contains_key("y") == false,
            "the uncomputed output is not a naked number".to_string(),
        );
        p.eq("summary-symbolic", report.summary.symbolic, 1);
    });
    p.case("impossible-refuses", |p| {
        let base = Source::from_workspace("language/examples/physics/newton-second.emath");
        let broken = base.text().replace("force = mass * acceleration", "force = mass + acceleration");
        Source::from_str("unbound-unit-mismatch", &broken).must_refuse(p, &["E-UNIT-101"]);
    });
    p.finish();
}
