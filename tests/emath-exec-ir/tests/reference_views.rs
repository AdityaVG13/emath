use std::collections::BTreeMap;
use std::str::FromStr;

use emath_core::{FeatureId, SemanticHash};
use emath_exec_ir::reference_views::{ReferenceViewError, generate_reference_views};
use emath_ir::{CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity};

use emath_test_harness::Probe;

fn capsule(id: &str, class: FeatureClass, maturity: Maturity) -> FeatureCapsule {
    FeatureCapsule {
        schema: FEATURE_CAPSULE_SCHEMA.to_string(),
        feature_id: FeatureId::from_str(id).unwrap(),
        semantic_hash: SemanticHash::from_str(&format!("sha256:{}", "2".repeat(64))).unwrap(),
        class,
        maturity,
        summary: format!("summary for {id}"),
        source: format!("language/spec/{id}.emath"),
        edges: vec![],
        slots: BTreeMap::from([
            (
                "worlds".to_string(),
                CapsuleSlot::Value("std.world.exact.int".to_string()),
            ),
            (
                "providers".to_string(),
                CapsuleSlot::Value("reference-vm".to_string()),
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

fn factual_views_are_deterministic_locked_and_cross_linked(ph: &mut Probe) {
    ph.case("factual_views_are_deterministic_locked_and_cross_linked", |ph| {
    let capsules = vec![
        capsule(
            "std.capability.math.add",
            FeatureClass::Capability,
            Maturity::Stable,
        ),
        capsule(
            "std.diagnostic.exactness_loss",
            FeatureClass::Diagnostic,
            Maturity::Proposed,
        ),
    ];
    let authority = BTreeMap::from([
        (
            "std.capability.math.add".to_string(),
            "capsule-active".to_string(),
        ),
        (
            "std.diagnostic.exactness_loss".to_string(),
            "capsule-candidate".to_string(),
        ),
    ]);
    let first = generate_reference_views(&capsules, &authority).unwrap();
    let second = generate_reference_views(&capsules, &authority).unwrap();
    eq_ref(ph, "1", &(first), &( second));
    first.verify().unwrap();
    ph.demand("2", first.pages["feature-index.md"].contains("std.capability.math.add"), "assertion failed: first.pages[\"feature-index.md\"].contains(\"std.capability.math.add\")");
    ph.demand("3", first.pages["diagnostics.md"].contains("std.diagnostic.exactness_loss"), "assertion failed: first.pages[\"diagnostics.md\"].contains(\"std.diagnostic.exactness_loss\")");
    ph.demand("4", first.pages["coverage.md"].contains("std.world.exact.int"), "assertion failed: first.pages[\"coverage.md\"].contains(\"std.world.exact.int\")");
    ph.demand("5", first.pages["gap-radar.md"].contains("complete publication gates"), "assertion failed: first.pages[\"gap-radar.md\"].contains(\"complete publication gates\")");

    });
}

fn unsupported_claims_manual_edits_and_stale_locks_refuse(ph: &mut Probe) {
    ph.case("unsupported_claims_manual_edits_and_stale_locks_refuse", |ph| {
    let cataloged = capsule(
        "std.capability.math.add",
        FeatureClass::Capability,
        Maturity::Cataloged,
    );
    eq_ref(ph, "1", &(
        generate_reference_views(
            &[cataloged],
            &BTreeMap::from([(
                "std.capability.math.add".to_string(),
                "capsule-active".to_string()
            )])
        )), &(
        Err(ReferenceViewError::CatalogClaimedLive(
            "std.capability.math.add".to_string()
        ))
    ));
    let proposed = capsule(
        "std.capability.math.add",
        FeatureClass::Capability,
        Maturity::Proposed,
    );
    let mut views = generate_reference_views(&[proposed], &BTreeMap::new()).unwrap();
    views
        .pages
        .get_mut("feature-index.md")
        .unwrap()
        .push_str("manual");
    eq_ref(ph, "2", &(views.verify()), &( Err(ReferenceViewError::StaleLock)));
    views
        .pages
        .get_mut("feature-index.md")
        .unwrap()
        .replace_range(..4, "EDIT");
    eq_ref(ph, "3", &(
        views.verify()), &(
        Err(ReferenceViewError::ManualEdit(
            "feature-index.md".to_string()
        ))
    ));

    });
}

#[test]
fn reference_views_contracts() {
    let mut ph = Probe::new("factual reference views are deterministic, locked, cross-linked, and refuse unsupported claims or stale locks");
    factual_views_are_deterministic_locked_and_cross_linked(&mut ph);
    unsupported_claims_manual_edits_and_stale_locks_refuse(&mut ph);
    ph.finish();
}
