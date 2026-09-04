use emath_core::limits::Limits;
use emath_sema::CompilerSession;

const SOURCE: &str = r#"
emath feature AddCapability:
    schema: "emath.feature-capsule"
    feature_id: "std.capability.math.add"
    semantic_hash: "sha256:b3c48065bed9c3cd20b8132c99e07ec148291c25ef24b31d67dcda58ea0e55a6"
    class: "capability"
    maturity: "proposed"
    summary: "Exact integer addition candidate"
    source: "catalog.capability-add"
    surface: "infix plus"
    semantics: "checked addition in the selected numeric world"
    exactness: "exact or diagnosed"
    effects: "pure"
    worlds: "std.world.exact.int"
    providers: "n/a(local-reference | generic VM implementation)"
    artifacts: "value,diagnostic"
    reference: "authored"
    conformance: "positive,negative,mutation"
    migration: "n/a(initial-capsule | legacy remains separately mapped)"
    authority_target: "capsule-candidate"
    presentation: "aliases=+"
    agent: "owners=language/spec/capabilities;checks=feature_capsules"
    edge: "requires_world -> std.world.exact.int"
    projection: "semantics -> required"
"#;

#[test]
fn generic_feature_shell_admits_candidate_without_live_declaration() {
    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("AddCapability.emath", SOURCE);
    assert_eq!(
        result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        Vec::<&str>::new()
    );
    assert_eq!(result.package.feature_capsules.len(), 1);
    assert!(result.package.declarations.is_empty());
    assert_eq!(
        result.package.feature_capsules[0].feature_id.as_str(),
        "std.capability.math.add"
    );
}

#[test]
fn generic_feature_shell_refuses_versioned_schema_and_class_mismatch() {
    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(
        "BadCapsule.emath",
        &SOURCE
            .replace("emath.feature-capsule", "emath.feature-capsule.v2")
            .replace("class: \"capability\"", "class: \"theory\""),
    );
    assert!(result.diagnostics.has_errors());
    assert!(result.package.feature_capsules.is_empty());
}
