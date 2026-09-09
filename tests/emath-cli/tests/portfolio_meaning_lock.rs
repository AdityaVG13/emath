//! meaning_lock tests migrated from the in-crate `#[cfg(test)]` module.
use emath_cli::portfolio::interpretation::{DisqualificationReason, InterpretationPolicy, MetricAxis, MetricPolarity, evaluate};
use emath_cli::portfolio::meaning_lock::*;
use emath_cli::portfolio::record::GuardFailure;
use emath_cli::portfolio::{Authority, WorldCandidate};
use emath_test_harness::Probe;
use std::collections::BTreeMap as Map;
fn sample() -> (LockKey, LockEntry) { (LockKey { declaration_id: 0x1111_1111_1111_1111, hole_id: WHOLE_TERM_HOLE.to_string() }, LockEntry { source: "glyphs.emath".into(), source_hash: 0x2222_2222_2222_2222, world_fingerprint: 0x3333_3333_3333_3333, portfolio_receipt_id: 0x4444_4444_4444_4444, selection_method: SelectionMethod::CliSet, selected_at: 1_700_000_000 }) }
fn world(fp: u64, authority: Authority) -> WorldCandidate { WorldCandidate::new(fp, "p", authority, Map::from([("cost".into(), 1)]), fp) }
fn axes() -> Vec<MetricAxis> { vec![MetricAxis::new("cost", MetricPolarity::Minimize)] }
#[test]
fn probe() {
    let mut p = Probe::new("meaning lock round-trips, refuses tamper and drift, commits user-locked worlds");
    p.case("codec", |p| { let mut lock = MeaningLock::with_cap(5); let (k, e) = sample(); lock.upsert(k, e); p.eq("bytes", MeaningLock::parse(&lock.encode()).expect("parse").encode().as_bytes().to_vec(), lock.encode().as_bytes().to_vec()); let (mut l, mut r) = (MeaningLock::empty(), MeaningLock::empty()); let (k, mut e) = sample(); l.upsert(k.clone(), e.clone()); e.selected_at = 9; r.upsert(k, e); p.eq("no-timestamp", l.lock_id, r.lock_id); p.ne("bytes-differ", l.encode(), r.encode()); });
    p.case("parse", |p| { match MeaningLock::parse("{\n  \"schema\": \"emath.meaning-lock\",\n  \"schema_version\": 99,\n  \"portfolio_cap\": 5,\n  \"lock_id\": \"0000000000000000\",\n  \"entries\": []\n}\n") { Err(LockError::UnknownVersion { version: 99 }) => { p.demand("version", true, "ok"); }, other => { p.fail("version", format!("expected UnknownVersion, got {other:?}")); } }; match MeaningLock::parse("{not-json") { Err(LockError::Malformed { .. }) => { p.demand("malformed", true, "ok"); }, other => { p.fail("malformed", format!("expected Malformed, got {other:?}")); } }; });
    p.case("tamper", |p| { let mut lock = MeaningLock::empty(); let (k, e) = sample(); lock.upsert(k, e); match MeaningLock::parse(&lock.encode().replace("3333333333333333", "aaaaaaaaaaaaaaaa")) { Err(LockError::Tampered { .. }) => { p.demand("tampered", true, "ok"); }, other => { p.fail("tampered", format!("expected Tampered, got {other:?}")); } }; });
    p.case("resolve", |p| { let mut lock = MeaningLock::empty(); let (k, e) = sample(); lock.upsert(k.clone(), e.clone()); p.eq("hit", lock.resolve(k.declaration_id, WHOLE_TERM_HOLE, "glyphs.emath").expect("resolve").expect("entry").world_fingerprint, e.world_fingerprint); match lock.resolve(0x9999, WHOLE_TERM_HOLE, "glyphs.emath") { Err(LockError::Drifted { fingerprint, .. }) => { p.eq("drift-fp", fingerprint, e.world_fingerprint); }, other => { p.fail("drift", format!("expected Drifted, got {other:?}")); } }; p.eq("other-source", lock.resolve(0x9999, WHOLE_TERM_HOLE, "other.emath").expect("unlocked").is_none(), true); });
    p.case("commit", |p| { let r = commit_locked_world(world(7, Authority::Structural), axes(), 0x10, 0x20, &SelectionMethod::CliSet).expect("commit"); p.eq("single", r.selected.clone(), vec![7]); match &r.input.policy { InterpretationPolicy::UserLocked { lock_id: 0x10, origin_receipt_id: 0x20, method } => { p.eq("method", method.as_str(), "cli-set"); }, other => { p.fail("policy", format!("expected UserLocked, got {other:?}")); } }; p.contains("receipt", &r.encode(), "user-locked"); });
    p.case("disqualified", |p| {
        let mut bad = world(9, Authority::Structural);
        bad.guard_failure = Some(GuardFailure { code: "hard-constraint:violated".into(), detail: "carrier empty".into() });
        let good = world(8, Authority::Structural);
        let receipt = evaluate(vec![good, bad], axes(), InterpretationPolicy::Portfolio).expect("p");
        match refuse_disqualified(9, &receipt) {
            Err(LockError::Disqualified { fingerprint: 9, ledger }) => {
                p.eq("fp", ledger.fingerprint, 9);
                match ledger.reason {
                    DisqualificationReason::FailedGuard { code, .. } => { p.eq("code", code, "hard-constraint:violated".to_string()); }
                    other => { p.fail("reason", format!("expected FailedGuard, got {other:?}")); }
                };
            }
            other => { p.fail("disq", format!("expected Disqualified, got {other:?}")); }
        };
        let mut drifted = world(3, Authority::Structural);
        drifted.guard_failure = Some(GuardFailure { code: "missing-metric".into(), detail: "cost".into() });
        match commit_locked_world(drifted, axes(), 1, 2, &SelectionMethod::CliSet) {
            Err(LockError::Drifted { fingerprint: 3, .. }) => { p.demand("drifted", true, "ok"); }
            other => { p.fail("drifted", format!("expected Drifted, got {other:?}")); }
        };
    });
    p.finish();
}
