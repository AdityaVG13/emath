//! Durable JSON write-parse equality roundtrip (AC G of
//! `emath-conform-harness-thin-lfpg`): `emath.evidence-bundle` was
//! previously untested for typed parse-back. The write and parse share
//! no code path (the parser is an independent reader), so equality on
//! the roundtrip is a real conformance check, not a tautology.

use emath_artifact::{
    ArtifactError, EVIDENCE_BUNDLE_SCHEMA, EvidenceBundleRecord, evidence_bundle_from_json,
    write_evidence_bundle,
};
use emath_core::{ContentId, SchemaId, content_id_of_str};
use emath_ir::{ClaimVerdict, EvidenceClaim, EvidenceLevel};

#[test]
fn evidence_bundle_json_roundtrip_is_typed_equal() {
    let bundle = EvidenceBundleRecord {
        schema: SchemaId(EVIDENCE_BUNDLE_SCHEMA.to_string()),
        bundle_id: content_id_of_str("bundle:roundtrip-demo"),
        source_package: content_id_of_str("package:roundtrip-demo"),
        resolution_plan: content_id_of_str("plan:roundtrip-demo"),
        claims: vec![EvidenceClaim {
            id: "claim-1".into(),
            statement: "roundtrip demo claim".into(),
            class: "conformance".into(),
            scope: "write-parse-equality".into(),
            assumptions: vec!["assumption-a".into()],
            producer: "roundtrip-demo".into(),
            checker: Some("independent-checker".into()),
            verdict: ClaimVerdict::Pass,
            level: EvidenceLevel::E3,
            falsifiers: vec!["falsifier-b".into()],
            artifacts: vec!["emath/evidence-bundle.json".into()],
            fresh_until: None,
        }],
        artifact_paths: vec!["emath/artifact-manifest.json".into()],
        reproduction: vec!["cargo test -p emath-artifact-tests --test roundtrip_evidence".into()],
    };

    let json = write_evidence_bundle(&bundle);
    let parsed =
        evidence_bundle_from_json(&json).expect("typed parse-back must accept the written document");
    assert_eq!(
        parsed, bundle,
        "emath.evidence-bundle must round-trip typed-equal through the independent parser"
    );
    assert_eq!(parsed.schema.0, EVIDENCE_BUNDLE_SCHEMA);
}

/// A truncated `\uXXXX` escape must be a typed malformed refusal, never
/// a panic: the independent parser owns its own escaping rules, so
/// malformed input is part of its contract (pass-5 bounds guard).
#[test]
fn truncated_unicode_escape_is_typed_malformed_not_panic() {
    let malformed = "{\"schema\": \"emath.evidence-\\u12";
    let err = evidence_bundle_from_json(malformed)
        .expect_err("truncated \\u escape must be refused, never panicked");
    assert!(
        matches!(err, ArtifactError::ManifestMalformed(_)),
        "expected ManifestMalformed, got {err:?}"
    );
}
