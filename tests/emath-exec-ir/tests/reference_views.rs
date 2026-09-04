use std::collections::BTreeMap;
use std::str::FromStr;

use emath_core::{FeatureId, SemanticHash};
use emath_exec_ir::reference_views::{ReferenceViewError, generate_reference_views};
use emath_ir::{CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity};

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

#[test]
fn factual_views_are_deterministic_locked_and_cross_linked() {
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
    assert_eq!(first, second);
    first.verify().unwrap();
    assert!(first.pages["feature-index.md"].contains("std.capability.math.add"));
    assert!(first.pages["diagnostics.md"].contains("std.diagnostic.exactness_loss"));
    assert!(first.pages["coverage.md"].contains("std.world.exact.int"));
    assert!(first.pages["gap-radar.md"].contains("complete publication gates"));
}

#[test]
fn unsupported_claims_manual_edits_and_stale_locks_refuse() {
    let cataloged = capsule(
        "std.capability.math.add",
        FeatureClass::Capability,
        Maturity::Cataloged,
    );
    assert_eq!(
        generate_reference_views(
            &[cataloged],
            &BTreeMap::from([(
                "std.capability.math.add".to_string(),
                "capsule-active".to_string()
            )])
        ),
        Err(ReferenceViewError::CatalogClaimedLive(
            "std.capability.math.add".to_string()
        ))
    );
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
    assert_eq!(views.verify(), Err(ReferenceViewError::StaleLock));
    views
        .pages
        .get_mut("feature-index.md")
        .unwrap()
        .replace_range(..4, "EDIT");
    assert_eq!(
        views.verify(),
        Err(ReferenceViewError::ManualEdit(
            "feature-index.md".to_string()
        ))
    );
}
