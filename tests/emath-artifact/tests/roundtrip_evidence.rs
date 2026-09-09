//! Durable JSON write-parse equality roundtrip: the write and parse share no code path.
use emath_artifact::{ArtifactError, EVIDENCE_BUNDLE_SCHEMA, EvidenceBundleRecord, evidence_bundle_from_json, write_evidence_bundle};
use emath_core::{SchemaId, content_id_of_str};
use emath_ir::{ClaimVerdict, EvidenceClaim, EvidenceLevel};
use emath_test_harness::Probe;

#[test]
fn probe() {
    let mut p = Probe::new("evidence bundle round-trips typed-equal and refuses truncated escapes");
    let bundle = EvidenceBundleRecord {
        schema: SchemaId(EVIDENCE_BUNDLE_SCHEMA.to_string()),
        bundle_id: content_id_of_str("bundle:roundtrip-demo"),
        source_package: content_id_of_str("package:roundtrip-demo"),
        resolution_plan: content_id_of_str("plan:roundtrip-demo"),
        claims: vec![EvidenceClaim {
            id: "claim-1".into(), statement: "roundtrip demo claim".into(), class: "conformance".into(),
            scope: "write-parse-equality".into(), assumptions: vec!["assumption-a".into()], producer: "roundtrip-demo".into(),
            checker: Some("independent-checker".into()), verdict: ClaimVerdict::Pass, level: EvidenceLevel::E3,
            falsifiers: vec!["falsifier-b".into()], artifacts: vec!["emath/evidence-bundle.json".into()], fresh_until: None,
        }],
        artifact_paths: vec!["emath/artifact-manifest.json".into()],
        reproduction: vec!["cargo test -p emath-artifact-tests --test roundtrip_evidence".into()],
    };
    match evidence_bundle_from_json(&write_evidence_bundle(&bundle)) {
        Ok(parsed) => {
            p.eq("schema", parsed.schema.0.clone(), EVIDENCE_BUNDLE_SCHEMA.to_string());
            p.eq("typed-equal", parsed, bundle);
        }
        Err(e) => {
            p.fail("roundtrip", format!("must parse back: {e:?}"));
        }
    }
    match evidence_bundle_from_json("{\"schema\": \"emath.evidence-\\u12") {
        Err(ArtifactError::ManifestMalformed(_)) => p.demand("malformed", true, "ok"),
        other => p.fail("malformed", format!("must be ManifestMalformed, got {other:?}")),
    };
    p.finish();
}
