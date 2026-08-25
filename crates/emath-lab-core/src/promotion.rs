//! Promotion policy engine.
//!
//! Maps gate verdict + paired stats + memory/energy evidence to one of
//! six outcomes (promote/shadow/canary/retain/demote/quarantine) with a
//! typed reason. Invariants: a gate-failing candidate is never promoted;
//! a regression retains the baseline or demotes a promoted candidate.

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
    /// Currently-promoted candidate that did not regress keeps the
    /// promoted route.
    RetainedPromotion { median_ratio: f64 },
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
            | Self::RetainedPromotion { .. }
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
            Self::RetainedPromotion { median_ratio } => {
                format!("already promoted; median ratio {median_ratio} retains the promoted route")
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
    /// Validates the policy (`E-HOST-015`).
    pub fn validate(&self) -> Result<(), LabError> {
        if !self.promote_min_median_ratio.is_finite()
            || self.promote_min_median_ratio <= 0.0
            || self.promote_min_median_ratio >= 1.0
        {
            return Err(LabError::new(
                "E-HOST-015",
                "promote_min_median_ratio must be in (0.0, 1.0)",
            ));
        }
        if !self.canary_min_median_ratio.is_finite()
            || self.canary_min_median_ratio <= 0.0
            || self.canary_min_median_ratio > 1.0
        {
            return Err(LabError::new(
                "E-HOST-015",
                "canary_min_median_ratio must be in (0.0, 1.0]",
            ));
        }
        if self.canary_min_median_ratio < self.promote_min_median_ratio {
            return Err(LabError::new(
                "E-HOST-015",
                "canary_min_median_ratio must be >= promote_min_median_ratio",
            ));
        }
        if !self.max_median_regression.is_finite() || self.max_median_regression < 1.0 {
            return Err(LabError::new(
                "E-HOST-015",
                "max_median_regression must be >= 1.0",
            ));
        }
        if !self.max_p99_regression.is_finite() || self.max_p99_regression < 1.0 {
            return Err(LabError::new(
                "E-HOST-015",
                "max_p99_regression must be >= 1.0",
            ));
        }
        if !self.max_memory_regression.is_finite() || self.max_memory_regression < 1.0 {
            return Err(LabError::new(
                "E-HOST-015",
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

/// Applies the frozen policy to gate + paired evidence. `energy` is
/// `(used, budget)`; `currently_promoted` picks demote-vs-retain.
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
    if currently_promoted {
        // Demote only fires on regression (checked above); a promoted
        // candidate between targets is still faster than baseline.
        return PromotionDecision {
            outcome: PromotionOutcome::Promote,
            reason: PromotionReason::RetainedPromotion { median_ratio },
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
