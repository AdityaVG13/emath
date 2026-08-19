//! Evidence-record resolution tests.
//!
//! Moved from #[cfg(test)] in crates/emath-evidence/src/ir.rs.

use emath_evidence::{
    CheckerRole, EvidenceKind, EvidenceRecord, Falsifier, FalsifierKind, Freshness, Independence,
    ProducerRole, can_become_resolved,
};
use emath_ir::{ClaimVerdict, EvidenceClaim, EvidenceLevel};

fn claim() -> EvidenceClaim {
    EvidenceClaim {
        id: "c1".into(),
        statement: "the interval encloses the true value".into(),
        class: "correctness".into(),
        scope: "exp-01".into(),
        assumptions: vec!["f64 arithmetic".into()],
        producer: "rumoca-interval".into(),
        checker: Some("indep-check".into()),
        verdict: ClaimVerdict::Pass,
        level: EvidenceLevel::E2,
        falsifiers: vec![],
        artifacts: vec!["cert.bin".into()],
        fresh_until: Some("2099-01-01T00:00:00Z".into()),
    }
}

fn record(incomplete: bool, verdict: ClaimVerdict) -> EvidenceRecord {
    EvidenceRecord {
        claim: claim(),
        kind: EvidenceKind::Interval,
        producer: ProducerRole {
            id: "rumoca-interval".into(),
            kind: EvidenceKind::Interval,
            version: "1.0.0".into(),
        },
        checker: Some(CheckerRole {
            id: "indep-check".into(),
            kind: EvidenceKind::Interval,
            version: "1.0.0".into(),
            independence: Independence::Independent,
        }),
        freshness: Freshness {
            issued: "2026-01-01T00:00:00Z".into(),
            valid_until: "2099-01-01T00:00:00Z".into(),
            renews_with: vec!["compiler-v1".into()],
        },
        falsifiers: vec![Falsifier {
            id: "f1".into(),
            kind: FalsifierKind::Counterexample,
            detail: "an input whose value lies outside the interval".into(),
        }],
        verdict,
        incomplete,
    }
}

#[test]
fn incomplete_computation_cannot_become_resolved_evidence() {
    let complete = record(false, ClaimVerdict::Pass);
    assert!(can_become_resolved(&complete));
    assert!(complete.refusal().is_none());
    let token = complete.canonical();
    assert_eq!(complete.canonical(), token);
    assert!(token.contains("record:c1:interval:"));

    let mut freshness_moved = complete.clone();
    freshness_moved.freshness.valid_until = "2098-01-01T00:00:00Z".into();
    assert_ne!(complete.canonical(), freshness_moved.canonical());

    let incomplete = record(true, ClaimVerdict::Pass);
    assert!(!can_become_resolved(&incomplete));
    assert_eq!(incomplete.refusal().unwrap().code, "E-EVID-404");

    let failed = record(false, ClaimVerdict::Fail);
    assert!(!failed.resolved());
    assert_eq!(failed.refusal().unwrap().code, "E-EVID-404");
}
