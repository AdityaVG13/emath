use emath_schema::parse_feature_capsule;
use std::fs;

#[test]
fn twelve_physics_catalog_ids_are_distinct_nonlive_capsules() {
    let source = fs::read_to_string("../../language/spec/kinds/physics.emath").unwrap();
    let docs = source
        .split("\nemath feature ")
        .enumerate()
        .filter_map(|(i, p)| (i > 0).then(|| format!("emath feature {p}")))
        .collect::<Vec<_>>();
    assert_eq!(docs.len(), 12);
    let mut ids = std::collections::BTreeSet::new();
    for doc in docs {
        let (capsule, issues) = parse_feature_capsule(&doc);
        assert!(issues.is_empty(), "{issues:?}");
        let capsule = capsule.unwrap();
        assert!(ids.insert(capsule.feature_id.clone()));
        assert_eq!(capsule.maturity, emath_ir::Maturity::Cataloged);
        assert!(capsule.has_blocking_hole());
    }
    assert!(source.contains("wave-16-language-gap-quantum-system-declaration"));
    assert!(source.contains("wave-16-language-gap-benchmark-declaration"));
    assert!(!source.contains("authority_target: \"capsule-active\""));
}
