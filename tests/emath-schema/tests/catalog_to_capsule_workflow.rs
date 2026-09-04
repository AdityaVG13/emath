use emath_schema::parse_feature_capsule;
use std::fs;

#[test]
fn implemented_and_unimplemented_records_keep_distinct_status() {
    let implemented =
        fs::read_to_string("../../language/spec/capabilities/core/add.emath").unwrap();
    let unimplemented = fs::read_to_string("../../language/spec/kinds/geometry.emath").unwrap();
    let (add, issues) = parse_feature_capsule(&implemented);
    assert!(issues.is_empty());
    assert_eq!(add.unwrap().maturity, emath_ir::Maturity::Stable);
    let space_doc = format!(
        "emath feature {}",
        unimplemented.split("\nemath feature ").nth(1).unwrap()
    );
    let (space, issues) = parse_feature_capsule(&space_doc);
    assert!(issues.is_empty());
    let space = space.unwrap();
    assert_eq!(space.maturity, emath_ir::Maturity::Cataloged);
    assert!(space.has_blocking_hole());
}

#[test]
fn template_requires_exact_ids_owner_conformance_and_rollback() {
    let template = fs::read_to_string("../../language/templates/catalog-to-capsule.emath").unwrap();
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
        assert!(template.contains(required), "missing {required}");
    }
    for forbidden in ["parser branch", "core op variant", "category is supported"] {
        assert!(!template.contains(forbidden));
    }
}
