//! Evidence-level policy tests.
//!
//! Moved from #[cfg(test)] in crates/emath-evidence/src/policy.rs.

use emath_evidence::{EvidenceEntry, EvidenceKind, EvidencePolicy, Independence, requirement_for};
use emath_ir::EvidenceLevel;

const PRODUCERS: [EvidenceKind; 7] = [
    EvidenceKind::FormalProof,
    EvidenceKind::Residual,
    EvidenceKind::Interval,
    EvidenceKind::Witness,
    EvidenceKind::Differential,
    EvidenceKind::Measurement,
    EvidenceKind::Structural,
];

#[test]
fn bars_get_stronger_with_level() {
    let policy = EvidencePolicy::default();

    for producer in PRODUCERS {
        assert!(
            policy.satisfied_by(
                EvidenceLevel::E0,
                "correctness",
                producer,
                Independence::None
            ),
            "E0 must admit {producer:?} with no checker"
        );
        assert!(!policy.satisfied_by(
            EvidenceLevel::E0,
            "correctness",
            producer,
            Independence::Independent
        ));
    }
    assert!(!policy.satisfied_by(
        EvidenceLevel::E0,
        "unknown-class",
        EvidenceKind::Measurement,
        Independence::None
    ));

    let measurement = EvidenceEntry {
        producer: EvidenceKind::Measurement,
        checker: Independence::None,
    };
    assert!(policy
        .admissible(EvidenceLevel::E1, "correctness")
        .contains(&measurement));
    assert!(!policy.satisfied_by(
        EvidenceLevel::E5,
        "correctness",
        EvidenceKind::Measurement,
        Independence::None
    ));

    assert!(policy.satisfied_by(
        EvidenceLevel::E3,
        "correctness",
        EvidenceKind::Differential,
        Independence::Independent
    ));
    assert!(!policy.satisfied_by(
        EvidenceLevel::E3,
        "correctness",
        EvidenceKind::Differential,
        Independence::Cooperating
    ));
    assert!(!policy.satisfied_by(
        EvidenceLevel::E5,
        "correctness",
        EvidenceKind::Differential,
        Independence::Independent
    ));

    let e5 = requirement_for(&policy, EvidenceLevel::E5, "safety");
    assert_eq!(
        e5,
        vec![EvidenceEntry {
            producer: EvidenceKind::FormalProof,
            checker: Independence::Independent,
        }]
    );
    assert!(policy.satisfied_by(
        EvidenceLevel::E5,
        "safety",
        EvidenceKind::FormalProof,
        Independence::Independent
    ));
    assert!(!policy
        .admissible(EvidenceLevel::E4, "correctness")
        .contains(&e5[0]));
}
