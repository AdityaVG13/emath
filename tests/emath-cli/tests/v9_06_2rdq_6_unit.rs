//! CLI: expand / exactness / freeze / why / assumptions.

use emath_cli::{EXIT_OK, EXIT_REFUSED, run};

fn repo_file(rel: &str) -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn expand_and_exactness_and_assumptions_succeed() {
    emath_syntax::install_source_parser();
    let path = repo_file("language/examples/intro/scratch.emath");
    assert_eq!(run(&["expand".into(), path.clone()]), EXIT_OK);
    assert_eq!(run(&["exactness".into(), path.clone()]), EXIT_OK);
    assert_eq!(
        run(&[
            "exactness".into(),
            path.clone(),
            "--raise".into(),
            "units".into()
        ]),
        EXIT_OK
    );
    assert_eq!(run(&["assumptions".into(), path.clone()]), EXIT_OK);
    assert_eq!(
        run(&["why".into(), path.clone(), "inference:1".into()]),
        EXIT_OK
    );
    assert_eq!(run(&["freeze".into(), path.clone()]), EXIT_OK);
}

#[test]
fn freeze_emits_versioned_lock_without_raising_authority() {
    emath_syntax::install_source_parser();
    let path = repo_file("language/examples/intro/scratch.emath");
    let tmp = std::env::temp_dir().join("emath-v9-06-2rdq-6-freeze.emath");
    assert_eq!(
        run(&[
            "freeze".into(),
            path,
            "--out".into(),
            tmp.display().to_string(),
            "--json".into(),
        ]),
        EXIT_OK
    );
    let lock_path = tmp.with_extension("freeze.lock.json");
    let lock = std::fs::read_to_string(&lock_path).expect("sidecar lock");
    assert!(lock.contains("emath.freeze.lock.v1"), "{lock}");
    assert!(lock.contains("emath:meaning:v1:"), "{lock}");
    assert!(lock.contains("\"authority_raised\": false"), "{lock}");
    assert!(lock.contains("strict-f64"), "{lock}");
    assert!(lock.contains("native.rust"), "{lock}");
    emath_artifact::parse_json_document(&lock).expect("lock must parse as JSON");
}

#[test]
fn freeze_refuses_claimed_exact_hole() {
    emath_syntax::install_source_parser();
    let path = repo_file("tests/invalid/v9_06_2rdq_6.emath");
    assert_eq!(run(&["freeze".into(), path]), EXIT_REFUSED);
}
