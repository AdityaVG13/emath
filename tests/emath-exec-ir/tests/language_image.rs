use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use emath_core::{CanonicalField, FeatureId, SemanticHash};
use emath_exec_ir::language_image::{
    FeatureAuthorityEntry, LanguageImage, LanguageImageError, LanguageSourceMapEntry,
    compile_language_directory,
};
use emath_exec_ir::term_compile::ParamShape;
use emath_ir::{
    CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity, MeaningSpine,
};

use emath_test_harness::Probe;

fn id(value: &str) -> FeatureId {
    FeatureId::from_str(value).unwrap()
}

fn capsule(semantics: &str) -> FeatureCapsule {
    let feature_id = id("std.capability.math.add");
    let semantic_hash = SemanticHash::new(&[
        CanonicalField::new("feature_id", feature_id.as_str().as_bytes()).unwrap(),
        CanonicalField::new("semantics", semantics.as_bytes()).unwrap(),
    ])
    .unwrap();
    FeatureCapsule {
        schema: FEATURE_CAPSULE_SCHEMA.to_string(),
        feature_id,
        semantic_hash,
        class: FeatureClass::Capability,
        maturity: Maturity::Proposed,
        summary: "add".to_string(),
        source: "language/spec/capabilities/core/add.emath".to_string(),
        edges: vec![],
        slots: BTreeMap::from([
            (
                "semantics".to_string(),
                CapsuleSlot::Value(semantics.to_string()),
            ),
            (
                "conformance".to_string(),
                CapsuleSlot::Value("test://add".to_string()),
            ),
        ]),
        projections: vec![],
    }
}

/// An authored exact-add reference body in the frozen capsule shape
/// (`reference_params` / `reference_signature` / `reference_body`, mode
/// `authored`).
fn reference_capsule() -> FeatureCapsule {
    let mut capsule =
        capsule("kernel=checked-add;arity=2;inputs=Int,Int;output=Int;exactness=exact");
    capsule.slots.insert(
        "reference".to_string(),
        CapsuleSlot::Value("authored".to_string()),
    );
    capsule.slots.insert(
        "reference_params".to_string(),
        CapsuleSlot::Value("lhs,rhs".to_string()),
    );
    capsule.slots.insert(
        "reference_signature".to_string(),
        CapsuleSlot::Value("add=2".to_string()),
    );
    capsule.slots.insert(
        "reference_body".to_string(),
        CapsuleSlot::Value("apply(add,var(lhs),var(rhs))".to_string()),
    );
    capsule
}

fn build(semantics: &str) -> LanguageImage {
    let capsule = capsule(semantics);
    build_with(&capsule)
}

fn build_with(capsule: &FeatureCapsule) -> LanguageImage {
    try_build_with(capsule).unwrap()
}

fn try_build_with(capsule: &FeatureCapsule) -> Result<LanguageImage, LanguageImageError> {
    let mut spine = MeaningSpine::default();
    spine.register_feature(capsule.feature_id.clone(), capsule.class);
    LanguageImage::build(
        std::slice::from_ref(capsule),
        &spine,
        &BTreeMap::from([("operators".to_string(), "add=checked".to_string())]),
        &[FeatureAuthorityEntry {
            feature_id: capsule.feature_id.clone(),
            state: "capsule-candidate".to_string(),
        }],
        &[LanguageSourceMapEntry {
            feature_id: capsule.feature_id.clone(),
            authored_source: capsule.source.clone(),
        }],
        vec![],
        Some(&[CanonicalField::new("repository_commit", b"abc123").unwrap()]),
    )
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

fn builder_and_loader_are_byte_deterministic_and_traceable(ph: &mut Probe) {
    ph.case("builder_and_loader_are_byte_deterministic_and_traceable", |ph| {
    let first = build("checked-add");
    let second = build("checked-add");
    eq_ref(ph, "1", &(first), &( second));
    first.verify().unwrap();
    ne_ref(ph, "2", &(
        first.semantic_hash.as_str()), &(
        first.distribution_hash.as_str()
    ));
    ne_ref(ph, "3", &(
        first.operational_hash.as_ref().unwrap().as_str()), &(
        first.semantic_hash.as_str()
    ));
    eq_ref(ph, "4", &(
        first.authored_source(&id("std.capability.math.add"))), &(
        Some("language/spec/capabilities/core/add.emath")
    ));
    ph.demand("5", 
        first
            .load_partition("language.tables")
            .unwrap()
            .contains("add=checked")
    , "assertion failed: first\n            .load_partition(\"language.tables\")\n            .unwrap()\n     ");
    ph.demand("6", 
        first
            .load_partition("language.capsules")
            .unwrap()
            .contains("std.capability.math.add")
    , "assertion failed: first\n            .load_partition(\"language.capsules\")\n            .unwrap()\n   ");

    });
}

fn mutations_and_stale_lock_refuse_without_overwriting_prior_image(ph: &mut Probe) {
    ph.case("mutations_and_stale_lock_refuse_without_overwriting_prior_image", |ph| {
    let prior = build("checked-add");
    let changed = build("wrapping-add");
    ne_ref(ph, "1", &(prior.semantic_hash), &( changed.semantic_hash));
    ne_ref(ph, "2", &(prior.distribution_hash), &( changed.distribution_hash));

    let mut stale = prior.clone();
    stale.lock.distribution_hash = changed.distribution_hash.clone();
    eq_ref(ph, "3", &(stale.verify()), &( Err(LanguageImageError::StaleLock)));

    let mut corrupt = prior.clone();
    corrupt.image.partitions[0].body.push_str("tampered");
    ph.demand("4", matches!(
        corrupt.verify(),
        Err(LanguageImageError::CorruptImage(_))
    ), "assertion failed: matches!(\n        corrupt.verify(),\n        Err(LanguageImageError::CorruptImage");

    ph.demand("5", 
        LanguageImage::build(
            &[capsule("checked-add")],
            &MeaningSpine::default(),
            &BTreeMap::new(),
            &[],
            &[],
            vec![prior.distribution_hash.clone()],
            None,
        )
        .is_err()
    , "assertion failed: LanguageImage::build(\n            &[capsule(\"checked-add\")],\n            &Meanin");
    ph.demand("6", prior.verify().is_ok(), format!( "previous image remains addressable"));

    });
}

fn exact_add_reference_compiles_encodes_and_loads_back_capability_keyed(ph: &mut Probe) {
    ph.case("exact_add_reference_compiles_encodes_and_loads_back_capability_keyed", |ph| {
    let image = build_with(&reference_capsule());
    image.verify().unwrap();

    let page = image
        .load_partition("language.reference")
        .expect("authored reference bodies encode a language.reference partition");
    ph.demand("1", 
        page.contains("std.capability.math.add"), format!(
        "the capability key must appear in the page"
    ));
    ph.demand("2", 
        page.contains("apply(add,var(lhs),var(rhs))"), format!(
        "the canonical term must appear in the page"
    ));

    let programs = LanguageImage::decode_reference_partition(page).unwrap();
    let cell = programs
        .get(&id("std.capability.math.add"))
        .expect("the loaded reference table is capability-keyed");
    eq_ref(ph, "3", &(cell.capability), &( "std.capability.math.add"));
    eq_ref(ph, "4", &(
        cell.params), &(
        vec![
            ("lhs".to_string(), ParamShape::Scalar),
            ("rhs".to_string(), ParamShape::Scalar),
        ]
    ));
    eq_ref(ph, "5", &(cell.program.input_count), &( 2));
    eq_ref(ph, "6", &(cell.program.state_count), &( 0));
    ph.demand("7", 
        matches!(
            cell.program.ops.last().map(|(op, _)| op),
            Some(emath_exec_ir::EmirOp::F64Add(_, _))
        ), format!(
        "exact-add lowers to the generic strict-add operation"
    ));

    let rebuilt = build_with(&reference_capsule());
    eq_ref(ph, format!(
        "identical capsule data rebuilds byte-identical reference bytecode"
    ), &(
        rebuilt.load_partition("language.reference")), &(
        image.load_partition("language.reference")));

    });
}

fn reference_body_refusals_are_typed(ph: &mut Probe) {
    ph.case("reference_body_refusals_are_typed", |ph| {
    // Partial slot sets refuse (all three or none).
    for missing in ["reference_params", "reference_signature", "reference_body"] {
        let mut partial = reference_capsule();
        partial.slots.remove(missing);
        ph.demand("1", 
            matches!(
                try_build_with(&partial),
                Err(LanguageImageError::InvalidReferenceBody { .. })
            ), format!(
            "dropping `{missing}` must refuse typed"
        ));
    }

    // Presence requires the authored reference mode.
    let mut generated = reference_capsule();
    generated.slots.insert(
        "reference".to_string(),
        CapsuleSlot::Value("generated".to_string()),
    );
    ph.demand("2", matches!(
        try_build_with(&generated),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ), "assertion failed: matches!(\n        try_build_with(&generated),\n        Err(LanguageImageError::In");

    // Malformed term text refuses.
    let mut malformed = reference_capsule();
    malformed.slots.insert(
        "reference_body".to_string(),
        CapsuleSlot::Value("apply(add,var(lhs".to_string()),
    );
    ph.demand("3", matches!(
        try_build_with(&malformed),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ), "assertion failed: matches!(\n        try_build_with(&malformed),\n        Err(LanguageImageError::In");

    // Operators outside the closed generic vocabulary refuse.
    let mut foreign = reference_capsule();
    foreign.slots.insert(
        "reference_signature".to_string(),
        CapsuleSlot::Value("frob=2".to_string()),
    );
    foreign.slots.insert(
        "reference_body".to_string(),
        CapsuleSlot::Value("apply(frob,var(lhs),var(rhs))".to_string()),
    );
    ph.demand("4", matches!(
        try_build_with(&foreign),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ), "assertion failed: matches!(\n        try_build_with(&foreign),\n        Err(LanguageImageError::Inva");

    // Free variables outside the declared params refuse.
    let mut ghost = reference_capsule();
    ghost.slots.insert(
        "reference_body".to_string(),
        CapsuleSlot::Value("apply(add,var(lhs),var(ghost))".to_string()),
    );
    ph.demand("5", matches!(
        try_build_with(&ghost),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ), "assertion failed: matches!(\n        try_build_with(&ghost),\n        Err(LanguageImageError::Invali");

    // Signature conflicts refuse.
    let mut conflict = reference_capsule();
    conflict.slots.insert(
        "reference_signature".to_string(),
        CapsuleSlot::Value("add=2,add=3".to_string()),
    );
    ph.demand("6", matches!(
        try_build_with(&conflict),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ), "assertion failed: matches!(\n        try_build_with(&conflict),\n        Err(LanguageImageError::Inv");

    // Declared inputs must line up with the reference params.
    let mut arity = reference_capsule();
    arity.slots.insert(
        "semantics".to_string(),
        CapsuleSlot::Value("kernel=checked-add;arity=1;inputs=Int;output=Int".to_string()),
    );
    ph.demand("7", matches!(
        try_build_with(&arity),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ), "assertion failed: matches!(\n        try_build_with(&arity),\n        Err(LanguageImageError::Invali");

    });
}

fn tampered_or_stale_reference_bytecode_refuses_typed(ph: &mut Probe) {
    ph.case("tampered_or_stale_reference_bytecode_refuses_typed", |ph| {
    let image = build_with(&reference_capsule());
    let page = image.load_partition("language.reference").unwrap();

    // Byte-level tampering is caught by the partition content id.
    let mut corrupt = image.clone();
    for partition in &mut corrupt.image.partitions {
        if partition.name == "language.reference" {
            partition.body.push_str("tampered\n");
        }
    }
    ph.demand("1", matches!(
        corrupt.verify(),
        Err(LanguageImageError::CorruptImage(_))
    ), "assertion failed: matches!(\n        corrupt.verify(),\n        Err(LanguageImageError::CorruptImage");

    // Tampered bytecode under a recomputed content id is caught by the
    // recompile-and-compare load validation.
    let doctored = page.replace("%1: ", "%9: ");
    ne_ref(ph, format!( "the doctored page must differ"), &(doctored), &( page));
    let mut stale = image.clone();
    for partition in &mut stale.image.partitions {
        if partition.name == "language.reference" {
            *partition = emath_exec_ir::image::ImagePartition::stamp(
                "language.reference",
                emath_exec_ir::image::PartitionKind::Bytecode,
                &doctored,
            );
        }
    }
    stale.verify().unwrap();
    ph.demand("3", matches!(
        LanguageImage::decode_reference_partition(
            stale.load_partition("language.reference").unwrap()
        ),
        Err(LanguageImageError::ReferenceBytecodeMismatch { .. })
    ), "assertion failed: matches!(\n        LanguageImage::decode_reference_partition(\n            stale.l");

    // A structurally broken page refuses typed instead of loading partial
    // authority.
    let mut broken = image.clone();
    for partition in &mut broken.image.partitions {
        if partition.name == "language.reference" {
            *partition = emath_exec_ir::image::ImagePartition::stamp(
                "language.reference",
                emath_exec_ir::image::PartitionKind::Bytecode,
                "reference std.capability.math.add\n",
            );
        }
    }
    broken.verify().unwrap();
    ph.demand("4", matches!(
        LanguageImage::decode_reference_partition(
            broken.load_partition("language.reference").unwrap()
        ),
        Err(LanguageImageError::ReferencePartitionMalformed(_))
    ), "assertion failed: matches!(\n        LanguageImage::decode_reference_partition(\n            broken.");

    });
}

fn mutated_reference_program_map_refuses_verification(ph: &mut Probe) {
    ph.case("mutated_reference_program_map_refuses_verification", |ph| {
    // Production construction over the real language directory: the
    // authored exact-add capsule must yield a loaded reference program,
    // and the baseline distribution must verify honestly.
    let distribution =
        compile_language_directory(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language"))
            .expect("the real language directory compiles with its authored reference bodies");
    let add = id("std.capability.math.add");
    ph.demand("1", 
        distribution.reference_programs.contains_key(&add), format!(
        "the real exact-add reference program is capability-keyed in the loaded table"
    ));
    distribution.verify().unwrap();

    // A program changed after compile refuses: the map no longer matches
    // the decoded `language.reference` partition.
    let mut changed = distribution.clone();
    changed
        .reference_programs
        .get_mut(&add)
        .unwrap()
        .program
        .result = emath_exec_ir::EmirValue(99);
    eq_ref(ph, "2", &(
        changed.verify()), &(
        Err(LanguageImageError::ReferenceBytecodeMismatch {
            feature: add.clone(),
        })
    ));

    // An extra installed capability refuses: the page never declared it.
    let mut extra = distribution.clone();
    let mut smuggled = extra.reference_programs.get(&add).unwrap().clone();
    smuggled.capability = "std.capability.math.sub".to_string();
    extra
        .reference_programs
        .insert(id("std.capability.math.sub"), smuggled);
    eq_ref(ph, "3", &(
        extra.verify()), &(
        Err(LanguageImageError::ReferenceBytecodeMismatch {
            feature: id("std.capability.math.sub"),
        })
    ));

    // Dropping an installed capability refuses just as loudly.
    let mut missing = distribution.clone();
    missing.reference_programs.remove(&add);
    eq_ref(ph, "4", &(
        missing.verify()), &(
        Err(LanguageImageError::ReferenceBytecodeMismatch { feature: add })
    ));

    });
}

#[test]
fn language_image_contracts() {
    let mut ph = Probe::new("language image builds and loads byte-deterministically; authored reference bodies encode capability-keyed; tampering or stale locks refuse typed");
    builder_and_loader_are_byte_deterministic_and_traceable(&mut ph);
    mutations_and_stale_lock_refuse_without_overwriting_prior_image(&mut ph);
    exact_add_reference_compiles_encodes_and_loads_back_capability_keyed(&mut ph);
    reference_body_refusals_are_typed(&mut ph);
    tampered_or_stale_reference_bytecode_refuses_typed(&mut ph);
    mutated_reference_program_map_refuses_verification(&mut ph);
    ph.finish();
}
