//! `emath exactness` / `freeze` are not constructor commands.
use emath_cli::EXIT_USAGE;
use emath_cli_lab::run;
use emath_test_harness::{Probe, boot};

fn repo(rel: &str) -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn probe() {
    boot();
    let mut p = Probe::new("exactness and freeze refuse; meaning budget is not a second language");
    let path = repo("tests/fixtures/language/intro/l1_guided.emath");
    p.eq("exactness", run(&["exactness".into(), path.clone()]), EXIT_USAGE);
    p.eq(
        "raise",
        run(&["exactness".into(), path.clone(), "--raise".into(), "units".into()]),
        EXIT_USAGE,
    );
    p.eq("freeze", run(&["freeze".into(), path]), EXIT_USAGE);
    p.finish();
}
