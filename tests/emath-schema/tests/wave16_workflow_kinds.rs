use emath_schema::parse_feature_capsule;
use std::fs;

#[test]
fn nine_workflow_catalog_ids_are_distinct_nonlive_capsules() {
    let source = fs::read_to_string("../../language/spec/kinds/workflow.emath").unwrap();
    let docs = source
        .split("\nemath feature ")
        .skip(1)
        .map(|p| format!("emath feature {p}"))
        .collect::<Vec<_>>();
    assert_eq!(docs.len(), 9);
    let mut ids = std::collections::BTreeSet::new();
    for doc in docs {
        let (capsule, issues) = parse_feature_capsule(&doc);
        assert!(issues.is_empty(), "{issues:?}");
        let capsule = capsule.unwrap();
        assert!(ids.insert(capsule.feature_id.clone()));
        assert!(capsule.has_blocking_hole());
    }
    for id in [
        "artifact-family",
        "proof-strategy",
        "solver-portfolio",
        "standard",
        "notebook",
        "course",
        "world-bridge",
        "multiworld",
        "digital-twin",
    ] {
        assert!(source.contains(&format!("wave-16-language-gap-{id}-declaration")));
    }
    assert!(!source.contains("authority_target: \"capsule-active\""));
}
