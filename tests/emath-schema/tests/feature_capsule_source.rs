use emath_test_harness::{Probe, Source, boot};

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
fn generic_feature_shell() {
    boot();
    let mut p = Probe::new(
        "feature shell admits a candidate without a live declaration; versioned schema or class mismatch refuses",
    );
    p.case("admits-candidate", |p| {
        let result = Source::from_str("AddCapability.emath", SOURCE).must_admit(p);
        p.eq("capsules", result.package.feature_capsules.len(), 1);
        p.demand("no-decls", result.package.declarations.is_empty(), "no live declaration");
        p.eq(
            "id",
            result.package.feature_capsules[0].feature_id.as_str(),
            "std.capability.math.add",
        );
    });
    p.case("version-class-mismatch-refuses", |p| {
        let mutated = SOURCE
            .replace("emath.feature-capsule", "emath.feature-capsule.v2")
            .replace("class: \"capability\"", "class: \"theory\"");
        let result = Source::from_str("BadCapsule.emath", &mutated).check();
        p.demand("errors", result.diagnostics.has_errors(), "mismatch must refuse");
        p.demand(
            "no-capsule",
            result.package.feature_capsules.is_empty(),
            "refused mismatch must not intern a capsule",
        );
    });
    p.finish();
}
