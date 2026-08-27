//! `emath solve --check` lists labeled completions, never a naked float.

use emath_cli::{EXIT_OK, EXIT_REFUSED, run};
use emath_syntax::expand_scratch;

#[test]
fn solve_check_lists_labeled_candidates() {
    emath_syntax::install_source_parser();
    let dir = std::env::temp_dir();
    let path = dir.join("emath-solve-check-x2.emath");
    std::fs::write(&path, "solve x^2 = 2\n").expect("write");
    let expansion = expand_scratch("solve x^2 = 2\n");
    assert!(
        expansion.solve_candidates.len() >= 5,
        "{:?}",
        expansion.solve_candidates
    );
    assert_eq!(
        run(&["solve".into(), "--check".into(), path.display().to_string(),]),
        EXIT_OK
    );
    assert_eq!(
        run(&[
            "solve".into(),
            "--apply".into(),
            "real-pm".into(),
            path.display().to_string(),
        ]),
        EXIT_OK
    );
    let _ = std::fs::remove_file(&path);
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
