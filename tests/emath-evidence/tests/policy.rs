//! Evidence-level policy tests.

use emath_evidence::{EvidenceEntry, EvidenceKind, EvidencePolicy, Independence, requirement_for};
use emath_ir::EvidenceLevel;
use emath_test_harness::{Case, Probe, check_all, expect_ok};

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
fn evidence_level_bars() {
    let mut p = Probe::new("evidence bars get stronger with level");
    let policy = EvidencePolicy::default();
    p.case("e0-admits-all-without-checker", |p| {
        expect_ok(check_all(
            &PRODUCERS.map(|k| Case::new("e0", k, true)),
            |k| policy.satisfied_by(EvidenceLevel::E0, "correctness", *k, Independence::None),
        ));
        expect_ok(check_all(
            &PRODUCERS.map(|k| Case::new("e0-checked", k, false)),
            |k| policy.satisfied_by(EvidenceLevel::E0, "correctness", *k, Independence::Independent),
        ));
        let unknown = policy.satisfied_by(EvidenceLevel::E0, "unknown-class", EvidenceKind::Measurement, Independence::None);
        p.demand("unknown-class-refused", !unknown, "unknown class must refuse");
    });
    p.case("e1-e5-measurement", |p| {
        let measurement = EvidenceEntry { producer: EvidenceKind::Measurement, checker: Independence::None };
        p.demand("e1-admits", policy.admissible(EvidenceLevel::E1, "correctness").contains(&measurement), "E1 admits measurement");
        p.demand("e5-refuses", !policy.satisfied_by(EvidenceLevel::E5, "correctness", EvidenceKind::Measurement, Independence::None), "E5 refuses lone measurement");
    });
    p.case("differential-independence", |p| {
        p.demand("e3-independent", policy.satisfied_by(EvidenceLevel::E3, "correctness", EvidenceKind::Differential, Independence::Independent), "E3 differential+independent");
        p.demand("e3-coop-refused", !policy.satisfied_by(EvidenceLevel::E3, "correctness", EvidenceKind::Differential, Independence::Cooperating), "E3 refuses cooperating");
        p.demand("e5-refused", !policy.satisfied_by(EvidenceLevel::E5, "correctness", EvidenceKind::Differential, Independence::Independent), "E5 refuses differential");
    });
    p.case("e5-safety-proof", |p| {
        let e5 = requirement_for(&policy, EvidenceLevel::E5, "safety");
        p.eq("entry", e5.clone(), vec![EvidenceEntry { producer: EvidenceKind::FormalProof, checker: Independence::Independent }]);
        p.demand("satisfied", policy.satisfied_by(EvidenceLevel::E5, "safety", EvidenceKind::FormalProof, Independence::Independent), "E5 safety proof");
        p.demand("e4-lacks", !policy.admissible(EvidenceLevel::E4, "correctness").contains(&e5[0]), "E4 correctness lacks it");
    });
    p.finish();
}
