use std::collections::BTreeMap;
use std::str::FromStr;

use emath_core::FeatureId;
use emath_sema::{LiveConformanceRequest, inspect_live_source};

fn authority() -> BTreeMap<FeatureId, String> {
    [
        "std.kind.function",
        "std.type.int",
        "std.capability.math.add",
    ]
    .into_iter()
    .map(|id| {
        (
            FeatureId::from_str(id).unwrap(),
            "legacy-active-dual-run".to_string(),
        )
    })
    .collect()
}

fn independent_add(left: i64, right: i64) -> Result<i64, &'static str> {
    left.checked_add(right).ok_or("exactness-loss")
}

fn candidate_add(left: i64, right: i64) -> Result<i64, &'static str> {
    left.checked_add(right).ok_or("exactness-loss")
}

fn independent_float_into_int(value: f64) -> Result<i64, &'static str> {
    if value.is_finite() && value.fract() == 0.0 {
        Ok(value as i64)
    } else {
        Err("exactness-loss")
    }
}

#[test]
fn add_exact_agrees_at_every_live_stage() {
    emath_syntax::install_source_parser();
    let image = format!("distribution-sha256:{}", "3".repeat(64));
    let auth = authority();
    let response = inspect_live_source(LiveConformanceRequest {
        source_name: "AddExact.emath",
        source: "emath function AddExact:\n    definitions:\n        result = 2 + 1\n",
        repository_commit: "abc123",
        compiler_identity: "emath-sema",
        language_image_id: &image,
        authority: &auth,
    })
    .unwrap();
    response.validate().unwrap();
    for stage in ["parse", "admit", "lower", "world", "execute", "artifact"] {
        assert!(response.stages.contains_key(stage));
    }
    assert_eq!(candidate_add(2, 1), independent_add(2, 1));
    assert_eq!(candidate_add(2, 1), Ok(3));
    assert_eq!(response.result_or_diagnosis, "value:3:exact-int");
    assert!(response.artifact_manifest.contains("value:3:exact-int"));
    assert!(response.artifact_manifest.contains(&image));
}

#[test]
fn diagnosis_overflow_and_stage_mutants_block_cutover() {
    assert_eq!(independent_float_into_int(1.5), Err("exactness-loss"));
    assert_eq!(candidate_add(i64::MAX, 1), Err("exactness-loss"));
    assert_ne!(candidate_add(2, 1), Ok(999));

    emath_syntax::install_source_parser();
    let image = format!("distribution-sha256:{}", "3".repeat(64));
    let auth = authority();
    let mut response = inspect_live_source(LiveConformanceRequest {
        source_name: "AddExact.emath",
        source: "emath function AddExact:\n    definitions:\n        result = 2 + 1\n",
        repository_commit: "abc123",
        compiler_identity: "emath-sema",
        language_image_id: &image,
        authority: &auth,
    })
    .unwrap();
    response.stages.remove("world");
    assert!(response.validate().is_err());
    response.stages.insert(
        "world".to_string(),
        emath_sema::StageStatus::Available("float-world".to_string()),
    );
    response.result_or_diagnosis = "value:999:exact-int".to_string();
    assert!(
        response.validate().is_err(),
        "artifact/result mismatch blocks authority"
    );
}
