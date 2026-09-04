use emath_schema::parse_feature_capsule;
use std::fs;

#[test]
fn nineteen_msc_anchors_are_accounted_without_support_claims() {
    let source = fs::read_to_string("../../language/spec/field_packs/msc2020.emath").unwrap();
    let docs = source
        .split("\nemath feature ")
        .skip(1)
        .map(|p| format!("emath feature {p}"))
        .collect::<Vec<_>>();
    assert_eq!(docs.len(), 19);
    let mut ids = std::collections::BTreeSet::new();
    for doc in docs {
        let (capsule, issues) = parse_feature_capsule(&doc);
        assert!(issues.is_empty(), "{issues:?}");
        let capsule = capsule.unwrap();
        assert!(ids.insert(capsule.feature_id.clone()));
        assert_eq!(capsule.class, emath_ir::FeatureClass::FieldPack);
        assert_eq!(capsule.maturity, emath_ir::Maturity::Cataloged);
        assert!(capsule.has_blocking_hole());
    }
    assert!(!source.contains("authority_target: \"capsule-active\""));
    assert!(source.contains("taxonomy-as-support"));
}
