use std::str::FromStr;
use emath_core::{CanonicalField, DistributionHash, FeatureId, FeatureIdErrorKind, HashDomain, LegacyId, LegacyIdKind, LegacyIdMapping, OperationalHash, SemanticHash};
use emath_test_harness::{Case, Probe, check_all};

fn fields<'a>(entries: &'a [(&'a str, &'a str)]) -> Vec<CanonicalField<'a>> {
    entries.iter().map(|(n, v)| CanonicalField::new(n, v.as_bytes()).unwrap()).collect()
}

#[test]
fn probe() {
    let mut p = Probe::new("feature ids stay stable unversioned names with canonical domain-separated hashes");
    if let Err(m) = check_all(
        &[
            Case::new("add", "std.capability.math.add", ("std".to_string(), "capability".to_string(), "math.add".to_string())),
            Case::new("tensor", "acme-labs.field_pack.tensor.linear_algebra", ("acme-labs".to_string(), "field_pack".to_string(), "tensor.linear_algebra".to_string())),
        ],
        |s| {
            let id = FeatureId::from_str(s).unwrap();
            (id.authority().to_string(), id.class().to_string(), id.path().collect::<Vec<_>>().join("."))
        },
    ) {
        p.fail("valid-shapes", m);
    } else {
        p.demand("valid-shapes", true, "ok");
    }
    p.case("stable", |p| {
        let id = FeatureId::from_str("std.capability.math.add").unwrap();
        p.eq("text", id.as_str(), "std.capability.math.add");
        p.eq("bytes", id.canonical_bytes(), "std.capability.math.add".as_bytes());
        p.eq("round-trip", FeatureId::from_str(&id.to_string()).unwrap(), id.clone());
        p.demand("class", id.require_class("capability").is_ok(), "must accept own class");
        p.eq("class-mismatch", id.require_class("operator").unwrap_err().kind(), FeatureIdErrorKind::ClassMismatch);
    });
    if let Err(m) = check_all(
        &[
            Case::new("upper", "Std.capability.math.add", FeatureIdErrorKind::Uppercase),
            Case::new("nfc", "cafe\u{301}.capability.math.add", FeatureIdErrorKind::NotNfc),
            Case::new("non-ascii", "caf\u{e9}.capability.math.add", FeatureIdErrorKind::NonAscii),
            Case::new("version", "std.capability.math.add@1", FeatureIdErrorKind::VersionSuffix),
            Case::new("at-word", "std.capability.math.add@draft", FeatureIdErrorKind::InvalidCharacter),
            Case::new("short", "std.capability", FeatureIdErrorKind::MissingPath),
            Case::new("empty-seg", "std.capability.math..add", FeatureIdErrorKind::EmptySegment),
            Case::new("slash", "std.capability.math/add", FeatureIdErrorKind::InvalidCharacter),
            Case::new("lead-digit", "1std.capability.math.add", FeatureIdErrorKind::InvalidStart),
            Case::new("trailing-dot", "std.capability.math.add.", FeatureIdErrorKind::EmptySegment),
            Case::new("path-digit", "std.capability.2add", FeatureIdErrorKind::InvalidStart),
        ],
        |s| FeatureId::from_str(s).unwrap_err().kind(),
    ) {
        p.fail("refusals", m);
    } else {
        p.demand("refusals", true, "ok");
    }
    p.case("hashes", |p| {
        let sem = SemanticHash::new(&fields(&[("semantics", "checked-add"), ("feature_id", "std.capability.math.add"), ("class", "capability")])).unwrap();
        p.eq("order", sem.clone(), SemanticHash::new(&fields(&[("class", "capability"), ("feature_id", "std.capability.math.add"), ("semantics", "checked-add")])).unwrap());
        p.eq("pinned", sem.to_string(), "sha256:833fb936080d97e5241e3c86dfe23df8e6aaa487b7a95a4c94a996e2559cfaaa".to_string());
        let (ss, ds, os) = (
            SemanticHash::new(&fields(&[("payload", "same")])).unwrap().as_str().to_string(),
            DistributionHash::new(&fields(&[("payload", "same")])).unwrap().as_str().to_string(),
            OperationalHash::new(&fields(&[("payload", "same")])).unwrap().as_str().to_string(),
        );
        p.demand("domains-differ", ss != ds && ss != os && ds != os, "domains must separate");
        p.eq("dist-domain", DistributionHash::new(&fields(&[("image", "capsule-bytes")])).unwrap().domain(), HashDomain::Distribution);
        p.eq("op-domain", OperationalHash::new(&fields(&[("repository_path", "language/std/add.emath")])).unwrap().domain(), HashDomain::Operational);
        p.ne("meaning-matters", sem, SemanticHash::new(&fields(&[("class", "capability"), ("feature_id", "std.capability.math.add")])).unwrap());
        p.eq("op-rejects-sem", OperationalHash::new(&fields(&[("repository_commit", "abc123"), ("semantics", "checked-add")])).unwrap_err().field(), "semantics");
        p.eq("sem-rejects-path", SemanticHash::new(&fields(&[("semantics", "checked-add"), ("repository_path", "/x")])).unwrap_err().field(), "repository_path");
        p.demand("cross-domain", SemanticHash::from_str(ds.as_str()).is_err(), "distribution bytes must not parse as semantic");
        p.eq("dup", SemanticHash::new(&fields(&[("semantics", "a"), ("semantics", "b")])).unwrap_err().field(), "semantics");
        p.demand("upper-field", CanonicalField::new("Schema", b"capsule").is_err(), "field names are snake-case");
    });
    p.case("legacy", |p| {
        let m = LegacyIdMapping::new(LegacyId::new(LegacyIdKind::Fnv1a64, "4a3f78ce09b18d21").unwrap(), FeatureId::from_str("std.capability.math.add").unwrap());
        p.eq("text", m.canonical_text(), "legacy_id=fnv1a64:4a3f78ce09b18d21\nfeature_id=std.capability.math.add\n".to_string());
        p.eq("round-trip", LegacyIdMapping::from_str(&m.canonical_text()).unwrap(), m);
        p.demand("tagged", LegacyIdMapping::from_str("4a3f78ce09b18d21=std.capability.math.add").is_err(), "must require kind tag");
    });
    p.finish();
}
