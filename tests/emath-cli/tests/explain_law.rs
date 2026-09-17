//! File/plan explanation is not a constructor command.
use emath_cli::{EXIT_OK, EXIT_USAGE};
use emath_cli_lab::run;
use emath_test_harness::{Probe, boot};
#[test]
fn probe() {
    boot();
    let mut p = Probe::new("explain file refuses; constructor check still admits");
    p.case("file-refuses", |p| {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../language/examples/code/autodiff.emath");
        p.eq(
            "exit-json",
            run(&[
                "explain".into(),
                path.to_string_lossy().into_owned(),
                "--json".into(),
            ]),
            EXIT_USAGE,
        );
    });
    p.case("diagnostic-code", |p| {
        p.eq(
            "type",
            run(&["explain".into(), "E-TYPE-002".into(), "--json".into()]),
            EXIT_OK,
        );
    });
    p.case("intro-checks", |p| {
        let scratch = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/language/scratch_bare.emath")
            .to_string_lossy()
            .into_owned();
        p.eq("scratch-refuses", run(&["check".into(), scratch]), EXIT_USAGE);
        for rel in [
            "language/examples/code/autodiff.emath",
            "language/examples/objects/heat-rod-sim.emath",
        ] {
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(rel)
                .to_string_lossy()
                .into_owned();
            p.eq(rel, run(&["check".into(), path]), EXIT_OK);
        }
    });
    p.finish();
}
