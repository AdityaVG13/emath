use std::collections::BTreeMap;
use std::str::FromStr;

use emath_core::{FeatureId, SemanticHash};
use emath_exec_ir::language_tables::{TableError, generate_runtime_tables};
use emath_ir::{CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity};

use emath_test_harness::Probe;

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

fn seven_runtime_table_families_generate_deterministically(ph: &mut Probe) {
    ph.case("seven_runtime_table_families_generate_deterministically", |ph| {
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
    eq_ref(ph, "1", &(first), &( second));
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
        ph.demand("2", first.tables.contains_key(name), format!( "missing {name}"));
    }
    ph.demand("3", 
        first
            .bytes
            .starts_with("# @generated from Feature Capsules; DO NOT EDIT")
    , "assertion failed: first\n            .bytes\n            .starts_with(\"# @generated from Feature Cap");
    ph.demand("4", 
        first
            .bytes
            .contains("source=language/spec/std.symbol.math.add.emath")
    , "assertion failed: first\n            .bytes\n            .contains(\"source=language/spec/std.symbol.");

    });
}

fn stale_alias_duplicate_precedence_and_unsafe_mutations_refuse(ph: &mut Probe) {
    ph.case("stale_alias_duplicate_precedence_and_unsafe_mutations_refuse", |ph| {
    let add = capsule(
        "std.symbol.math.add",
        FeatureClass::Symbol,
        "add",
        "aliases=+",
        "infix;precedence=60",
    );
    eq_ref(ph, "1", &(
        generate_runtime_tables(&[add.clone(), add.clone()])), &(
        Err(TableError::DuplicateFeature(add.feature_id.clone()))
    ));
    let other = capsule(
        "std.symbol.math.plus",
        FeatureClass::Symbol,
        "add",
        "aliases=+",
        "infix;precedence=60",
    );
    eq_ref(ph, "2", &(
        generate_runtime_tables(&[add.clone(), other])), &(
        Err(TableError::AliasCollision("+".to_string()))
    ));
    let no_alias = capsule(
        "std.symbol.math.times",
        FeatureClass::Symbol,
        "mul",
        "none",
        "infix;precedence=70",
    );
    ph.demand("3", matches!(
        generate_runtime_tables(&[no_alias]),
        Err(TableError::PrecedenceAmbiguity(_))
    ), "assertion failed: matches!(\n        generate_runtime_tables(&[no_alias]),\n        Err(TableError::");
    let provider = capsule(
        "std.provider.native",
        FeatureClass::Provider,
        "vendor::Native<T>",
        "none",
        "provider",
    );
    eq_ref(ph, "4", &(
        generate_runtime_tables(&[provider])), &(
        Err(TableError::UnsafeGeneratedText)
    ));
    let mut generated = generate_runtime_tables(&[add]).unwrap();
    generated.bytes.push_str("manual edit");
    eq_ref(ph, "5", &(generated.verify()), &( Err(TableError::StaleLock)));

    });
}

#[test]
fn runtime_table_contracts() {
    let mut ph = Probe::new("runtime language tables generate deterministically, share only declared aliases, and refuse drift or unsafe mutations");
    seven_runtime_table_families_generate_deterministically(&mut ph);
    stale_alias_duplicate_precedence_and_unsafe_mutations_refuse(&mut ph);
    ph.finish();
}
