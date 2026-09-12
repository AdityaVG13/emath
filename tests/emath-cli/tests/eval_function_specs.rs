//! `emath-lab eval` is not a constructor command.
use emath_cli::{CliExit, EXIT_USAGE};
use emath_cli_lab::run;
use emath_test_harness::Probe;

#[test]
fn eval_function_specs() {
    let mut p = Probe::new("emath-lab eval refuses; use emath run");
    p.case("eval-gone", |p| {
        let code = run(&[
            "eval".into(),
            "square.emath".into(),
            "--function".into(),
            "Square".into(),
            "--set".into(),
            "x=3".into(),
            "--json".into(),
        ]);
        p.eq("code", code, EXIT_USAGE);
        p.eq("cli-exit", format!("{code:?}"), format!("{:?}", CliExit::Usage));
    });
    p.case("sweep-gone", |p| {
        p.eq(
            "code",
            run(&["sweep".into(), "square.emath".into(), "--json".into()]),
            EXIT_USAGE,
        );
    });
    p.case("genesis-gone", |p| {
        p.eq(
            "code",
            run(&["genesis".into(), "square.emath".into(), "--out".into(), "out".into()]),
            EXIT_USAGE,
        );
    });
    p.finish();
}
