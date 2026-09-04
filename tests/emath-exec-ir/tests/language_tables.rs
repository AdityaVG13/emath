use std::collections::BTreeMap;
use std::str::FromStr;

use emath_core::{FeatureId, SemanticHash};
use emath_exec_ir::language_tables::{TableError, generate_runtime_tables};
use emath_ir::{CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity};

fn capsule(
    id: &str,
    class: FeatureClass,
    handle: &str,
    presentation: &str,
    surface: &str,
) -> FeatureCapsule {
    FeatureCapsule {
        schema: FEATURE_CAPSULE_SCHEMA.to_string(),
        feature_id: FeatureId::from_str(id).unwrap(),
        semantic_hash: SemanticHash::from_str(&format!("sha256:{}", "1".repeat(64))).unwrap(),
        class,
        maturity: Maturity::Proposed,
        summary: id.to_string(),
        source: format!("language/spec/{id}.emath"),
        edges: vec![],
        slots: BTreeMap::from([
            (
                "semantics".to_string(),
                CapsuleSlot::Value(handle.to_string()),
            ),
            (
                "presentation".to_string(),
                CapsuleSlot::Value(presentation.to_string()),
            ),
            (
                "surface".to_string(),
                CapsuleSlot::Value(surface.to_string()),
            ),
        ]),
        projections: vec![],
    }
}

#[test]
fn seven_runtime_table_families_generate_deterministically() {
    let capsules = vec![
        capsule(
            "std.symbol.math.add",
            FeatureClass::Symbol,
            "add",
            "aliases=+",
            "infix;precedence=60",
        ),
        capsule(
            "std.binder.sum",
            FeatureClass::Binder,
            "fold-add",
            "aliases=sum",
            "binder",
        ),
        capsule(
            "std.kind.function",
            FeatureClass::Kind,
            "function",
            "none",
            "declaration",
        ),
        capsule(
            "std.diagnostic.exactness_loss",
            FeatureClass::Diagnostic,
            "exactness-loss",
            "none",
            "diagnostic",
        ),
        capsule(
            "std.world.exact.int",
            FeatureClass::World,
            "exact-int",
            "none",
            "world",
        ),
        capsule(
            "std.provider.reference",
            FeatureClass::Provider,
            "reference",
            "none",
            "provider",
        ),
        capsule(
            "std.capability.math.add",
            FeatureClass::Capability,
            "checked-add",
            "none",
            "capability",
        ),
    ];
    let first = generate_runtime_tables(&capsules).unwrap();
    let second = generate_runtime_tables(&capsules).unwrap();
    assert_eq!(first, second);
    first.verify().unwrap();
    for name in [
        "symbols",
        "binders",
        "kinds-sections",
        "diagnostics",
        "worlds",
        "providers",
        "capabilities",
    ] {
        assert!(first.tables.contains_key(name), "missing {name}");
    }
    assert!(
        first
            .bytes
            .starts_with("# @generated from Feature Capsules; DO NOT EDIT")
    );
    assert!(
        first
            .bytes
            .contains("source=language/spec/std.symbol.math.add.emath")
    );
}

#[test]
fn stale_alias_duplicate_precedence_and_unsafe_mutations_refuse() {
    let add = capsule(
        "std.symbol.math.add",
        FeatureClass::Symbol,
        "add",
        "aliases=+",
        "infix;precedence=60",
    );
    assert_eq!(
        generate_runtime_tables(&[add.clone(), add.clone()]),
        Err(TableError::DuplicateFeature(add.feature_id.clone()))
    );
    let other = capsule(
        "std.symbol.math.plus",
        FeatureClass::Symbol,
        "add",
        "aliases=+",
        "infix;precedence=60",
    );
    assert_eq!(
        generate_runtime_tables(&[add.clone(), other]),
        Err(TableError::AliasCollision("+".to_string()))
    );
    let no_alias = capsule(
        "std.symbol.math.times",
        FeatureClass::Symbol,
        "mul",
        "none",
        "infix;precedence=70",
    );
    assert!(matches!(
        generate_runtime_tables(&[no_alias]),
        Err(TableError::PrecedenceAmbiguity(_))
    ));
    let provider = capsule(
        "std.provider.native",
        FeatureClass::Provider,
        "vendor::Native<T>",
        "none",
        "provider",
    );
    assert_eq!(
        generate_runtime_tables(&[provider]),
        Err(TableError::UnsafeGeneratedText)
    );
    let mut generated = generate_runtime_tables(&[add]).unwrap();
    generated.bytes.push_str("manual edit");
    assert_eq!(generated.verify(), Err(TableError::StaleLock));
}
