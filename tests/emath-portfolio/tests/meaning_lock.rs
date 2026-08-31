//! meaning_lock tests migrated from the in-crate `#[cfg(test)]` module.

use emath_portfolio::meaning_lock::*;
use emath_portfolio::interpretation::{
    evaluate, DisqualificationReason, InterpretationPolicy, MetricAxis, MetricPolarity,
};
use emath_portfolio::record::GuardFailure;
use emath_portfolio::{Authority, WorldCandidate};
use std::collections::BTreeMap as Map;

fn sample_entry() -> (LockKey, LockEntry) {
    (
        LockKey {
            declaration_id: 0x1111_1111_1111_1111,
            hole_id: WHOLE_TERM_HOLE.to_string(),
        },
        LockEntry {
            source: "glyphs.emath".to_string(),
            source_hash: 0x2222_2222_2222_2222,
            world_fingerprint: 0x3333_3333_3333_3333,
            portfolio_receipt_id: 0x4444_4444_4444_4444,
            selection_method: SelectionMethod::CliSet,
            selected_at: 1_700_000_000,
        },
    )
}

fn world(fp: u64, authority: Authority) -> WorldCandidate {
    let mut metrics = Map::new();
    metrics.insert("cost".to_string(), 1);
    WorldCandidate::new(fp, "p", authority, metrics, fp)
}

fn axes() -> Vec<MetricAxis> {
    vec![MetricAxis::new("cost", MetricPolarity::Minimize)]
}

#[test]
fn round_trip_is_byte_deterministic() {
    let mut lock = MeaningLock::with_cap(5);
    let (key, entry) = sample_entry();
    lock.upsert(key, entry);
    let first = lock.encode();
    let parsed = MeaningLock::parse(&first).expect("parse");
    let second = parsed.encode();
    assert_eq!(first.as_bytes(), second.as_bytes());
    assert_eq!(parsed.lock_id, lock.lock_id);
    assert_eq!(first, lock.encode());
}

#[test]
fn timestamp_is_excluded_from_lock_id() {
    let mut left = MeaningLock::empty();
    let mut right = MeaningLock::empty();
    let (key, mut entry) = sample_entry();
    left.upsert(key.clone(), entry.clone());
    entry.selected_at = 9;
    right.upsert(key, entry);
    assert_eq!(left.lock_id, right.lock_id);
    assert_ne!(left.encode(), right.encode());
}

#[test]
fn unknown_schema_version_refuses() {
    let body = "{\n  \"schema\": \"emath.meaning-lock\",\n  \"schema_version\": 99,\n  \"portfolio_cap\": 5,\n  \"lock_id\": \"0000000000000000\",\n  \"entries\": []\n}\n";
    match MeaningLock::parse(body) {
        Err(LockError::UnknownVersion { version }) => assert_eq!(version, 99),
        other => panic!("expected UnknownVersion, got {other:?}"),
    }
}

#[test]
fn malformed_file_refuses() {
    match MeaningLock::parse("{not-json") {
        Err(LockError::Malformed { .. }) => {}
        other => panic!("expected Malformed, got {other:?}"),
    }
    match MeaningLock::parse(
        "{\n  \"schema\": \"emath.meaning-lock\",\n  \"schema_version\": 1,\n  \"portfolio_cap\": 5,\n  \"lock_id\": \"0000000000000000\",\n  \"entries\": [],\n  \"extra\": 1\n}\n",
    ) {
        Err(LockError::Malformed { detail }) => {
            assert!(detail.contains("unknown field"), "{detail}")
        }
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn tampered_fingerprint_refuses() {
    let mut lock = MeaningLock::empty();
    let (key, entry) = sample_entry();
    lock.upsert(key, entry);
    let mut body = lock.encode();
    body = body.replace("3333333333333333", "aaaaaaaaaaaaaaaa");
    match MeaningLock::parse(&body) {
        Err(LockError::Tampered { .. }) => {}
        other => panic!("expected Tampered, got {other:?}"),
    }
}

#[test]
fn fingerprint_match_and_source_drift() {
    let mut lock = MeaningLock::empty();
    let (key, entry) = sample_entry();
    lock.upsert(key.clone(), entry.clone());
    let hit = lock
        .resolve(key.declaration_id, WHOLE_TERM_HOLE, "glyphs.emath")
        .expect("resolve")
        .expect("entry");
    assert_eq!(hit.world_fingerprint, entry.world_fingerprint);
    match lock.resolve(0x9999, WHOLE_TERM_HOLE, "glyphs.emath") {
        Err(LockError::Drifted { fingerprint, .. }) => {
            assert_eq!(fingerprint, entry.world_fingerprint);
        }
        other => panic!("expected Drifted, got {other:?}"),
    }
    assert!(lock
        .resolve(0x9999, WHOLE_TERM_HOLE, "other.emath")
        .expect("other source is unlocked")
        .is_none());
}

#[test]
fn commit_locked_world_is_single_world_user_locked() {
    let locked = world(7, Authority::Structural);
    let receipt = commit_locked_world(locked, axes(), 0x10, 0x20, &SelectionMethod::CliSet)
        .expect("commit");
    assert_eq!(receipt.selected, vec![7]);
    assert!(receipt.archived.is_empty());
    match &receipt.input.policy {
        InterpretationPolicy::UserLocked {
            lock_id,
            origin_receipt_id,
            method,
        } => {
            assert_eq!(*lock_id, 0x10);
            assert_eq!(*origin_receipt_id, 0x20);
            assert_eq!(method, "cli-set");
        }
        other => panic!("expected UserLocked, got {other:?}"),
    }
    assert!(receipt.encode().contains("user-locked"));
    assert_eq!(
        receipt
            .input
            .candidates
            .iter()
            .map(|candidate| candidate.labeled_authority)
            .max()
            .expect("candidate"),
        Authority::Structural
    );
}

#[test]
fn lock_on_disqualified_world_is_refused_with_ledger() {
    let mut bad = world(9, Authority::Structural);
    bad.guard_failure = Some(GuardFailure {
        code: "hard-constraint:violated".to_string(),
        detail: "carrier empty".to_string(),
    });
    let good = world(8, Authority::Structural);
    let receipt =
        evaluate(vec![good, bad], axes(), InterpretationPolicy::Portfolio).expect("portfolio");
    match refuse_disqualified(9, &receipt) {
        Err(LockError::Disqualified {
            fingerprint,
            ledger,
        }) => {
            assert_eq!(fingerprint, 9);
            assert_eq!(ledger.fingerprint, 9);
            match ledger.reason {
                DisqualificationReason::FailedGuard { code, .. } => {
                    assert_eq!(code, "hard-constraint:violated");
                }
                other => panic!("expected FailedGuard, got {other:?}"),
            }
        }
        other => panic!("expected Disqualified, got {other:?}"),
    }
}

#[test]
fn drifted_locked_candidate_refuses() {
    let mut locked = world(3, Authority::Structural);
    locked.guard_failure = Some(GuardFailure {
        code: "missing-metric".to_string(),
        detail: "cost".to_string(),
    });
    match commit_locked_world(locked, axes(), 1, 2, &SelectionMethod::CliSet) {
        Err(LockError::Drifted { fingerprint, .. }) => assert_eq!(fingerprint, 3),
        other => panic!("expected Drifted, got {other:?}"),
    }
}
