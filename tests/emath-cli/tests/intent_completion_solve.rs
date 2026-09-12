//! `emath solve` is not a constructor command.
use emath_cli::EXIT_USAGE;
use emath_cli_lab::run;
use emath_test_harness::{Probe, boot};

#[test]
fn probe() {
    boot();
    let mut p = Probe::new("solve refuses; labeled worlds are not a second language");
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../language/examples/intro/solve_x2_eq_2.emath")
        .display()
        .to_string();
    p.eq("check", run(&["solve".into(), "--check".into(), path.clone()]), EXIT_USAGE);
    p.eq(
        "apply",
        run(&["solve".into(), "--apply".into(), "real-pm".into(), path]),
        EXIT_USAGE,
    );
    p.finish();
}
