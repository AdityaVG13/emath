use emath_schema::parse_feature_capsule;
use emath_test_harness::{Probe, workspace_file};

#[test]
fn catalog_to_capsule_status_and_template() {
    let mut p = Probe::new(
        "implemented vs cataloged stay distinct; template requires exact ids/owner/conformance/rollback and forbids parser-branch claims",
    );
    let implemented = workspace_file("language/spec/capabilities/core/add.emath");
    let hole_doc = r#"emath feature SpaceKind:
    schema: "emath.feature-capsule"
    feature_id: "std.kind.space"
    class: "kind"
    semantic_hash: "sha256:0298ac3385cbe4ab0da1225310c99dcbf305c9c8dcc8b4bf066ab8fd2cd22272"
    maturity: "cataloged"
    summary: "space declaration kind"
    source: "wave-16-language-gap-space-declaration"
    surface: "emath space Name: suite"
    semantics: "hole(kind-semantics | requires field-pack theory and world contracts)"
    exactness: "n/a(structural-kind | no numeric result)"
    effects: "pure declaration data"
    worlds: "n/a(structural-kind | world selected by instances)"
    providers: "n/a(structural-kind | no provider required to catalog)"
    artifacts: "source,diagnostic"
    reference: "authored"
    conformance: "positive:generic-shell;negative:unknown-section;mutation:class-mismatch"
    migration: "n/a(initial-capsule | no prior authority)"
    authority_target: "capsule-candidate"
    presentation: "aliases=space"
    agent: "owners=language/spec/kinds/geometry.emath;prerequisites=std.syntax.declaration.generic;hazards=catalog-as-support;edits=capsule,conformance;checks=geometry-kind-dumps-absent"
    edge: "depends_on -> std.syntax.declaration.generic"
    projection: "semantics -> hole(kind-semantics | unresolved semantic contract)"
"#;
    let (add, issues) = parse_feature_capsule(&implemented);
    p.demand("add-issues", issues.is_empty(), format!("{issues:?}"));
    match add {
        Some(add) => { p.eq("add-maturity", add.maturity, emath_ir::Maturity::Stable); }
        None => { p.fail("add", "add capsule must parse"); }
    }
    let (hole, issues) = parse_feature_capsule(hole_doc);
    p.demand("hole-issues", issues.is_empty(), format!("{issues:?}"));
    match hole {
        Some(hole) => {
            p.eq("hole-maturity", hole.maturity, emath_ir::Maturity::Cataloged);
            p.demand("blocking-hole", hole.has_blocking_hole(), "cataloged must block");
        }
        None => { p.fail("hole", "inline cataloged capsule must parse"); }
    }
    let template = workspace_file("language/templates/catalog-to-capsule.emath");
    for required in [
        "EXACT_CATALOG_ID",
        "AUTHORITY.CLASS.PATH",
        "positive:CASE",
        "negative:CASE",
        "mutation:CASE",
        "migration:CASE",
        "owners=FILES",
        "prerequisites=FEATURE IDS",
        "authority_target",
        "projection",
    ] {
        p.contains(required, &template, required);
    }
    for forbidden in ["parser branch", "core op variant", "category is supported"] {
        p.demand(
            format!("forbid-{forbidden}"),
            !template.contains(forbidden),
            format!("template must not contain {forbidden}"),
        );
    }
    p.finish();
}
