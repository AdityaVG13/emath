use std::collections::BTreeMap;
use std::str::FromStr;

use emath_core::FeatureId;
use emath_sema::{LiveAdapterError, LiveConformanceRequest, inspect_live_source};

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
            "legacy-active".to_string(),
        )
    })
    .collect()
}

#[test]
fn add_exact_reports_real_parse_admit_lower_execute_and_artifact_stages() {
    emath_syntax::install_source_parser();
    let source = "emath function AddExact:\n    definitions:\n        result = 2 + 1\n";
    let request = LiveConformanceRequest {
        source_name: "AddExact.emath",
        source,
        repository_commit: "abc123",
        compiler_identity: "emath-sema",
        language_image_id: &format!("distribution-sha256:{}", "1".repeat(64)),
        authority: &authority(),
    };
    let first = inspect_live_source(request.clone()).unwrap();
    let second = inspect_live_source(request).unwrap();
    assert_eq!(first.source_hash, second.source_hash);
    assert_eq!(first.cst_identity, second.cst_identity);
    assert_eq!(first.result_or_diagnosis, "value:3:exact-int");
    first.validate().unwrap();
    assert!(first.artifact_manifest.contains(&first.source_hash));
    assert_eq!(first.resolved_features.len(), 3);
}

#[test]
fn unavailable_and_mutated_evidence_never_looks_complete() {
    emath_syntax::install_source_parser();
    let image = format!("distribution-sha256:{}", "1".repeat(64));
    let auth = authority();
    assert_eq!(
        inspect_live_source(LiveConformanceRequest {
            source_name: "forged.emath",
            source: "emath function f:\n",
            repository_commit: "not-a-commit",
            compiler_identity: "emath-sema",
            language_image_id: &image,
            authority: &auth,
        }),
        Err(LiveAdapterError::InvalidCommit)
    );
    let mut response = inspect_live_source(LiveConformanceRequest {
        source_name: "FloatIntoInt.emath",
        source: "emath function FloatIntoInt:\n    outputs:\n        result: Int\n    definitions:\n        result = 1.5\n",
        repository_commit: "abc123",
        compiler_identity: "emath-sema",
        language_image_id: &image,
        authority: &auth,
    }).unwrap();
    response.stages.remove("artifact");
    assert_eq!(
        response.validate(),
        Err(LiveAdapterError::PartialClaim("artifact".to_string()))
    );
    response.stages.insert(
        "artifact".to_string(),
        emath_sema::StageStatus::Available("forged".to_string()),
    );
    response.source_hash = "fnv1a64:forged".to_string();
    assert_eq!(response.validate(), Err(LiveAdapterError::MixedSource));
}
