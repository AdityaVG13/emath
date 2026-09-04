use std::str::FromStr;

use emath_core::{
    CanonicalField, DistributionHash, FeatureId, FeatureIdErrorKind, HashDomain, LegacyId,
    LegacyIdKind, LegacyIdMapping, OperationalHash, SemanticHash,
};

fn fields<'a>(entries: &'a [(&'a str, &'a str)]) -> Vec<CanonicalField<'a>> {
    entries
        .iter()
        .map(|(name, value)| CanonicalField::new(name, value.as_bytes()).unwrap())
        .collect()
}

#[test]
fn feature_ids_are_stable_unversioned_concept_names() {
    for (raw, authority, class, path) in [
        (
            "std.capability.math.add",
            "std",
            "capability",
            &["math", "add"][..],
        ),
        (
            "acme-labs.field_pack.tensor.linear_algebra",
            "acme-labs",
            "field_pack",
            &["tensor", "linear_algebra"][..],
        ),
    ] {
        let id = FeatureId::from_str(raw).unwrap();
        assert_eq!(id.as_str(), raw);
        assert_eq!(id.authority(), authority);
        assert_eq!(id.class(), class);
        assert_eq!(id.path().collect::<Vec<_>>(), path);
        assert_eq!(id.canonical_bytes(), raw.as_bytes());
        assert_eq!(id.to_string(), raw);
        assert_eq!(FeatureId::from_str(&id.to_string()).unwrap(), id);
        id.require_class(class).unwrap();
    }
}

#[test]
fn malformed_or_versioned_feature_ids_are_refused_without_rewriting() {
    for (raw, expected) in [
        ("Std.capability.math.add", FeatureIdErrorKind::Uppercase),
        (
            "cafe\u{301}.capability.math.add",
            FeatureIdErrorKind::NotNfc,
        ),
        ("café.capability.math.add", FeatureIdErrorKind::NonAscii),
        (
            "std.capability.math.add@1",
            FeatureIdErrorKind::VersionSuffix,
        ),
        (
            "std.capability.math.add@draft",
            FeatureIdErrorKind::InvalidCharacter,
        ),
        ("std.capability", FeatureIdErrorKind::MissingPath),
        ("std.capability.math..add", FeatureIdErrorKind::EmptySegment),
        (
            "std.capability.math/add",
            FeatureIdErrorKind::InvalidCharacter,
        ),
        ("1std.capability.math.add", FeatureIdErrorKind::InvalidStart),
        ("std.capability.math.add.", FeatureIdErrorKind::EmptySegment),
        ("std.capability.2add", FeatureIdErrorKind::InvalidStart),
    ] {
        let error = FeatureId::from_str(raw).unwrap_err();
        assert_eq!(error.kind(), expected, "wrong refusal for {raw:?}: {error}");
    }

    let id = FeatureId::from_str("std.capability.math.add").unwrap();
    assert_eq!(
        id.require_class("operator").unwrap_err().kind(),
        FeatureIdErrorKind::ClassMismatch
    );
}

#[test]
fn feature_hashes_are_canonical_deterministic_and_domain_separated() {
    let semantic_fields = fields(&[
        ("semantics", "checked-add"),
        ("feature_id", "std.capability.math.add"),
        ("class", "capability"),
    ]);
    let reordered_semantic_fields = fields(&[
        ("class", "capability"),
        ("feature_id", "std.capability.math.add"),
        ("semantics", "checked-add"),
    ]);
    let distribution_fields = fields(&[("image", "capsule-bytes"), ("alias", "+")]);
    let operational_fields = fields(&[("repository_path", "language/std/add.emath")]);

    let semantic = SemanticHash::new(&semantic_fields).unwrap();
    assert_eq!(
        semantic,
        SemanticHash::new(&reordered_semantic_fields).unwrap(),
        "canonical field order must not depend on insertion order"
    );
    assert_eq!(
        semantic.to_string(),
        "sha256:833fb936080d97e5241e3c86dfe23df8e6aaa487b7a95a4c94a996e2559cfaaa"
    );

    let same_payload = fields(&[("payload", "same")]);
    let semantic_same = SemanticHash::new(&same_payload).unwrap();
    let distribution_same = DistributionHash::new(&same_payload).unwrap();
    let operational_same = OperationalHash::new(&same_payload).unwrap();
    assert_ne!(semantic_same.as_str(), distribution_same.as_str());
    assert_ne!(semantic_same.as_str(), operational_same.as_str());
    assert_ne!(distribution_same.as_str(), operational_same.as_str());

    assert_eq!(
        DistributionHash::new(&distribution_fields)
            .unwrap()
            .domain(),
        HashDomain::Distribution
    );
    assert_eq!(
        OperationalHash::new(&operational_fields).unwrap().domain(),
        HashDomain::Operational
    );
}

#[test]
fn hash_envelopes_enforce_domain_boundaries_and_semantic_mutations() {
    let original = fields(&[
        ("class", "capability"),
        ("feature_id", "std.capability.math.add"),
        ("semantics", "checked-add"),
    ]);
    let missing_semantics = fields(&[
        ("class", "capability"),
        ("feature_id", "std.capability.math.add"),
    ]);
    assert_ne!(
        SemanticHash::new(&original).unwrap(),
        SemanticHash::new(&missing_semantics).unwrap(),
        "removing meaning must change semantic identity"
    );

    let semantic_with_path = fields(&[
        ("semantics", "checked-add"),
        ("repository_path", "/Users/person/project/add.emath"),
    ]);
    assert_eq!(
        SemanticHash::new(&semantic_with_path).unwrap_err().field(),
        "repository_path"
    );

    let operational_with_semantics = fields(&[
        ("repository_commit", "abc123"),
        ("semantics", "checked-add"),
    ]);
    assert_eq!(
        OperationalHash::new(&operational_with_semantics)
            .unwrap_err()
            .field(),
        "semantics"
    );

    let distribution = DistributionHash::new(&fields(&[("image", "capsule-bytes")])).unwrap();
    assert!(SemanticHash::from_str(distribution.as_str()).is_err());

    for forbidden in ["schema_version", "edition", "major", "minor", "patch"] {
        assert_eq!(
            CanonicalField::new(forbidden, b"1").unwrap_err().field(),
            forbidden
        );
    }
    assert!(CanonicalField::new("Schema", b"capsule").is_err());

    let duplicate = fields(&[("semantics", "checked-add"), ("semantics", "wrapping-add")]);
    assert_eq!(
        SemanticHash::new(&duplicate).unwrap_err().field(),
        "semantics"
    );
}

#[test]
fn legacy_ids_round_trip_only_with_an_explicit_kind_tag() {
    let feature_id = FeatureId::from_str("std.capability.math.add").unwrap();
    let mapping = LegacyIdMapping::new(
        LegacyId::new(LegacyIdKind::Fnv1a64, "4a3f78ce09b18d21").unwrap(),
        feature_id,
    );
    let encoded = mapping.canonical_text();
    assert_eq!(
        encoded,
        "legacy_id=fnv1a64:4a3f78ce09b18d21\nfeature_id=std.capability.math.add\n"
    );
    assert_eq!(LegacyIdMapping::from_str(&encoded).unwrap(), mapping);
    assert!(LegacyIdMapping::from_str("4a3f78ce09b18d21=std.capability.math.add").is_err());
}
