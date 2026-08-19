//! Promotion policy tests (origin `crates/emath-lab-core/src/promotion.rs`).

use emath_lab_core::{
    EnginePolicy, GateCheck, GateCheckKind, GateVerdict, PairedResult, PromotionOutcome,
    PromotionReason, QualityGate, Route, Selector, decide,
};

fn open_gate() -> GateVerdict {
    QualityGate::evaluate(vec![GateCheck::pass(
        "correctness",
        GateCheckKind::Correctness,
    )])
}

fn paired(median_ratio: f64) -> PairedResult {
    PairedResult {
        samples_used: 3,
        outliers_removed: 0,
        median_baseline_ns: 100.0,
        median_candidate_ns: 100.0 * median_ratio,
        median_ratio,
        p99_ratio: median_ratio,
        wins: 2,
        losses: 1,
        ties: 0,
        raw_retained: true,
        paired: true,
        seed: 1,
    }
}

#[test]
fn currently_promoted_non_regressed_stays_on_the_promoted_route() {
    // median_ratio 0.97: still 3% faster than the baseline, between
    // the canary (0.99) and promote (0.95) targets. The incumbent
    // must not be taken off-air and mislabeled TooSlow.
    let policy = EnginePolicy::default();
    let decision = decide(&policy, &open_gate(), Some(&paired(0.97)), None, None, true);
    assert_eq!(decision.outcome, PromotionOutcome::Promote);
    assert_eq!(
        decision.reason,
        PromotionReason::RetainedPromotion { median_ratio: 0.97 }
    );
    // And the runtime selector keeps serving the candidate route.
    let mut selector = Selector::new(open_gate(), decision.outcome, 16).expect("valid selector");
    assert_eq!(selector.dispatch(1), Route::Candidate);
    assert_eq!(selector.dispatch(9), Route::Candidate);
}

#[test]
fn regressed_promoted_candidate_is_demoted_not_retained() {
    let policy = EnginePolicy::default();
    let decision = decide(&policy, &open_gate(), Some(&paired(1.06)), None, None, true);
    assert_eq!(decision.outcome, PromotionOutcome::Demote);
    assert!(matches!(
        decision.reason,
        PromotionReason::MedianRegression { .. }
    ));
}

#[test]
fn not_promoted_candidate_between_targets_goes_canary() {
    let policy = EnginePolicy::default();
    let decision = decide(
        &policy,
        &open_gate(),
        Some(&paired(0.97)),
        None,
        None,
        false,
    );
    assert_eq!(decision.outcome, PromotionOutcome::Canary);
}
