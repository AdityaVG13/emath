//! coverage_cmd tests migrated from the in-crate `#[cfg(test)]` module.
use emath_cli_lab::coverage_cmd::*;
use emath_cli_lab::coverage_seed::{self, DomainSeed};
use emath_test_harness::{Case, Probe, check_all, expect_ok};
use std::path::Path;

#[test]
fn coverage_ledger_contract() {
    let mut p = Probe::new("coverage ratings resolve, ledger is canonical, drift refuses");
    expect_ok(check_all(
        &[
            Case::new("full", "FULL", Some(3)),
            Case::new("syntax", "SYNTAX-ONLY", Some(2)),
            Case::new("missing", "MISSING", Some(0)),
            Case::new("partial", "PARTIAL", None),
            Case::new("full-ish", "FULL-ISH", None),
        ],
        |rating| rating_to_level(rating),
    ));
    p.case("levels", |p| {
        for (index, level) in SUPPORT_LEVELS.iter().enumerate() {
            p.eq(*level, SUPPORT_LEVELS.iter().position(|name| name == level), Some(index));
        }
        p.eq("nonsense", SUPPORT_LEVELS.iter().position(|name| *name == "nonsense"), None);
    });
    p.case("bad-rating", |p| {
        let seed = DomainSeed {
            msc: "99",
            super_domain: "test",
            label: "test",
            ratings: ["FULL", "FULL", "FULL", "FULL", "FULL", "PARTIAL-WHOLE"],
            artifacts: [Some("x"); 6],
            packages: &[],
        };
        let error = resolve_levels(&seed).expect_err("PARTIAL-WHOLE must fail");
        p.contains("code", &error, E_BAD_RATING);
    });
    p.case("seeds-evidenced", |p| {
        for seed in coverage_seed::SEED.iter() {
            let levels = match resolve_levels(seed) {
                Ok(levels) => levels,
                Err(error) => {
                    p.fail(seed.msc, format!("seed ratings resolve: {error}"));
                    continue;
                }
            };
            for (index, level) in levels.iter().enumerate() {
                if *level >= COVERAGE_THRESHOLD {
                    p.demand(format!("{}-{}", seed.msc, FACETS[index]), seed.artifacts[index].is_some(), "reference-impl facet needs artifact");
                }
            }
        }
    });
    p.case("ledger", |p| {
        let first = ledger_json().expect("ledger generates");
        let second = ledger_json().expect("ledger regenerates");
        p.eq("deterministic", first.clone(), second);
        let parsed = emath_artifact::parse_json_document(&first).expect("valid JSON");
        p.eq("schema", parsed.string_field("schema").expect("schema"), "emath.coverage-ledger".to_string());
    });
    p.case("artifacts", |p| {
        let empty = std::env::temp_dir().join(format!("emath-cov-empty-{}", std::process::id()));
        std::fs::create_dir_all(&empty).expect("temp dir");
        let error = verify_artifacts(&empty).expect_err("missing root has no artifacts");
        p.contains("missing", &error, E_MISSING_ARTIFACT);
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        p.demand("workspace-ok", verify_artifacts(&workspace).is_ok(), "seed artifacts exist");
        let _ = std::fs::remove_dir(&empty);
    });
    p.case("packages", |p| {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalog = workspace.join("language/stdlib/PACKAGE_CATALOG.md");
        p.demand("claimed", verify_packages(&catalog).is_ok(), "every catalog row claimed exactly");
        let real = std::fs::read_to_string(&catalog).expect("real catalog");
        let dir = std::env::temp_dir().join(format!("emath-cov-pkg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let bad = dir.join("PACKAGE_CATALOG.md");
        std::fs::write(&bad, format!("{real}| `core::nonexistent` | nothing | 1 |\n")).expect("write catalog");
        let error = verify_packages(&bad).expect_err("unclaimed package");
        p.contains("unclaimed", &error, E_PACKAGE_UNCLAIMED);
        let _ = std::fs::remove_file(&bad);
        let _ = std::fs::remove_dir(&dir);
    });
    p.case("drift", |p| {
        let generated = ledger_json().expect("ledger generates");
        let dir = std::env::temp_dir().join(format!("emath-cov-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let ledger = dir.join("ledger.json");
        std::fs::write(&ledger, &generated).expect("write ledger");
        p.eq("clean", check_against_disk(&generated, &ledger).expect("check runs"), true);
        std::fs::write(&ledger, "{}\n").expect("corrupt ledger");
        p.eq("drift", check_against_disk(&generated, &ledger).expect("check runs"), false);
        let _ = std::fs::remove_file(&ledger);
        let _ = std::fs::remove_dir(&dir);
    });
    p.finish();
}
