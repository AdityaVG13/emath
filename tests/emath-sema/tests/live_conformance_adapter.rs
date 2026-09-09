//! Live source-to-artifact conformance: real stages, discriminating diagnoses, never complete on forged evidence.
use std::collections::BTreeMap;
use std::str::FromStr;

use emath_core::FeatureId;
use emath_sema::{
    LiveAdapterError, LiveConformanceRequest, LiveConformanceResponse, StageStatus, inspect_live_source,
};
use emath_test_harness::Probe;

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

fn image() -> String {
    format!("distribution-sha256:{}", "1".repeat(64))
}

const ADD_EXACT: &str = "emath function AddExact:\n    definitions:\n        result = 2 + 1\n";

fn inspect(p: &mut Probe, name: &str, request: LiveConformanceRequest<'_>) -> Option<LiveConformanceResponse> {
    match inspect_live_source(request) {
        Ok(response) => Some(response),
        Err(error) => {
            p.fail(name, format!("introspection must succeed, got {error:?}"));
            None
        }
    }
}

#[test]
fn live_conformance_adapter() {
    // No boot(): the adapter's tiny-exact oracle reads the legacy
    // `Binary { ExactAdd }` lowering, and installing the capsule
    // distribution would reroute `+` to `Apply` (see
    // language_feature_lowering.rs). Parser install only, as before.
    emath_syntax::install_source_parser();
    let mut p = Probe::new(
        "live introspection reports real parse/admit/lower/execute/artifact stages and never looks complete on forged evidence",
    );
    p.case("add-exact-stages", |p| {
        let image = image();
        let auth = authority();
        let first = match inspect(
            &mut *p,
            "first",
            LiveConformanceRequest {
                source_name: "AddExact.emath",
                source: ADD_EXACT,
                repository_commit: "abc123",
                compiler_identity: "emath-sema",
                language_image_id: &image,
                authority: &auth,
            },
        ) {
            Some(first) => first,
            None => return,
        };
        let second = match inspect(
            &mut *p,
            "second",
            LiveConformanceRequest {
                source_name: "AddExact.emath",
                source: ADD_EXACT,
                repository_commit: "abc123",
                compiler_identity: "emath-sema",
                language_image_id: &image,
                authority: &auth,
            },
        ) {
            Some(second) => second,
            None => return,
        };
        p.eq("source-hash", &first.source_hash, &second.source_hash);
        p.eq("cst-identity", &first.cst_identity, &second.cst_identity);
        p.eq("result", first.result_or_diagnosis.as_str(), "value:3:exact-int");
        p.eq("validate", first.validate(), Ok(()));
        p.demand(
            "manifest",
            first.artifact_manifest.contains(&first.source_hash),
            "artifact manifest must bind the source hash",
        );
        p.eq("features", first.resolved_features.len(), 3);
    });
    p.case("forged-evidence", |p| {
        let image = image();
        let auth = authority();
        p.eq(
            "bad-commit",
            inspect_live_source(LiveConformanceRequest {
                source_name: "forged.emath",
                source: "emath function f:\n",
                repository_commit: "not-a-commit",
                compiler_identity: "emath-sema",
                language_image_id: &image,
                authority: &auth,
            }),
            Err(LiveAdapterError::InvalidCommit),
        );
        let mut response = match inspect(
            &mut *p,
            "float-into-int",
            LiveConformanceRequest {
                source_name: "FloatIntoInt.emath",
                source: "emath function FloatIntoInt:\n    outputs:\n        result: Int\n    definitions:\n        result = 1.5\n",
                repository_commit: "abc123",
                compiler_identity: "emath-sema",
                language_image_id: &image,
                authority: &auth,
            },
        ) {
            Some(response) => response,
            None => return,
        };
        response.stages.remove("artifact");
        p.eq(
            "partial-claim",
            response.validate(),
            Err(LiveAdapterError::PartialClaim("artifact".to_string())),
        );
        response.stages.insert(
            "artifact".to_string(),
            StageStatus::Available("forged".to_string()),
        );
        response.source_hash = "fnv1a64:forged".to_string();
        p.eq(
            "mixed-source",
            response.validate(),
            Err(LiveAdapterError::MixedSource),
        );
    });
    p.case("int-overflow", |p| {
        let image = image();
        let auth = authority();
        match inspect(
            &mut *p,
            "int-overflow",
            LiveConformanceRequest {
                source_name: "IntOverflow.emath",
                source: "emath function IntOverflow:\n    definitions:\n        result = 9223372036854775807 + 1\n",
                repository_commit: "abc123",
                compiler_identity: "emath-sema",
                language_image_id: &image,
                authority: &auth,
            },
        ) {
            Some(report) => {
                p.eq("diagnosis", report.result_or_diagnosis.as_str(), "diagnosis:E-INT-OVERFLOW");
                p.eq("validate", report.validate(), Ok(()));
            }
            None => {}
        }
    });
    p.case("mutation-control", |p| {
        let image = image();
        let auth = authority();
        match inspect(
            &mut *p,
            "mutation-control",
            LiveConformanceRequest {
                source_name: "AddExact.emath",
                source: ADD_EXACT,
                repository_commit: "abc123",
                compiler_identity: "emath-sema",
                language_image_id: &image,
                authority: &auth,
            },
        ) {
            Some(report) => {
                p.eq("result", report.result_or_diagnosis.as_str(), "value:3:exact-int");
                p.ne("discriminates", report.result_or_diagnosis.as_str(), "value:999:exact-int");
            }
            None => {}
        }
    });
    p.finish();
}
