//! `emath test --work N`: the budget-lane pin. The fixture's heavy row
//! needs more than the default 1M work units; without the flag the lane
//! fails that row with budget_exhausted, with it the whole module
//! passes. Failure-first: the default-lane failure predates the flag.
use emath_cli::{EXIT_OK, EXIT_REFUSED};
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
    let mut p = Probe::new("the test lane raises its work budget on demand");
    let fixture = repo("tests/fixtures/constructor/work_budget_row.emath");
    p.eq(
        "default-budget-refuses-heavy-row",
        run(&["test".into(), fixture.clone()]),
        EXIT_REFUSED,
    );
    p.eq(
        "work-admits-heavy-row",
        run(&["test".into(), fixture, "--work".into(), "20000000".into()]),
        EXIT_OK,
    );
    p.finish();
}
