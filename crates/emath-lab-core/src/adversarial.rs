//!: adversarial experiment tests.
//!
//! Deterministic detectors for changed inputs, benchmark cheating,
//! asymmetric warmup, missing failures, poisoned calibration and
//! non-comparable builds. A failing check makes the experiment
//! incomparable (`E-HOST-008`): no promotion path may consume it.

use crate::error::LabError;

/// Facts an honest harness records; detectors run over them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunFacts {
    /// Input set fingerprint for the baseline runs.
    pub baseline_input_fingerprint: Option<String>,
    /// Input set fingerprint for the candidate runs.
    pub candidate_input_fingerprint: Option<String>,
    /// Operations executed by the baseline.
    pub baseline_operations: u64,
    /// Operations executed by the candidate.
    pub candidate_operations: u64,
    /// Warmup repetitions used for the baseline.
    pub baseline_warmup_repetitions: u64,
    /// Warmup repetitions used for the candidate.
    pub candidate_warmup_repetitions: u64,
    /// Poisoned inputs injected into the candidate run.
    pub poisoned_inputs_injected: u64,
    /// Correctness failures observed during the candidate run.
    pub correctness_failures_observed: u64,
    /// Whether the calibration partition joined the decision corpus.
    pub calibration_joined_decision_corpus: bool,
    /// Crate profile of the baseline artifact.
    pub baseline_profile: String,
    /// Crate profile of the candidate artifact.
    pub candidate_profile: String,
    /// Toolchain of the baseline artifact.
    pub baseline_toolchain: String,
    /// Toolchain of the candidate artifact.
    pub candidate_toolchain: String,
}

/// One adversarial check result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdversarialCheck {
    /// Stable check id.
    pub id: &'static str,
    /// Whether the check passed.
    pub passes: bool,
    /// Detail on failure.
    pub detail: String,
}

impl AdversarialCheck {
    #[must_use]
    fn pass(id: &'static str) -> Self {
        Self {
            id,
            passes: true,
            detail: String::new(),
        }
    }

    #[must_use]
    fn fail(id: &'static str, detail: impl Into<String>) -> Self {
        Self {
            id,
            passes: false,
            detail: detail.into(),
        }
    }
}

/// Detects that baseline and candidate ran different input sets.
#[must_use]
pub fn check_changed_inputs(facts: &RunFacts) -> AdversarialCheck {
    match (
        &facts.baseline_input_fingerprint,
        &facts.candidate_input_fingerprint,
    ) {
        (Some(baseline), Some(candidate)) if baseline == candidate => {
            AdversarialCheck::pass("changed-inputs")
        }
        (Some(baseline), Some(candidate)) => AdversarialCheck::fail(
            "changed-inputs",
            format!("input sets differ: baseline {baseline}, candidate {candidate}"),
        ),
        _ => AdversarialCheck::fail(
            "changed-inputs",
            "input fingerprint missing from one side of the experiment",
        ),
    }
}

/// Detects benchmark cheating: a candidate that performed no work.
#[must_use]
pub fn check_benchmark_cheating(facts: &RunFacts) -> AdversarialCheck {
    if facts.candidate_operations == 0 {
        AdversarialCheck::fail(
            "benchmark-cheating",
            "candidate reports zero operations; result is not trusted",
        )
    } else {
        AdversarialCheck::pass("benchmark-cheating")
    }
}

/// Detects asymmetric warmup between the artifacts.
#[must_use]
pub fn check_asymmetric_warmup(facts: &RunFacts) -> AdversarialCheck {
    if facts.baseline_warmup_repetitions == facts.candidate_warmup_repetitions {
        AdversarialCheck::pass("asymmetric-warmup")
    } else {
        AdversarialCheck::fail(
            "asymmetric-warmup",
            format!(
                "warmup mismatch: baseline {}, candidate {}",
                facts.baseline_warmup_repetitions, facts.candidate_warmup_repetitions
            ),
        )
    }
}

/// Detects missing failures: poisoned inputs must produce failures.
#[must_use]
pub fn check_missing_failures(facts: &RunFacts) -> AdversarialCheck {
    if facts.poisoned_inputs_injected > 0 && facts.correctness_failures_observed == 0 {
        AdversarialCheck::fail(
            "missing-failures",
            format!(
                "{} poisoned inputs produced no correctness failure",
                facts.poisoned_inputs_injected
            ),
        )
    } else {
        AdversarialCheck::pass("missing-failures")
    }
}

/// Detects poisoned calibration: calibration data in the decision corpus.
#[must_use]
pub fn check_poisoned_calibration(facts: &RunFacts) -> AdversarialCheck {
    if facts.calibration_joined_decision_corpus {
        AdversarialCheck::fail(
            "poisoned-calibration",
            "calibration partition joined the decision corpus",
        )
    } else {
        AdversarialCheck::pass("poisoned-calibration")
    }
}

/// Detects non-comparable builds (profile/toolchain mismatch).
#[must_use]
pub fn check_non_comparable_builds(facts: &RunFacts) -> AdversarialCheck {
    if facts.baseline_profile == facts.candidate_profile
        && facts.baseline_toolchain == facts.candidate_toolchain
    {
        AdversarialCheck::pass("non-comparable-builds")
    } else {
        AdversarialCheck::fail(
            "non-comparable-builds",
            format!(
                "artifacts differ: profile {} vs {}, toolchain {} vs {}",
                facts.baseline_profile,
                facts.candidate_profile,
                facts.baseline_toolchain,
                facts.candidate_toolchain
            ),
        )
    }
}

/// Runs every detector in stable order.
#[must_use]
pub fn run_all(facts: &RunFacts) -> Vec<AdversarialCheck> {
    vec![
        check_changed_inputs(facts),
        check_benchmark_cheating(facts),
        check_asymmetric_warmup(facts),
        check_missing_failures(facts),
        check_poisoned_calibration(facts),
        check_non_comparable_builds(facts),
    ]
}

/// Refuses the experiment when any adversarial check fails
/// (`E-HOST-008`, first failure).
pub fn require_comparable(facts: &RunFacts) -> Result<(), LabError> {
    for check in run_all(facts) {
        if !check.passes {
            return Err(LabError::new(
                "E-HOST-008",
                format!("incomparable experiment: {} ({})", check.id, check.detail),
            ));
        }
    }
    Ok(())
}
