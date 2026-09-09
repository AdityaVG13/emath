use emath_core::{FeatureId, SemanticHash};
use emath_exec_ir::language_tables::generate_runtime_tables;
use emath_ir::{CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity};
use std::collections::BTreeMap;
use std::str::FromStr;

use emath_test_harness::Probe;

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


/// assert_eq/assert_ne semantics over references: borrows both operands
/// (like the macros) and allows PartialEq between distinct types.
fn eq_ref<T: ?Sized + std::fmt::Debug, U: ?Sized + std::fmt::Debug>(
    ph: &mut Probe,
    name: impl Into<String>,
    actual: &T,
    expected: &U,
) where
    T: PartialEq<U>,
{
    ph.demand(name, actual == expected, format!("expected {expected:?}, got {actual:?}"));
}

fn ne_ref<T: ?Sized + std::fmt::Debug, U: ?Sized + std::fmt::Debug>(
    ph: &mut Probe,
    name: impl Into<String>,
    actual: &T,
    unexpected: &U,
) where
    T: PartialEq<U>,
{
    ph.demand(name, actual != unexpected, format!("got forbidden value {unexpected:?}"));
}

fn stage_one_and_two_language_table_locks_agree(ph: &mut Probe) {
    ph.case("stage_one_and_two_language_table_locks_agree", |ph| {
    let genome = vec![capsule()];
    let stage1 = generate_runtime_tables(&genome).unwrap();
    let stage2 = generate_runtime_tables(&genome).unwrap();
    eq_ref(ph, "1", &(stage1.lock), &( stage2.lock));
    eq_ref(ph, "2", &(stage1.bytes), &( stage2.bytes));
    let mut mutated = genome;
    mutated[0].slots.insert(
        "semantics".into(),
        CapsuleSlot::Value("wrapping-add".into()),
    );
    ne_ref(ph, "3", &(stage1.lock), &( generate_runtime_tables(&mutated).unwrap().lock));

    });
}

#[test]
fn genome_bootstrap_contracts() {
    let mut ph = Probe::new("runtime language-table lock is deterministic under one genome and drifts when the semantics slot changes");
    stage_one_and_two_language_table_locks_agree(&mut ph);
    ph.finish();
}
