use emath_core::{FeatureId, SemanticHash};
use emath_exec_ir::language_tables::generate_runtime_tables;
use emath_ir::{CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity};
use std::collections::BTreeMap;
use std::str::FromStr;

fn capsule() -> FeatureCapsule {
    FeatureCapsule {
        schema: FEATURE_CAPSULE_SCHEMA.to_string(),
        feature_id: FeatureId::from_str("std.capability.math.add").unwrap(),
        semantic_hash: SemanticHash::from_str(&format!("sha256:{}", "5".repeat(64))).unwrap(),
        class: FeatureClass::Capability,
        maturity: Maturity::Stable,
        summary: "add".into(),
        source: "add.emath".into(),
        edges: vec![],
        slots: BTreeMap::from([
            ("semantics".into(), CapsuleSlot::Value("checked-add".into())),
            (
                "presentation".into(),
                CapsuleSlot::Value("aliases=+".into()),
            ),
            (
                "surface".into(),
                CapsuleSlot::Value("infix;precedence=60".into()),
            ),
        ]),
        projections: vec![],
    }
}

#[test]
fn stage_one_and_two_language_table_locks_agree() {
    let genome = vec![capsule()];
    let stage1 = generate_runtime_tables(&genome).unwrap();
    let stage2 = generate_runtime_tables(&genome).unwrap();
    assert_eq!(stage1.lock, stage2.lock);
    assert_eq!(stage1.bytes, stage2.bytes);
    let mut mutated = genome;
    mutated[0].slots.insert(
        "semantics".into(),
        CapsuleSlot::Value("wrapping-add".into()),
    );
    assert_ne!(stage1.lock, generate_runtime_tables(&mutated).unwrap().lock);
}
