use emath_schema::parse_feature_capsule;
use std::fs;

#[test]
fn fifteen_catalog_ids_map_to_distinct_nonlive_kind_capsules() {
    let source = fs::read_to_string("../../language/spec/kinds/geometry.emath").unwrap();
    let documents = source
        .split("\nemath feature ")
        .enumerate()
        .map(|(index, part)| {
            if index == 0 {
                part.to_string()
            } else {
                format!("emath feature {part}")
            }
        })
        .filter(|part| part.contains("emath feature "))
        .collect::<Vec<_>>();
    assert_eq!(documents.len(), 15);
    let mut ids = std::collections::BTreeSet::new();
    for document in documents {
        let (capsule, issues) = parse_feature_capsule(&document);
        assert!(issues.is_empty(), "{issues:?}");
        let capsule = capsule.unwrap();
        assert!(ids.insert(capsule.feature_id.clone()));
        assert_eq!(capsule.maturity, emath_ir::Maturity::Cataloged);
        assert!(capsule.has_blocking_hole());
    }
}

#[test]
fn category_is_not_claimed_live_from_one_sample() {
    let source = fs::read_to_string("../../language/spec/kinds/geometry.emath").unwrap();
    assert!(!source.contains("maturity: \"stable\""));
    assert!(!source.contains("authority_target: \"capsule-active\""));
    assert!(source.contains("wave-16-language-gap-space-declaration"));
    assert!(source.contains("wave-16-language-gap-natural-transformation-declaration"));
}
