//! `emath solve --check` lists labeled completions, never a naked float.

use emath_cli::{EXIT_OK, EXIT_REFUSED, run, solve_check_json_document};
use emath_syntax::expand_scratch;

fn assert_solve_candidate_keys(cand: &emath_artifact::JsonValue) {
    for key in [
        "label",
        "result_type",
        "domain",
        "exactness",
        "method",
        "evidence_class",
    ] {
        let _ = cand.string_field(key).unwrap_or_else(|_| panic!("{key}"));
    }
    match cand.field("holes").expect("holes") {
        emath_artifact::JsonValue::Arr(_) => {}
        other => panic!("holes must be array, got {other:?}"),
    }
    for key in ["beginner_default", "selected"] {
        match cand.field(key).unwrap_or_else(|_| panic!("{key}")) {
            emath_artifact::JsonValue::Bool(_) => {}
            other => panic!("{key} must be bool, got {other:?}"),
        }
    }
}

#[test]
fn solve_check_lists_labeled_candidates() {
    emath_syntax::install_source_parser();
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("language/examples/intro/solve_x2_eq_2.emath");
    let expansion = expand_scratch("solve x^2 = 2\n");
    assert_eq!(expansion.solve.menu().len(), 5, "{:?}", expansion.solve);
    assert_eq!(expansion.solve.menu(), &emath_syntax::SolveWorld::ALL);
    assert_eq!(
        run(&["solve".into(), "--check".into(), path.display().to_string(),]),
        EXIT_OK
    );
    assert_eq!(
        run(&[
            "solve".into(),
            "--check".into(),
            "--json".into(),
            path.display().to_string(),
        ]),
        EXIT_OK
    );
    let parsed = emath_artifact::parse_json_document(&solve_check_json_document(&expansion))
        .expect("solve --check --json");
    assert_eq!(parsed.string_field("command").expect("command"), "solve");
    assert_eq!(
        parsed.field("ok").expect("ok"),
        &emath_artifact::JsonValue::Bool(true)
    );
    let candidates = match parsed.field("solve_candidates").expect("solve_candidates") {
        emath_artifact::JsonValue::Arr(items) => items,
        other => panic!("solve_candidates must be array, got {other:?}"),
    };
    assert_eq!(candidates.len(), 5, "{candidates:?}");
    let labels: Vec<String> = candidates
        .iter()
        .map(|cand| cand.string_field("label").expect("label"))
        .collect();
    assert_eq!(
        labels,
        vec![
            "real-pm".to_string(),
            "complex".to_string(),
            "modular".to_string(),
            "symbolic".to_string(),
            "numeric".to_string(),
        ]
    );
    for (world, cand) in emath_syntax::SolveWorld::ALL.iter().zip(candidates) {
        assert_solve_candidate_keys(cand);
        let label = cand.string_field("label").expect("label");
        assert_eq!(label, world.as_str());
        assert_eq!(emath_syntax::SolveWorld::parse_label(&label), Some(*world));
        assert_eq!(
            cand.string_field("result_type").expect("result_type"),
            world.result_type()
        );
    }
    assert_eq!(
        run(&[
            "solve".into(),
            "--apply".into(),
            "real-pm".into(),
            path.display().to_string(),
        ]),
        EXIT_OK
    );
}

#[test]
fn solve_check_refuses_unlabeled_unique() {
    emath_syntax::install_source_parser();
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/invalid/solve_x2_eq_2_unlabeled.emath");
    assert_eq!(
        run(&["solve".into(), "--check".into(), path.display().to_string(),]),
        EXIT_REFUSED
    );
}

#[test]
fn solve_apply_unknown_label_is_refused_not_a_sixth_world() {
    emath_syntax::install_source_parser();
    let dir = std::env::temp_dir();
    let path = dir.join("emath-solve-apply-unknown.emath");
    std::fs::write(&path, "solve x^2 = 2\n").expect("write");
    assert_eq!(
        run(&[
            "solve".into(),
            "--apply".into(),
            "quaternion".into(),
            path.display().to_string(),
        ]),
        EXIT_REFUSED
    );
    let _ = std::fs::remove_file(&path);
}
