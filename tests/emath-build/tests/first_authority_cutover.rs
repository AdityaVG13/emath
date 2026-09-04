use std::str::FromStr;

use emath_artifact::{AuthorityEntry, AuthorityLock, AuthorityState};
use emath_build::{
    CutoverError, FIRST_CUTOVER_IDS, PublicationEvidence, activate_first_cutover, rollback_feature,
};
use emath_core::{FeatureId, SemanticHash};

fn hash(seed: usize) -> SemanticHash {
    SemanticHash::from_str(&format!("sha256:{:064x}", seed + 1)).unwrap()
}

fn evidence() -> PublicationEvidence {
    PublicationEvidence {
        schemas_valid: true,
        reference_vectors_valid: true,
        deterministic_image: true,
        unrealized_coverage_explicit: true,
        projections_complete: true,
        live_adapter: true,
        unique_authority: true,
        no_blocking_holes: true,
        migrations_valid: true,
        independent_conformance: true,
        generated_views_fresh: true,
        authorized_semantic_change: true,
        conformance: vec!["first-cutover".to_string()],
        generated_views: vec!["language.generated".to_string()],
        rollback: "prior-image".to_string(),
    }
}

fn fixture() -> (AuthorityLock, Vec<(FeatureId, SemanticHash)>) {
    let mut lock = AuthorityLock::default();
    let mut hashes = Vec::new();
    for (index, raw) in FIRST_CUTOVER_IDS.iter().enumerate() {
        let id = FeatureId::from_str(raw).unwrap();
        lock.entries.insert(
            id.clone(),
            AuthorityEntry {
                state: AuthorityState::LegacyActive,
                active_source: "legacy".to_string(),
                semantic_hash: hash(100 + index),
            },
        );
        hashes.push((id, hash(index)));
    }
    (lock, hashes)
}

#[test]
fn exactly_eighteen_features_activate_and_one_rolls_back_independently() {
    assert_eq!(FIRST_CUTOVER_IDS.len(), 18);
    let (mut lock, hashes) = fixture();
    activate_first_cutover(&mut lock, &hashes, &evidence()).unwrap();
    assert!(
        lock.entries
            .values()
            .all(|entry| entry.state == AuthorityState::CapsuleActive
                && entry.active_source == "capsule")
    );
    let target = FeatureId::from_str("std.capability.math.add").unwrap();
    let unaffected = FeatureId::from_str("std.type.int").unwrap();
    let unaffected_hash = lock.entries[&unaffected].semantic_hash.clone();
    rollback_feature(&mut lock, &target, hash(999), &evidence()).unwrap();
    assert_eq!(lock.entries[&target].state, AuthorityState::LegacyActive);
    assert_eq!(
        lock.entries[&unaffected].state,
        AuthorityState::CapsuleActive
    );
    assert_eq!(lock.entries[&unaffected].semantic_hash, unaffected_hash);
}

#[test]
fn extra_missing_or_failed_gate_is_atomic() {
    let (mut lock, mut hashes) = fixture();
    hashes.push((FeatureId::from_str("std.binder.sum").unwrap(), hash(2000)));
    assert_eq!(
        activate_first_cutover(&mut lock, &hashes, &evidence()),
        Err(CutoverError::WrongFeatureSet)
    );
    hashes.pop();
    let before = lock.clone();
    let mut bad = evidence();
    bad.live_adapter = false;
    assert!(activate_first_cutover(&mut lock, &hashes, &bad).is_err());
    assert_eq!(lock, before, "failed atomic packet changes no authority");
}
