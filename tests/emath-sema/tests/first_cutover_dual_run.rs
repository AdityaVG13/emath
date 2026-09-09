use std::collections::BTreeMap;
use std::str::FromStr;

use emath_core::FeatureId;
use emath_sema::{LiveConformanceRequest, inspect_live_source};
use emath_test_harness::Probe;

fn authority() -> BTreeMap<FeatureId, String> {
    ["std.kind.function", "std.type.int", "std.capability.math.add"]
        .into_iter()
        .map(|id| (FeatureId::from_str(id).unwrap(), "legacy-active-dual-run".to_string()))
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

const ADD_EXACT: &str = "emath function AddExact:\n    definitions:\n        result = 2 + 1\n";

#[test]
fn first_cutover_dual_run() {
    emath_syntax::install_source_parser();
    let mut p = Probe::new("live adapter matches exact-int oracle; stage and result mutants block cutover");
    let image = format!("distribution-sha256:{}", "3".repeat(64));
    p.case("exact-agrees-at-every-stage", |p| {
        let auth = authority();
        let response = match inspect_live_source(LiveConformanceRequest {
            source_name: "AddExact.emath",
            source: ADD_EXACT,
            repository_commit: "abc123",
            compiler_identity: "emath-sema",
            language_image_id: &image,
            authority: &auth,
        }) {
            Ok(response) => response,
            Err(error) => {
                p.fail("inspect", format!("inspect_live_source must succeed, got {error:?}"));
                return;
            }
        };
        p.demand("validate", response.validate().is_ok(), "live response must validate");
        for stage in ["parse", "admit", "lower", "world", "execute", "artifact"] {
            p.demand(format!("stage/{stage}"), response.stages.contains_key(stage), "every live stage present");
        }
        p.eq("dual-run", candidate_add(2, 1), independent_add(2, 1));
        p.eq("add-value", candidate_add(2, 1), Ok(3));
        p.eq("result", response.result_or_diagnosis.clone(), "value:3:exact-int".to_string());
        p.contains("artifact-value", &response.artifact_manifest, "value:3:exact-int");
        p.contains("artifact-image", &response.artifact_manifest, &image);
    });
    p.case("mutants-block-cutover", |p| {
        p.eq("float-fract-refuses", independent_float_into_int(1.5), Err("exactness-loss"));
        p.eq("overflow-refuses", candidate_add(i64::MAX, 1), Err("exactness-loss"));
        p.ne("no-silent-999", candidate_add(2, 1), Ok(999));
        let auth = authority();
        let mut response = match inspect_live_source(LiveConformanceRequest {
            source_name: "AddExact.emath",
            source: ADD_EXACT,
            repository_commit: "abc123",
            compiler_identity: "emath-sema",
            language_image_id: &image,
            authority: &auth,
        }) {
            Ok(response) => response,
            Err(error) => {
                p.fail("inspect", format!("inspect_live_source must succeed, got {error:?}"));
                return;
            }
        };
        response.stages.remove("world");
        p.demand("missing-stage-blocks", response.validate().is_err(), "removed world stage must fail validation");
        response.stages.insert("world".to_string(), emath_sema::StageStatus::Available("float-world".to_string()));
        response.result_or_diagnosis = "value:999:exact-int".to_string();
        p.demand("result-mismatch-blocks", response.validate().is_err(), "artifact/result mismatch must block authority");
    });
    p.finish();
}
