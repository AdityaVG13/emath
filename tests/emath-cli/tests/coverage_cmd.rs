//! coverage_cmd tests migrated from the in-crate `#[cfg(test)]` module.

use emath_cli::coverage_cmd::*;
use emath_cli::coverage_seed::{self, DomainSeed};
use std::path::Path;

#[test]
fn support_level_ordering_is_total() {
    for (index, level) in SUPPORT_LEVELS.iter().enumerate() {
        assert_eq!(
            SUPPORT_LEVELS.iter().position(|name| name == level),
            Some(index)
        );
    }
    assert_eq!(
        SUPPORT_LEVELS.iter().position(|name| *name == "nonsense"),
        None
    );
}

#[test]
fn rating_mapping_matches_documented_vocabulary() {
    assert_eq!(rating_to_level("FULL"), Some(3));
    assert_eq!(rating_to_level("SYNTAX-ONLY"), Some(2));
    assert_eq!(rating_to_level("MISSING"), Some(0));
    // PARTIAL must be split per facet, never mapped wholesale.
    assert_eq!(rating_to_level("PARTIAL"), None);
    assert_eq!(rating_to_level("FULL-ISH"), None);
}

#[test]
fn bad_rating_is_refused_with_code() {
    let seed = DomainSeed {
        msc: "99",
        super_domain: "test",
        label: "test",
        ratings: ["FULL", "FULL", "FULL", "FULL", "FULL", "PARTIAL-WHOLE"],
        artifacts: [Some("x"); 6],
        packages: &[],
    };
    let error = resolve_levels(&seed).expect_err("PARTIAL-WHOLE must fail");
    assert!(error.starts_with(E_BAD_RATING), "{error}");
}

#[test]
fn seed_resolves_and_is_evidenced() {
    // Every seed row resolves through the rating mapping and every
    // reference-impl+ facet pins an artifact, else ledger_json refuses.
    for seed in coverage_seed::SEED.iter() {
        let levels = resolve_levels(seed).expect("seed ratings resolve");
        for (index, level) in levels.iter().enumerate() {
            if *level >= COVERAGE_THRESHOLD {
                assert!(
                    seed.artifacts[index].is_some(),
                    "{} {} unevidenced",
                    seed.msc,
                    FACETS[index]
                );
            }
        }
    }
}

#[test]
fn ledger_is_canonical_and_deterministic() {
    let first = ledger_json().expect("ledger generates");
    let second = ledger_json().expect("ledger regenerates");
    assert_eq!(first, second);
    let parsed = emath_artifact::parse_json_document(&first).expect("valid JSON");
    assert_eq!(
        parsed.string_field("schema").expect("schema"),
        "emath.coverage-ledger"
    );
}

#[test]
fn missing_artifact_is_refused() {
    let empty = std::env::temp_dir().join(format!("emath-cov-empty-{}", std::process::id()));
    std::fs::create_dir_all(&empty).expect("temp dir");
    let error = verify_artifacts(&empty).expect_err("missing root has no artifacts");
    assert!(error.starts_with(E_MISSING_ARTIFACT), "{error}");
    // From the workspace root every seed artifact exists.
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    verify_artifacts(&workspace).expect("seed artifacts exist");
    let _ = std::fs::remove_dir(&empty);
}

#[test]
fn package_catalog_rows_are_all_claimed() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = workspace.join("language/stdlib/PACKAGE_CATALOG.md");
    verify_packages(&catalog).expect("every catalog row claimed exactly");
}

#[test]
fn unclaimed_package_is_refused() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let real = std::fs::read_to_string(workspace.join("language/stdlib/PACKAGE_CATALOG.md"))
        .expect("real catalog");
    let dir = std::env::temp_dir().join(format!("emath-cov-pkg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let catalog = dir.join("PACKAGE_CATALOG.md");
    std::fs::write(
        &catalog,
        format!("{real}| `core::nonexistent` | nothing | 1 |\n"),
    )
    .expect("write catalog");
    let error = verify_packages(&catalog).expect_err("unclaimed package");
    assert!(error.starts_with(E_PACKAGE_UNCLAIMED), "{error}");
    let _ = std::fs::remove_file(&catalog);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn check_detects_drift() {
    let generated = ledger_json().expect("ledger generates");
    let dir = std::env::temp_dir().join(format!("emath-cov-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let ledger = dir.join("ledger.json");
    std::fs::write(&ledger, &generated).expect("write ledger");
    assert!(check_against_disk(&generated, &ledger).expect("check runs"));
    std::fs::write(&ledger, "{}\n").expect("corrupt ledger");
    assert!(!check_against_disk(&generated, &ledger).expect("check runs"));
    let _ = std::fs::remove_file(&ledger);
    let _ = std::fs::remove_dir(&dir);
}
