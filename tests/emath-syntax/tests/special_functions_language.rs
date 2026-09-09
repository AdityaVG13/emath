//! End-to-end `.emath` admission and reference execution for
//! `core::special_functions`.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::{TestVerdict, run_package};
use emath_sema::session::CompilerSession;

const SOURCE: &str = "\
emath function SpecialValues:
    outputs:
        gamma_five: Float64
        beta_value: Float64
        erf_zero: Float64
        gamma_bound: Float64

    definitions:
        gamma_five = gamma(5)
        beta_value = beta(2, 3)
        erf_zero = erf(0)
        gamma_bound = gamma_error_bound(5)

    tests:
        example <known_values>:
            expect gamma_five == 24
            expect beta_value == 0.08333333333333333
            expect erf_zero == 0
            expect gamma_bound > 0
            expect gamma_bound < 1e-12
";

fn checked(source: &str) -> emath_sema::CheckResult {
    CompilerSession::new(Limits::default()).check_owned("special-functions", source)
}

use emath_test_harness::{Probe, boot};

#[test]
fn special_functions_language() {
    boot();
    let mut probe = Probe::new("End-to-end `.emath` admission and reference execution for `core::special_functions`.");
    probe.case("special_functions_admit_and_execute_with_declared_bound", |p| {
    let f0 = p.failures().len();

    let checked = checked(SOURCE);
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let run = run_package(&checked.package);
    p.eq("2", run.summary.passed, 1);
    p.eq("3", run.declarations[0].tests[0].definitions.get("gamma_five"), Some(&Value::F64(24.0)));

    });
    probe.case("gamma_pole_refuses_at_runtime", |p| {
    let f0 = p.failures().len();

    let source = SOURCE.replace("gamma(5)", "gamma(0)");
    let checked = checked(&source);
    p.demand("1",!checked.diagnostics.has_errors(), stringify!(!checked.diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    let run = run_package(&checked.package);
    p.demand("2",matches!(
            &run.declarations[0].tests[0].verdict,
            TestVerdict::Fault { fault }
                if format!("{fault:?}").contains("E-SPECIAL-POLE")
        ), format!(
        "{run:#?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
