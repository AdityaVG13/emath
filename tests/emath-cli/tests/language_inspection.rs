use std::collections::BTreeMap;
use std::str::FromStr;

use emath_artifact::{AuthorityEntry, AuthorityLock, AuthorityState};
use emath_cli::language_cmd::{LanguageCommand, LanguageInspectError, LanguageInspection};
use emath_core::{CanonicalField, FeatureId, SemanticHash};
use emath_exec_ir::language_image::{FeatureAuthorityEntry, LanguageImage, LanguageSourceMapEntry};
use emath_ir::{
    CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity, MeaningEdge,
    MeaningEdgeKind, MeaningResource, MeaningSpine,
};

fn fixture() -> (
    LanguageImage,
    Vec<FeatureCapsule>,
    MeaningSpine,
    AuthorityLock,
) {
    let id = FeatureId::from_str("std.capability.math.add").unwrap();
    let semantic = SemanticHash::from_str(&format!("sha256:{}", "4".repeat(64))).unwrap();
    let capsule = FeatureCapsule {
        schema: FEATURE_CAPSULE_SCHEMA.to_string(),
        feature_id: id.clone(),
        semantic_hash: semantic.clone(),
        class: FeatureClass::Capability,
        maturity: Maturity::Stable,
        summary: "add".to_string(),
        source: "language/spec/capabilities/core/add.emath".to_string(),
        edges: vec![],
        slots: BTreeMap::from([
            (
                "agent".to_string(),
                CapsuleSlot::Value("owners=add.emath;hazards=exactness".to_string()),
            ),
            (
                "worlds".to_string(),
                CapsuleSlot::Value("exact-int".to_string()),
            ),
            (
                "providers".to_string(),
                CapsuleSlot::Value("reference".to_string()),
            ),
        ]),
        projections: vec![],
    };
    let mut spine = MeaningSpine::default();
    spine.register_feature(id.clone(), FeatureClass::Capability);
    let doc = MeaningResource::parse("doc://reference/add").unwrap();
    spine.register_external(doc.clone()).unwrap();
    spine
        .insert(MeaningEdge {
            source: MeaningResource::Feature(id.clone()),
            kind: MeaningEdgeKind::Documents,
            target: doc,
        })
        .unwrap();
    let image = LanguageImage::build(
        &[capsule.clone()],
        &spine,
        &BTreeMap::new(),
        &[FeatureAuthorityEntry {
            feature_id: id.clone(),
            state: "capsule-active".to_string(),
        }],
        &[LanguageSourceMapEntry {
            feature_id: id.clone(),
            authored_source: capsule.source.clone(),
        }],
        vec![],
        Some(&[CanonicalField::new("repository_commit", b"abc").unwrap()]),
    )
    .unwrap();
    let mut authority = AuthorityLock::default();
    authority.entries.insert(
        id,
        AuthorityEntry {
            state: AuthorityState::CapsuleActive,
            active_source: "capsule".to_string(),
            semantic_hash: semantic,
        },
    );
    (image, vec![capsule], spine, authority)
}

#[test]
fn six_inspection_commands_bind_fresh_image_and_honest_output() {
    let (image, capsules, spine, authority) = fixture();
    let receipts = BTreeMap::new();
    let inspect = LanguageInspection {
        image: &image,
        capsules: &capsules,
        spine: &spine,
        authority: &authority,
        receipts: &receipts,
    };
    let id = FeatureId::from_str("std.capability.math.add").unwrap();
    for command in [
        LanguageCommand::Orient(id.clone()),
        LanguageCommand::Impact(id.clone()),
        LanguageCommand::Authority(id.clone()),
        LanguageCommand::Gaps(None),
        LanguageCommand::CheckImage,
    ] {
        let human = inspect.run(command.clone(), false).unwrap();
        let json = inspect.run(command, true).unwrap();
        assert!(human.starts_with("image_id="));
        assert!(json.contains("emath.language-inspection"));
    }
    assert_eq!(
        inspect.run(LanguageCommand::Receipt(id), false),
        Err(LanguageInspectError::IncompleteReceipt(
            FeatureId::from_str("std.capability.math.add").unwrap()
        ))
    );
}

#[test]
fn stale_and_unknown_images_never_report_authority() {
    let (mut image, capsules, spine, authority) = fixture();
    image.image.partitions[0].body.push_str("tamper");
    let receipts = BTreeMap::new();
    let inspect = LanguageInspection {
        image: &image,
        capsules: &capsules,
        spine: &spine,
        authority: &authority,
        receipts: &receipts,
    };
    assert_eq!(
        inspect.run(LanguageCommand::CheckImage, false),
        Err(LanguageInspectError::StaleImage)
    );
}
