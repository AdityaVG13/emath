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

#[test]
fn builder_and_loader_are_byte_deterministic_and_traceable() {
    let first = build("checked-add");
    let second = build("checked-add");
    assert_eq!(first, second);
    first.verify().unwrap();
    assert_ne!(
        first.semantic_hash.as_str(),
        first.distribution_hash.as_str()
    );
    assert_ne!(
        first.operational_hash.as_ref().unwrap().as_str(),
        first.semantic_hash.as_str()
    );
    assert_eq!(
        first.authored_source(&id("std.capability.math.add")),
        Some("language/spec/capabilities/core/add.emath")
    );
    assert!(
        first
            .load_partition("language.tables")
            .unwrap()
            .contains("add=checked")
    );
    assert!(
        first
            .load_partition("language.capsules")
            .unwrap()
            .contains("std.capability.math.add")
    );
}

#[test]
fn mutations_and_stale_lock_refuse_without_overwriting_prior_image() {
    let prior = build("checked-add");
    let changed = build("wrapping-add");
    assert_ne!(prior.semantic_hash, changed.semantic_hash);
    assert_ne!(prior.distribution_hash, changed.distribution_hash);

    let mut stale = prior.clone();
    stale.lock.distribution_hash = changed.distribution_hash.clone();
    assert_eq!(stale.verify(), Err(LanguageImageError::StaleLock));

    let mut corrupt = prior.clone();
    corrupt.image.partitions[0].body.push_str("tampered");
    assert!(matches!(
        corrupt.verify(),
        Err(LanguageImageError::CorruptImage(_))
    ));

    assert!(
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
    );
    assert!(prior.verify().is_ok(), "previous image remains addressable");
}

#[test]
fn exact_add_reference_compiles_encodes_and_loads_back_capability_keyed() {
    let image = build_with(&reference_capsule());
    image.verify().unwrap();

    let page = image
        .load_partition("language.reference")
        .expect("authored reference bodies encode a language.reference partition");
    assert!(
        page.contains("std.capability.math.add"),
        "the capability key must appear in the page"
    );
    assert!(
        page.contains("apply(add,var(lhs),var(rhs))"),
        "the canonical term must appear in the page"
    );

    let programs = LanguageImage::decode_reference_partition(page).unwrap();
    let cell = programs
        .get(&id("std.capability.math.add"))
        .expect("the loaded reference table is capability-keyed");
    assert_eq!(cell.capability, "std.capability.math.add");
    assert_eq!(
        cell.params,
        vec![
            ("lhs".to_string(), ParamShape::Scalar),
            ("rhs".to_string(), ParamShape::Scalar),
        ]
    );
    assert_eq!(cell.program.input_count, 2);
    assert_eq!(cell.program.state_count, 0);
    assert!(
        matches!(
            cell.program.ops.last().map(|(op, _)| op),
            Some(emath_exec_ir::EmirOp::F64Add(_, _))
        ),
        "exact-add lowers to the generic strict-add operation"
    );

    let rebuilt = build_with(&reference_capsule());
    assert_eq!(
        rebuilt.load_partition("language.reference"),
        image.load_partition("language.reference"),
        "identical capsule data rebuilds byte-identical reference bytecode"
    );
}

#[test]
fn reference_body_refusals_are_typed() {
    // Partial slot sets refuse (all three or none).
    for missing in ["reference_params", "reference_signature", "reference_body"] {
        let mut partial = reference_capsule();
        partial.slots.remove(missing);
        assert!(
            matches!(
                try_build_with(&partial),
                Err(LanguageImageError::InvalidReferenceBody { .. })
            ),
            "dropping `{missing}` must refuse typed"
        );
    }

    // Presence requires the authored reference mode.
    let mut generated = reference_capsule();
    generated.slots.insert(
        "reference".to_string(),
        CapsuleSlot::Value("generated".to_string()),
    );
    assert!(matches!(
        try_build_with(&generated),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ));

    // Malformed term text refuses.
    let mut malformed = reference_capsule();
    malformed.slots.insert(
        "reference_body".to_string(),
        CapsuleSlot::Value("apply(add,var(lhs".to_string()),
    );
    assert!(matches!(
        try_build_with(&malformed),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ));

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
    assert!(matches!(
        try_build_with(&foreign),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ));

    // Free variables outside the declared params refuse.
    let mut ghost = reference_capsule();
    ghost.slots.insert(
        "reference_body".to_string(),
        CapsuleSlot::Value("apply(add,var(lhs),var(ghost))".to_string()),
    );
    assert!(matches!(
        try_build_with(&ghost),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ));

    // Signature conflicts refuse.
    let mut conflict = reference_capsule();
    conflict.slots.insert(
        "reference_signature".to_string(),
        CapsuleSlot::Value("add=2,add=3".to_string()),
    );
    assert!(matches!(
        try_build_with(&conflict),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ));

    // Declared inputs must line up with the reference params.
    let mut arity = reference_capsule();
    arity.slots.insert(
        "semantics".to_string(),
        CapsuleSlot::Value("kernel=checked-add;arity=1;inputs=Int;output=Int".to_string()),
    );
    assert!(matches!(
        try_build_with(&arity),
        Err(LanguageImageError::InvalidReferenceBody { .. })
    ));
}

#[test]
fn tampered_or_stale_reference_bytecode_refuses_typed() {
    let image = build_with(&reference_capsule());
    let page = image.load_partition("language.reference").unwrap();

    // Byte-level tampering is caught by the partition content id.
    let mut corrupt = image.clone();
    for partition in &mut corrupt.image.partitions {
        if partition.name == "language.reference" {
            partition.body.push_str("tampered\n");
        }
    }
    assert!(matches!(
        corrupt.verify(),
        Err(LanguageImageError::CorruptImage(_))
    ));

    // Tampered bytecode under a recomputed content id is caught by the
    // recompile-and-compare load validation.
    let doctored = page.replace("%1: ", "%9: ");
    assert_ne!(doctored, page, "the doctored page must differ");
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
    assert!(matches!(
        LanguageImage::decode_reference_partition(
            stale.load_partition("language.reference").unwrap()
        ),
        Err(LanguageImageError::ReferenceBytecodeMismatch { .. })
    ));

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
    assert!(matches!(
        LanguageImage::decode_reference_partition(
            broken.load_partition("language.reference").unwrap()
        ),
        Err(LanguageImageError::ReferencePartitionMalformed(_))
    ));
}

#[test]
fn mutated_reference_program_map_refuses_verification() {
    // Production construction over the real language directory: the
    // authored exact-add capsule must yield a loaded reference program,
    // and the baseline distribution must verify honestly.
    let distribution =
        compile_language_directory(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language"))
            .expect("the real language directory compiles with its authored reference bodies");
    let add = id("std.capability.math.add");
    assert!(
        distribution.reference_programs.contains_key(&add),
        "the real exact-add reference program is capability-keyed in the loaded table"
    );
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
    assert_eq!(
        changed.verify(),
        Err(LanguageImageError::ReferenceBytecodeMismatch {
            feature: add.clone(),
        })
    );

    // An extra installed capability refuses: the page never declared it.
    let mut extra = distribution.clone();
    let mut smuggled = extra.reference_programs.get(&add).unwrap().clone();
    smuggled.capability = "std.capability.math.sub".to_string();
    extra
        .reference_programs
        .insert(id("std.capability.math.sub"), smuggled);
    assert_eq!(
        extra.verify(),
        Err(LanguageImageError::ReferenceBytecodeMismatch {
            feature: id("std.capability.math.sub"),
        })
    );

    // Dropping an installed capability refuses just as loudly.
    let mut missing = distribution.clone();
    missing.reference_programs.remove(&add);
    assert_eq!(
        missing.verify(),
        Err(LanguageImageError::ReferenceBytecodeMismatch { feature: add })
    );
}
