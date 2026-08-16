//!: promotion policy engine.
//!
//! Maps gate verdict + paired statistics + memory/energy evidence to one
//! of six outcomes — promote, shadow, canary, retain, demote, quarantine —
//! with a typed reason for every non-promote path. Hard invariants: a
//! candidate that fails the quality gate is never promoted, and a metric
//! regression retains the baseline (or demotes a promoted candidate)
//! according to the frozen policy.

use crate::error::LabError;
use crate::gate::GateVerdict;
use crate::stats::PairedResult;

/// Promotion outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromotionOutcome {
    /// Route all traffic to the candidate.
    Promote,
    /// Measure the candidate in production, never serve.
    Shadow,
    /// Serve a small canary cohort.
    Canary,
    /// Keep the baseline; no change.
    Retain,
    /// Step a promoted candidate back to baseline.
    Demote,
    /// Block the candidate from all promotion paths.
    Quarantine,
}

impl PromotionOutcome {
    /// Stable outcome token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Promote => "promote",
            Self::Shadow => "shadow",
            Self::Canary => "canary",
            Self::Retain => "retain",
            Self::Demote => "demote",
            Self::Quarantine => "quarantine",
        }
    }
}

/// Typed reason for a promotion decision.
#[derive(Clone, Debug, PartialEq)]
pub enum PromotionReason {
    /// Quality gate blocked the candidate (`E-HOST-005`, or the check's
    /// own evidence code).
    CorrectnessFailed { code: &'static str, detail: String },
    /// Runtime drift detected (`E-HOST-010`); the candidate was demoted.
    DriftDetected {
        /// Drift dimension.
        kind: crate::drift::DriftKind,
        /// Metric id that drifted.
        metric: String,
    },
    /// Evidence missing for a declared metric.
    EvidenceMissing { metric: String },
    /// Too few samples for a decision (`E-HOST-006`).
    InsufficientSamples { need: u64, got: u64 },
    /// No paired comparison is available (`E-HOST-008`).
    Incomparable { detail: String },
    /// Median ratio regressed below the policy floor (`E-HOST-007`).
    MedianRegression { median_ratio: f64 },
    /// p99 ratio regressed beyond the policy ceiling (`E-HOST-007`).
    P99Regression { p99_ratio: f64 },
    /// Peak-memory ratio regressed (`E-HOST-007`).
    MemoryRegression { ratio: f64 },
    /// Energy exceeded the frozen budget (`E-HOST-007`).
    EnergyOverBudget { joules: f64, budget: f64 },
    /// Candidate meets the promotion target.
    MeetsTarget { median_ratio: f64 },
    /// Candidate is fit for a canary cohort.
    CanaryCohort { median_ratio: f64 },
    /// Not below target yet; shadow for more evidence.
    NeedsMoreData { median_ratio: f64 },
    /// Candidate is slower but inside tolerance.
    TooSlow { median_ratio: f64 },
    /// Manual hold by an operator.
    ManualHold,
}

impl PromotionReason {
    /// Stable code for this reason, when it is a refusal.
    #[must_use]
    pub const fn code(&self) -> Option<&'static str> {
        match self {
            Self::CorrectnessFailed { code, .. } => Some(*code),
            Self::DriftDetected { .. } => Some("E-HOST-010"),
            Self::EvidenceMissing { .. } => Some("E-HOST-005"),
            Self::InsufficientSamples { .. } => Some("E-HOST-006"),
            Self::Incomparable { .. } => Some("E-HOST-008"),
            Self::MedianRegression { .. }
            | Self::P99Regression { .. }
            | Self::MemoryRegression { .. }
            | Self::EnergyOverBudget { .. } => Some("E-HOST-007"),
            Self::MeetsTarget { .. }
            | Self::CanaryCohort { .. }
            | Self::NeedsMoreData { .. }
            | Self::TooSlow { .. }
            | Self::ManualHold => None,
        }
    }

    /// One-line stable description used in receipts.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::CorrectnessFailed { code, detail } => {
                format!("{code}: {detail}")
            }
            Self::DriftDetected { kind, metric } => {
                format!("E-HOST-010: {} drift in {metric}", kind.as_str())
            }
            Self::EvidenceMissing { metric } => {
                format!("E-HOST-005: evidence missing for {metric}")
            }
            Self::InsufficientSamples { need, got } => {
                format!("E-HOST-006: needed {need} samples, got {got}")
            }
            Self::Incomparable { detail } => format!("E-HOST-008: {detail}"),
            Self::MedianRegression { median_ratio } => {
                format!("E-HOST-007: median ratio {median_ratio} below floor")
            }
            Self::P99Regression { p99_ratio } => {
                format!("E-HOST-007: p99 ratio {p99_ratio} above ceiling")
            }
            Self::MemoryRegression { ratio } => {
                format!("E-HOST-007: memory ratio {ratio} above ceiling")
            }
            Self::EnergyOverBudget { joules, budget } => {
                format!("E-HOST-007: energy {joules} J over budget {budget} J")
            }
            Self::MeetsTarget { median_ratio } => {
                format!("median ratio {median_ratio} meets promotion target")
            }
            Self::CanaryCohort { median_ratio } => {
                format!("median ratio {median_ratio} qualifies for canary")
            }
            Self::NeedsMoreData { median_ratio } => {
                format!("median ratio {median_ratio}; shadow for more evidence")
            }
            Self::TooSlow { median_ratio } => {
                format!("median ratio {median_ratio} below parity")
            }
            Self::ManualHold => "manual hold".to_string(),
        }
    }
}

/// Frozen promotion policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnginePolicy {
    /// Minimum median ratio (candidate/baseline) to promote.
    pub promote_min_median_ratio: f64,
    /// Minimum median ratio to enter the canary cohort.
    pub canary_min_median_ratio: f64,
    /// Worst acceptable median ratio (below this = regression).
    pub max_median_regression: f64,
    /// Worst acceptable p99 ratio.
    pub max_p99_regression: f64,
    /// Worst acceptable peak-memory ratio.
    pub max_memory_regression: f64,
    /// Quarantine (not merely retain) on gate failure.
    pub quarantine_on_gate_failure: bool,
}

impl Default for EnginePolicy {
    fn default() -> Self {
        Self {
            promote_min_median_ratio: 0.95, // candidate 5% faster than baseline
            canary_min_median_ratio: 0.99,
            max_median_regression: 1.05, // slower by more than 5% is a regression
            max_p99_regression: 1.10,
            max_memory_regression: 1.10,
            quarantine_on_gate_failure: true,
        }
    }
}

impl EnginePolicy {
    /// Validates the policy (`E-HOST-003`).
    pub fn validate(&self) -> Result<(), LabError> {
        if !self.promote_min_median_ratio.is_finite()
            || self.promote_min_median_ratio <= 0.0
            || self.promote_min_median_ratio >= 1.0
        {
            return Err(LabError::new(
                "E-HOST-003",
                "promote_min_median_ratio must be in (0.0, 1.0)",
            ));
        }
        if !self.canary_min_median_ratio.is_finite()
            || self.canary_min_median_ratio <= 0.0
            || self.canary_min_median_ratio > 1.0
        {
            return Err(LabError::new(
                "E-HOST-003",
                "canary_min_median_ratio must be in (0.0, 1.0]",
            ));
        }
        if self.canary_min_median_ratio < self.promote_min_median_ratio {
            return Err(LabError::new(
                "E-HOST-003",
                "canary_min_median_ratio must be >= promote_min_median_ratio",
            ));
        }
        if !self.max_median_regression.is_finite() || self.max_median_regression < 1.0 {
            return Err(LabError::new(
                "E-HOST-003",
                "max_median_regression must be >= 1.0",
            ));
        }
        if !self.max_p99_regression.is_finite() || self.max_p99_regression < 1.0 {
            return Err(LabError::new(
                "E-HOST-003",
                "max_p99_regression must be >= 1.0",
            ));
        }
        if !self.max_memory_regression.is_finite() || self.max_memory_regression < 1.0 {
            return Err(LabError::new(
                "E-HOST-003",
                "max_memory_regression must be >= 1.0",
            ));
        }
        Ok(())
    }
}

/// Promotion decision.
#[derive(Clone, Debug, PartialEq)]
pub struct PromotionDecision {
    /// Outcome.
    pub outcome: PromotionOutcome,
    /// Typed reason.
    pub reason: PromotionReason,
}

impl PromotionDecision {
    /// Stable one-line receipt entry.
    #[must_use]
    pub fn receipt_line(&self) -> String {
        format!("{}: {}", self.outcome.as_str(), self.reason.describe())
    }
}

/// Applies the frozen policy to the gate verdict and paired evidence.
///
/// `memory_ratio` is candidate/baseline peak memory; `energy` is
/// `(joules_used, budget_joules)`. `currently_promoted` selects
/// demote-vs-retain on regression.
#[must_use]
pub fn decide(
    policy: &EnginePolicy,
    gate: &GateVerdict,
    paired: Option<&PairedResult>,
    memory_ratio: Option<f64>,
    energy: Option<(f64, f64)>,
    currently_promoted: bool,
) -> PromotionDecision {
    debug_assert!(policy.validate().is_ok(), "policy must be validated");
    if policy.quarantine_on_gate_failure {
        if let Some((code, label)) = gate.first_failure() {
            return PromotionDecision {
                outcome: PromotionOutcome::Quarantine,
                reason: PromotionReason::CorrectnessFailed {
                    code,
                    detail: label.to_string(),
                },
            };
        }
    } else if let Some((code, label)) = gate.first_failure() {
        return PromotionDecision {
            outcome: PromotionOutcome::Retain,
            reason: PromotionReason::CorrectnessFailed {
                code,
                detail: label.to_string(),
            },
        };
    }
    let Some(paired) = paired else {
        return PromotionDecision {
            outcome: PromotionOutcome::Retain,
            reason: PromotionReason::Incomparable {
                detail: "no paired comparison available".to_string(),
            },
        };
    };
    if paired.samples_used < 3 {
        return PromotionDecision {
            outcome: PromotionOutcome::Retain,
            reason: PromotionReason::InsufficientSamples {
                need: 3,
                got: paired.samples_used,
            },
        };
    }
    if let Some((joules, budget)) = energy {
        if joules > budget {
            return regression_decision(
                PromotionReason::EnergyOverBudget { joules, budget },
                currently_promoted,
            );
        }
    }
    if let Some(ratio) = memory_ratio {
        if ratio > policy.max_memory_regression {
            return regression_decision(
                PromotionReason::MemoryRegression { ratio },
                currently_promoted,
            );
        }
    }
    if paired.p99_ratio > policy.max_p99_regression {
        return regression_decision(
            PromotionReason::P99Regression {
                p99_ratio: paired.p99_ratio,
            },
            currently_promoted,
        );
    }
    if paired.median_ratio > policy.max_median_regression {
        return regression_decision(
            PromotionReason::MedianRegression {
                median_ratio: paired.median_ratio,
            },
            currently_promoted,
        );
    }
    let median_ratio = paired.median_ratio;
    if median_ratio <= policy.promote_min_median_ratio {
        return PromotionDecision {
            outcome: PromotionOutcome::Promote,
            reason: PromotionReason::MeetsTarget { median_ratio },
        };
    }
    if !currently_promoted && median_ratio <= policy.canary_min_median_ratio {
        return PromotionDecision {
            outcome: PromotionOutcome::Canary,
            reason: PromotionReason::CanaryCohort { median_ratio },
        };
    }
    if !currently_promoted && median_ratio <= 1.0 {
        return PromotionDecision {
            outcome: PromotionOutcome::Shadow,
            reason: PromotionReason::NeedsMoreData { median_ratio },
        };
    }
    PromotionDecision {
        outcome: PromotionOutcome::Retain,
        reason: PromotionReason::TooSlow { median_ratio },
    }
}

/// Demotes a promoted candidate on regression, otherwise retains baseline.
#[must_use]
fn regression_decision(reason: PromotionReason, currently_promoted: bool) -> PromotionDecision {
    PromotionDecision {
        outcome: if currently_promoted {
            PromotionOutcome::Demote
        } else {
            PromotionOutcome::Retain
        },
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::{GateCheck, GateCheckKind};

    fn passing_gate() -> GateVerdict {
        crate::gate::QualityGate::evaluate(vec![
            GateCheck::pass("correctness", GateCheckKind::Correctness),
            GateCheck::pass("evidence", GateCheckKind::Evidence),
        ])
    }

    fn blocked_gate() -> GateVerdict {
        crate::gate::QualityGate::evaluate(vec![
            GateCheck::pass("evidence", GateCheckKind::Evidence),
            GateCheck::fail(
                "correctness",
                GateCheckKind::Correctness,
                "E-HOST-005",
                "absolute error out of tolerance",
            ),
        ])
    }

    fn paired(median_ratio: f64) -> PairedResult {
        PairedResult {
            samples_used: 8,
            outliers_removed: 0,
            median_baseline_ns: 100.0,
            median_candidate_ns: 100.0 * median_ratio,
            median_ratio,
            p99_ratio: if median_ratio <= 1.05 {
                median_ratio * 1.02
            } else {
                median_ratio
            },
            wins: 6,
            losses: 2,
            ties: 0,
            raw_retained: true,
            paired: true,
            seed: 0,
        }
    }

    #[test]
    fn correctness_failure_never_promotes_despite_speed() {
        let policy = EnginePolicy::default();
        let decision = decide(
            &policy,
            &blocked_gate(),
            Some(&paired(0.5)),
            None,
            None,
            false,
        );
        assert_eq!(decision.outcome, PromotionOutcome::Quarantine);
        assert_eq!(decision.reason.code(), Some("E-HOST-005"));
        assert!(decision
            .receipt_line()
            .starts_with("quarantine: E-HOST-005"));
    }

    #[test]
    fn regression_retains_baseline_and_demotes_incumbent() {
        let policy = EnginePolicy::default();
        let slow = paired(1.20);
        let decision = decide(&policy, &passing_gate(), Some(&slow), None, None, false);
        assert_eq!(decision.outcome, PromotionOutcome::Retain);
        assert_eq!(decision.reason.code(), Some("E-HOST-007"));
        let promoted = decide(&policy, &passing_gate(), Some(&slow), None, None, true);
        assert_eq!(promoted.outcome, PromotionOutcome::Demote);
    }

    #[test]
    fn promotion_ladder_is_respected() {
        let policy = EnginePolicy::default();
        let promote = decide(
            &policy,
            &passing_gate(),
            Some(&paired(0.90)),
            None,
            None,
            false,
        );
        assert_eq!(promote.outcome, PromotionOutcome::Promote);
        let canary = decide(
            &policy,
            &passing_gate(),
            Some(&paired(0.97)),
            None,
            None,
            false,
        );
        assert_eq!(canary.outcome, PromotionOutcome::Canary);
        let shadow = decide(
            &policy,
            &passing_gate(),
            Some(&paired(1.0)),
            None,
            None,
            false,
        );
        assert_eq!(shadow.outcome, PromotionOutcome::Shadow);
        let slow = decide(
            &policy,
            &passing_gate(),
            Some(&paired(1.03)),
            None,
            None,
            false,
        );
        assert_eq!(slow.outcome, PromotionOutcome::Retain);
    }

    #[test]
    fn missing_or_thin_evidence_holds_promotion() {
        let policy = EnginePolicy::default();
        let missing = decide(&policy, &passing_gate(), None, None, None, false);
        assert_eq!(missing.outcome, PromotionOutcome::Retain);
        assert_eq!(missing.reason.code(), Some("E-HOST-008"));
        let mut thin = paired(0.5);
        thin.samples_used = 2;
        let held = decide(&policy, &passing_gate(), Some(&thin), None, None, false);
        assert_eq!(held.outcome, PromotionOutcome::Retain);
        assert_eq!(held.reason.code(), Some("E-HOST-006"));
    }

    #[test]
    fn energy_and_memory_regressions_are_typed() {
        let policy = EnginePolicy::default();
        let energy = decide(
            &policy,
            &passing_gate(),
            Some(&paired(0.5)),
            None,
            Some((30.0, 20.0)),
            false,
        );
        assert_eq!(energy.outcome, PromotionOutcome::Retain);
        let memory = decide(
            &policy,
            &passing_gate(),
            Some(&paired(0.5)),
            Some(1.5),
            None,
            false,
        );
        assert_eq!(memory.reason.code(), Some("E-HOST-007"));
    }

    #[test]
    fn invalid_policy_is_refused() {
        let policy = EnginePolicy {
            promote_min_median_ratio: 2.0,
            ..EnginePolicy::default()
        };
        let error = policy.validate().unwrap_err();
        assert_eq!(error.code, "E-HOST-003");
        let inverted = EnginePolicy {
            canary_min_median_ratio: 0.5,
            ..EnginePolicy::default()
        };
        assert_eq!(inverted.validate().unwrap_err().code, "E-HOST-003");
    }
}
