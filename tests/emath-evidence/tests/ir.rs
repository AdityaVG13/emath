//! Evidence-record resolution tests.

use emath_evidence::{
    CheckerRole, EvidenceKind, EvidenceRecord, Falsifier, FalsifierKind, Freshness, Independence,
    ProducerRole, can_become_resolved,
};
use emath_ir::{ClaimVerdict, EvidenceClaim, EvidenceLevel};
use emath_test_harness::Probe;

fn claim() -> EvidenceClaim {
    EvidenceClaim { id: "c1".into(), statement: "the interval encloses the true value".into(), class: "correctness".into(), scope: "exp-01".into(), assumptions: vec!["f64 arithmetic".into()], producer: "rumoca-interval".into(), checker: Some("indep-check".into()), verdict: ClaimVerdict::Pass, level: EvidenceLevel::E2, falsifiers: vec![], artifacts: vec!["cert.bin".into()], fresh_until: Some("2099-01-01T00:00:00Z".into()) }
}

fn record(incomplete: bool, verdict: ClaimVerdict) -> EvidenceRecord {
    EvidenceRecord { claim: claim(), kind: EvidenceKind::Interval, producer: ProducerRole { id: "rumoca-interval".into(), kind: EvidenceKind::Interval, version: "1.0.0".into() }, checker: Some(CheckerRole { id: "indep-check".into(), kind: EvidenceKind::Interval, version: "1.0.0".into(), independence: Independence::Independent }), freshness: Freshness { issued: "2026-01-01T00:00:00Z".into(), valid_until: "2099-01-01T00:00:00Z".into(), renews_with: vec!["compiler-v1".into()] }, falsifiers: vec![Falsifier { id: "f1".into(), kind: FalsifierKind::Counterexample, detail: "an input whose value lies outside the interval".into() }], verdict, incomplete }
}

#[test]
fn evidence_record_resolution() {
    let mut p = Probe::new("only complete passing computations become resolved evidence");
    p.case("complete-resolves", |p| {
        let complete = record(false, ClaimVerdict::Pass);
        p.demand("resolvable", can_become_resolved(&complete), "complete pass must resolve");
        p.demand("no-refusal", complete.refusal().is_none(), "complete pass has no refusal");
        let token = complete.canonical();
        p.eq("stable", complete.canonical(), token.clone());
        p.contains("shape", &token, "record:c1:interval:");
        let mut moved = complete.clone();
        moved.freshness.valid_until = "2098-01-01T00:00:00Z".into();
        p.ne("freshness-binds", complete.canonical(), moved.canonical());
    });
    p.case("incomplete-refused", |p| {
        let incomplete = record(true, ClaimVerdict::Pass);
        p.demand("not-resolvable", !can_become_resolved(&incomplete), "incomplete must not resolve");
        p.eq("code", incomplete.refusal().map(|r| r.code), Some("E-EVID-404"));
    });
    p.case("failed-refused", |p| {
        let failed = record(false, ClaimVerdict::Fail);
        p.demand("not-resolved", !failed.resolved(), "failing record is not resolved");
        p.eq("code", failed.refusal().map(|r| r.code), Some("E-EVID-404"));
    });
    p.finish();
}
