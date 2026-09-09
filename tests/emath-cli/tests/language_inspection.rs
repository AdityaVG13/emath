//! Language inspection binds a fresh image and reports honest output.
use emath_artifact::{AuthorityEntry, AuthorityLock, AuthorityState};
use emath_cli::language_cmd::{LanguageCommand, LanguageInspectError, LanguageInspection};
use emath_core::{CanonicalField, FeatureId, SemanticHash};
use emath_exec_ir::language_image::{FeatureAuthorityEntry, LanguageImage, LanguageSourceMapEntry};
use emath_ir::{CapsuleSlot, FEATURE_CAPSULE_SCHEMA, FeatureCapsule, FeatureClass, Maturity, MeaningEdge, MeaningEdgeKind, MeaningResource, MeaningSpine};
use emath_test_harness::Probe;
use std::collections::BTreeMap;
use std::str::FromStr;
fn fixture() -> (LanguageImage, Vec<FeatureCapsule>, MeaningSpine, AuthorityLock) {
    let id = FeatureId::from_str("std.capability.math.add").unwrap();
    let sem = SemanticHash::from_str(&format!("sha256:{}", "4".repeat(64))).unwrap();
    let cap = FeatureCapsule { schema: FEATURE_CAPSULE_SCHEMA.to_string(), feature_id: id.clone(), semantic_hash: sem.clone(), class: FeatureClass::Capability, maturity: Maturity::Stable, summary: "add".to_string(), source: "language/spec/capabilities/core/add.emath".to_string(), edges: vec![], slots: BTreeMap::from([("agent".into(), CapsuleSlot::Value("owners=add.emath;hazards=exactness".into())), ("worlds".into(), CapsuleSlot::Value("exact-int".into())), ("providers".into(), CapsuleSlot::Value("reference".into()))]), projections: vec![] };
    let mut spine = MeaningSpine::default(); spine.register_feature(id.clone(), FeatureClass::Capability);
    let doc = MeaningResource::parse("doc://reference/add").unwrap(); spine.register_external(doc.clone()).unwrap();
    spine.insert(MeaningEdge { source: MeaningResource::Feature(id.clone()), kind: MeaningEdgeKind::Documents, target: doc }).unwrap();
    let image = LanguageImage::build(&[cap.clone()], &spine, &BTreeMap::new(), &[FeatureAuthorityEntry { feature_id: id.clone(), state: "capsule-active".into() }], &[LanguageSourceMapEntry { feature_id: id.clone(), authored_source: cap.source.clone() }], vec![], Some(&[CanonicalField::new("repository_commit", b"abc").unwrap()])).unwrap();
    let mut auth = AuthorityLock::default(); auth.entries.insert(id, AuthorityEntry { state: AuthorityState::CapsuleActive, active_source: "capsule".into(), semantic_hash: sem });
    (image, vec![cap], spine, auth)
}
#[test]
fn probe() {
    let mut p = Probe::new("six inspection commands bind fresh image, stale images never report authority");
    let (image, caps, spine, auth) = fixture(); let receipts = BTreeMap::new();
    let ins = LanguageInspection { image: &image, capsules: &caps, spine: &spine, authority: &auth, receipts: &receipts };
    let id = FeatureId::from_str("std.capability.math.add").unwrap();
    p.case("commands", |p| { for cmd in [LanguageCommand::Orient(id.clone()), LanguageCommand::Impact(id.clone()), LanguageCommand::Authority(id.clone()), LanguageCommand::Gaps(None), LanguageCommand::CheckImage] { p.demand("human", ins.run(cmd.clone(), false).unwrap().starts_with("image_id="), "human starts with image_id"); p.contains("schema", &ins.run(cmd, true).unwrap(), "emath.language-inspection"); } p.eq("receipt", ins.run(LanguageCommand::Receipt(id.clone()), false), Err(LanguageInspectError::IncompleteReceipt(id.clone()))); });
    p.case("stale", |p| { let (mut image, caps, spine, auth) = fixture(); image.image.partitions[0].body.push_str("tamper"); let ins = LanguageInspection { image: &image, capsules: &caps, spine: &spine, authority: &auth, receipts: &BTreeMap::new() }; p.eq("refuses", ins.run(LanguageCommand::CheckImage, false), Err(LanguageInspectError::StaleImage)); });
    p.finish();
}
