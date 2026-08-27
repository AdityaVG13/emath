//! `emath explain E-LAW-001` renders a checker-backed Cayley witness.
//! File-mode `--json` is `PlanInspection::to_json` (`emath.plan-explanation v1`).

use std::path::PathBuf;

use emath_cli::{EXIT_OK, explain_inspections, run};

fn repo_file(rel: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn explain_e_law_001_ascii() {
    emath_syntax::install_source_parser();
    assert_eq!(run(&["explain".into(), "E-LAW-001".into()]), EXIT_OK);
}

#[test]
fn explain_e_law_001_json() {
    emath_syntax::install_source_parser();
    assert_eq!(
        run(&["explain".into(), "E-LAW-001".into(), "--json".into()]),
        EXIT_OK
    );
}

#[test]
fn explain_file_json_is_plan_inspection() {
    emath_syntax::install_source_parser();
    let path = repo_file("language/examples/intro/autodiff.emath");
    assert_eq!(
        run(&["explain".into(), path.clone(), "--json".into()]),
        EXIT_OK
    );
    let inspections = explain_inspections(std::path::Path::new(&path)).expect("inspections");
    assert!(
        !inspections.is_empty(),
        "autodiff must produce at least one evaluate-goal inspection"
    );
    let json = inspections[0].to_json();
    assert!(
        json.contains("\"schema\": \"emath.plan-explanation v1\""),
        "{json}"
    );
    assert!(json.contains("\"policy\""), "{json}");
    assert!(json.contains("\"candidates\""), "{json}");
    assert!(json.contains("\"artifact_class\""), "{json}");
    assert!(
        !json.contains("\"symbol_note\""),
        "must not hand-roll a different object under the plan-explanation schema: {json}"
    );
    let explained = inspections[0].explain();
    assert!(
        explained.contains("policy:"),
        "human explain must come from PlanInspection::explain: {explained}"
    );
}

#[test]
fn intro_progressive_examples_check() {
    emath_syntax::install_source_parser();
    for rel in [
        "language/examples/intro/scratch.emath",
        "language/examples/intro/autodiff.emath",
        "language/examples/numerical/heat-rod-sim.emath",
    ] {
        let path = repo_file(rel);
        assert_eq!(
            run(&["check".into(), path]),
            EXIT_OK,
            "{rel} must admit after rebuild"
        );
    }
}
