//! `emath-store` evidence-state schema tests (migrated from
//! `crates/emath-store/src/lib.rs`).

use emath_core::{
    ArtifactId, EvidenceId, MeaningId, PackId, RecipeId, SnapshotId, SourceId, ViewId,
};
use emath_ir::{ExprId, MeaningError, NumericProfile, SemanticPackage};
use emath_sema::CompilerSession;
use emath_store::schema::{
    self, CLAIM_STATUS_FAIL, CLAIM_STATUS_OK, CLAIM_STATUS_PENDING, SCHEMA_SQL,
};

#[test]
fn schema_is_deterministic_ddl() {
    // DDL is a compile-time constant: the schema text is stable across
    // builds. Both tables and the closed status CHECK must be present.
    assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS artifacts ("));
    assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS evidence ("));
    assert!(SCHEMA_SQL.contains("status IN ('ok', 'fail', 'pending')"));
    assert!(SCHEMA_SQL.contains("PRIMARY KEY (artifact_id, claim, seq)"));
}

#[test]
fn claim_status_validation_boundaries() {
    assert!(schema::valid_claim_status(CLAIM_STATUS_OK));
    assert!(schema::valid_claim_status(CLAIM_STATUS_FAIL));
    assert!(schema::valid_claim_status(CLAIM_STATUS_PENDING));
    assert!(!schema::valid_claim_status(""));
    assert!(!schema::valid_claim_status("okay"));
    assert!(!schema::valid_claim_status("OK"));
    assert!(!schema::valid_claim_status(" pending"));
}

#[test]
fn identity_domains_are_versioned_and_separated() {
    let payload = b"same canonical payload";
    let source = SourceId::from_bytes(payload);
    let meaning = MeaningId::from_bytes(payload);
    let evidence = EvidenceId::from_bytes(payload);
    let view = ViewId::from_bytes(payload);
    let recipe = RecipeId::from_bytes(payload);
    let artifact = ArtifactId::from_bytes(payload);
    let snapshot = SnapshotId::from_bytes(payload);
    let pack = PackId::from_bytes(payload);

    assert!(source.as_str().starts_with(SourceId::PREFIX));
    assert!(meaning.as_str().starts_with(MeaningId::PREFIX));
    assert!(evidence.as_str().starts_with(EvidenceId::PREFIX));
    assert!(view.as_str().starts_with(ViewId::PREFIX));
    assert!(recipe.as_str().starts_with(RecipeId::PREFIX));
    assert!(artifact.as_str().starts_with(ArtifactId::PREFIX));
    assert!(snapshot.as_str().starts_with(SnapshotId::PREFIX));
    assert!(pack.as_str().starts_with(PackId::PREFIX));

    let identities = [
        source.as_str(),
        meaning.as_str(),
        evidence.as_str(),
        view.as_str(),
        recipe.as_str(),
        artifact.as_str(),
        snapshot.as_str(),
        pack.as_str(),
    ];
    for (index, identity) in identities.iter().enumerate() {
        assert_eq!(identity.len(), identities[index].rfind(':').unwrap() + 65);
        assert!(!identities[index + 1..].contains(identity));
    }
}

#[test]
fn source_mutation_changes_identity_and_wire_verification() {
    assert_eq!(
        SourceId::from_bytes(b"").as_str(),
        "emath:source:v1:acd0aeb36f91ce4893b33dae198e072135f61d5dbcb5e88899dd01ffc1cb0716"
    );
    let original = SourceId::from_bytes(b"emath function square:\n  return x * x\n");
    let mutated = SourceId::from_bytes(b"emath function square:\n  return x * x!");
    assert_ne!(original, mutated);

    let encoded = original.to_string();
    assert_eq!(encoded.parse::<SourceId>().unwrap(), original);
    assert!(
        encoded
            .replace("emath:source:", "emath:meaning:")
            .parse::<SourceId>()
            .is_err()
    );
    assert!(encoded.to_uppercase().parse::<SourceId>().is_err());
    assert!(encoded[..encoded.len() - 1].parse::<SourceId>().is_err());
}

fn admit(source: &str) -> SemanticPackage {
    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned("meaning-id.emath", source);
    let errors = result
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "source must admit: {errors:#?}");
    result.package
}

#[test]
fn presentation_and_alpha_renames_share_meaning() {
    let left = admit(
        "emath function f:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n",
    );
    let right = admit(
        "# same admitted mathematics\n\nemath function square:\n    inputs:\n        value: Float64\n    definitions:\n        result = value * value\n",
    );
    assert_eq!(
        left.meaning_id(&[]).unwrap(),
        right.meaning_id(&[]).unwrap()
    );

    let sum_i = admit(
        "emath function Sum:\n    outputs:\n        total: Float64\n    definitions:\n        total = sum i in 1..6: i\n",
    );
    let sum_k = admit(
        "emath function Renamed:\n    outputs:\n        answer: Float64\n    definitions:\n        answer = sum k in 1..6: k\n",
    );
    assert_eq!(
        sum_i.meaning_id(&[]).unwrap(),
        sum_k.meaning_id(&[]).unwrap()
    );
}

#[test]
fn notation_aliases_share_meaning() {
    let glyph = admit(
        r#"notation infixl 40 "⊕" => core::math::pow alias "pw"
emath function Power:
    inputs:
        x: Float64
        y: Float64
    definitions:
        result = x⊕y
"#,
    );
    let alias = admit(
        r#"notation infixl 40 "⊕" => core::math::pow alias "pw"
emath function PowerAlias:
    inputs:
        a: Float64
        b: Float64
    definitions:
        value = a pw b
"#,
    );
    assert_eq!(
        glyph.meaning_id(&[]).unwrap(),
        alias.meaning_id(&[]).unwrap()
    );
}

#[test]
fn semantic_policy_and_dependencies_change_meaning() {
    let base = admit(
        "emath function Square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n",
    );
    let changed = admit(
        "emath function Square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x + 1.0\n",
    );
    assert_ne!(
        base.meaning_id(&[]).unwrap(),
        changed.meaning_id(&[]).unwrap()
    );

    let mut prose = base.clone();
    prose.declarations[0].about = Some("non-authoritative documentation".to_string());
    assert_eq!(
        base.meaning_id(&[]).unwrap(),
        prose.meaning_id(&[]).unwrap()
    );

    let mut interval = base.clone();
    interval.declarations[0].compile_spec.numeric = NumericProfile::IntervalF64;
    assert_ne!(
        base.meaning_id(&[]).unwrap(),
        interval.meaning_id(&[]).unwrap()
    );

    let dependency = MeaningId::from_bytes(b"dependency meaning");
    assert_ne!(
        base.meaning_id(&[]).unwrap(),
        base.meaning_id(std::slice::from_ref(&dependency)).unwrap()
    );
    let other_dependency = MeaningId::from_bytes(b"other dependency meaning");
    assert_eq!(
        base.meaning_id(&[dependency.clone(), other_dependency.clone(), dependency])
            .unwrap(),
        base.meaning_id(&[
            other_dependency,
            MeaningId::from_bytes(b"dependency meaning")
        ])
        .unwrap()
    );

    let mut malformed = base;
    *malformed.declarations[0]
        .definitions
        .values_mut()
        .next()
        .unwrap() = ExprId(u32::MAX);
    assert!(matches!(
        malformed.meaning_id(&[]),
        Err(MeaningError::MissingExpr(ExprId(u32::MAX)))
    ));
}
