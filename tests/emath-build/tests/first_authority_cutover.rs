use std::str::FromStr;
use emath_artifact::{AuthorityEntry, AuthorityLock, AuthorityState};
use emath_build::{CutoverError, FIRST_CUTOVER_CONFORMANCE_CASES, FIRST_CUTOVER_IDS, PublicationEvidence, activate_first_cutover, rollback_feature};
use emath_core::{FeatureId, SemanticHash};
use emath_test_harness::Probe;

fn hash(seed: usize) -> SemanticHash {
    SemanticHash::from_str(&format!("sha256:{:064x}", seed + 1)).unwrap()
}
fn evidence() -> PublicationEvidence {
    PublicationEvidence {
        schemas_valid: true, reference_vectors_valid: true, deterministic_image: true, unrealized_coverage_explicit: true,
        projections_complete: true, live_adapter: true, unique_authority: true, no_blocking_holes: true,
        migrations_valid: true, independent_conformance: true, generated_views_fresh: true, authorized_semantic_change: true,
        conformance: vec!["first-cutover".into()], generated_views: vec!["language.generated".into()], rollback: "prior-image".into(),
    }
}
fn fixture() -> (AuthorityLock, Vec<(FeatureId, SemanticHash)>) {
    let mut lock = AuthorityLock::default();
    let mut hashes = vec![];
    for (i, raw) in FIRST_CUTOVER_IDS.iter().enumerate() {
        let id = FeatureId::from_str(raw).unwrap();
        lock.entries.insert(id.clone(), AuthorityEntry { state: AuthorityState::LegacyActive, active_source: "legacy".into(), semantic_hash: hash(100 + i) });
        hashes.push((id, hash(i)));
    }
    (lock, hashes)
}

#[test]
fn probe() {
    let mut p = Probe::new("first cutover activates eighteen features atomically and rolls back independently");
    p.eq("eighteen", FIRST_CUTOVER_IDS.len(), 18);
    p.case("activate-rollback", |p| {
        let (mut lock, hashes) = fixture();
        if activate_first_cutover(&mut lock, &hashes, &evidence()).is_err() {
            p.fail("activate", "must activate");
            return;
        }
        p.demand("capsule", lock.entries.values().all(|e| e.state == AuthorityState::CapsuleActive && e.active_source == "capsule"), "all must be capsule-active");
        let target = FeatureId::from_str("std.capability.math.add").unwrap();
        let unaffected = FeatureId::from_str("std.type.int").unwrap();
        let keep = lock.entries[&unaffected].semantic_hash.clone();
        rollback_feature(&mut lock, &target, hash(999), &evidence()).unwrap();
        p.eq("target", lock.entries[&target].state, AuthorityState::LegacyActive);
        p.eq("other-state", lock.entries[&unaffected].state, AuthorityState::CapsuleActive);
        p.eq("other-hash", lock.entries[&unaffected].semantic_hash.clone(), keep);
    });
    p.case("atomic", |p| {
        let (mut lock, mut hashes) = fixture();
        hashes.push((FeatureId::from_str("std.binder.sum").unwrap(), hash(2000)));
        p.eq("wrong-set", activate_first_cutover(&mut lock, &hashes, &evidence()), Err(CutoverError::WrongFeatureSet));
        hashes.pop();
        let before = lock.clone();
        let mut bad = evidence();
        bad.live_adapter = false;
        p.demand("gate-fails", activate_first_cutover(&mut lock, &hashes, &bad).is_err(), "bad gate must fail");
        p.eq("unchanged", lock, before);
    });
    p.eq("cases", FIRST_CUTOVER_CONFORMANCE_CASES, ["AddExact", "FloatIntoInt", "IntOverflow", "AddExactMutationControl"]);
    p.finish();
}
