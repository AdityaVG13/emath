//! Promotion-policy route tests.

use emath_lab_core::{EnginePolicy, GateCheck, GateCheckKind, GateVerdict, PairedResult, PromotionOutcome, PromotionReason, QualityGate, Route, Selector, decide};
use emath_test_harness::Probe;

fn open_gate() -> GateVerdict {
    QualityGate::evaluate(vec![GateCheck::pass("correctness", GateCheckKind::Correctness)])
}

fn paired(median_ratio: f64) -> PairedResult {
    PairedResult { samples_used: 3, outliers_removed: 0, median_baseline_ns: 100.0, median_candidate_ns: 100.0 * median_ratio, median_ratio, p99_ratio: median_ratio, wins: 2, losses: 1, ties: 0, raw_retained: true, paired: true, seed: 1 }
}

#[test]
fn promotion_policy() {
    let mut p = Probe::new("promoted candidates stay, regressed demote, middling canary");
    let policy = EnginePolicy::default();
    p.case("retained-promotion", |p| {
        let decision = decide(&policy, &open_gate(), Some(&paired(0.97)), None, None, true);
        p.eq("outcome", decision.outcome, PromotionOutcome::Promote);
        p.eq("reason", decision.reason, PromotionReason::RetainedPromotion { median_ratio: 0.97 });
        let mut selector = Selector::new(open_gate(), decision.outcome, 16).expect("valid selector");
        p.eq("dispatch-early", selector.dispatch(1), Route::Candidate);
        p.eq("dispatch-late", selector.dispatch(9), Route::Candidate);
    });
    p.case("regression-demotes", |p| {
        let decision = decide(&policy, &open_gate(), Some(&paired(1.06)), None, None, true);
        p.eq("outcome", decision.outcome, PromotionOutcome::Demote);
        p.demand("reason", matches!(decision.reason, PromotionReason::MedianRegression { .. }), "regression demotes with cause");
    });
    p.case("middle-canaries", |p| {
        let decision = decide(&policy, &open_gate(), Some(&paired(0.97)), None, None, false);
        p.eq("outcome", decision.outcome, PromotionOutcome::Canary);
    });
    p.finish();
}
