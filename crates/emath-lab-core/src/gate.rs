//!: correctness/quality gate API.
//!
//! Semantic, evidence and correctness checks run *before* any performance
//! measurement. A candidate whose gate fails is never measured, never
//! promoted; failures carry stable codes (`E-HOST-005` by default, or the
//! check's own evidence code such as `E-EVID-*`). The verdict is
//! deterministic: checks are evaluated in sorted label order.

use crate::error::LabError;

/// Class of a gate check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateCheckKind {
    /// Semantic check (types, shapes, domains).
    Semantic,
    /// Evidence/checker certificate validation.
    Evidence,
    /// Numeric correctness on the frozen corpus.
    Correctness,
    /// Environment/build comparability.
    Environment,
}

impl GateCheckKind {
    /// Stable kind token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Semantic => "semantic",
            Self::Evidence => "evidence",
            Self::Correctness => "correctness",
            Self::Environment => "environment",
        }
    }
}

/// One gate check result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateCheck {
    /// Stable check label (sorted at evaluation).
    pub label: String,
    /// Check kind.
    pub kind: GateCheckKind,
    /// Whether the check passed.
    pub passes: bool,
    /// Stable failure code (`E-HOST-005` default, `E-EVID-*` allowed).
    pub code: Option<&'static str>,
    /// Detail message.
    pub detail: String,
}

impl GateCheck {
    /// Passing check.
    #[must_use]
    pub fn pass(label: impl Into<String>, kind: GateCheckKind) -> Self {
        Self {
            label: label.into(),
            kind,
            passes: true,
            code: None,
            detail: String::new(),
        }
    }

    /// Failing check with a stable code.
    #[must_use]
    pub fn fail(
        label: impl Into<String>,
        kind: GateCheckKind,
        code: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            kind,
            passes: false,
            code: Some(code),
            detail: detail.into(),
        }
    }
}

/// Deterministic gate verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateVerdict {
    checks: Vec<GateCheck>,
}

impl GateVerdict {
    /// Whether the candidate is eligible for performance measurement.
    #[must_use]
    pub fn eligible(&self) -> bool {
        self.checks.iter().all(|check| check.passes)
    }

    /// Failed checks in evaluation (sorted) order.
    #[must_use]
    pub fn failures(&self) -> Vec<&GateCheck> {
        self.checks.iter().filter(|check| !check.passes).collect()
    }

    /// All checks passed, sorted by label.
    #[must_use]
    pub fn passed(&self) -> Vec<&GateCheck> {
        self.checks.iter().filter(|check| check.passes).collect()
    }

    /// Stable failure codes, sorted and deduplicated.
    #[must_use]
    pub fn failure_codes(&self) -> Vec<&'static str> {
        let mut codes: Vec<&'static str> = self
            .failures()
            .iter()
            .map(|check| check.code.unwrap_or("E-HOST-005"))
            .collect();
        codes.sort_unstable();
        codes.dedup();
        codes
    }

    /// First failure as `(code, label)`.
    #[must_use]
    pub fn first_failure(&self) -> Option<(&'static str, &str)> {
        self.failures()
            .first()
            .map(|check| (check.code.unwrap_or("E-HOST-005"), check.label.as_str()))
    }

    /// All check labels, sorted.
    #[must_use]
    pub fn labels(&self) -> Vec<&str> {
        self.checks
            .iter()
            .map(|check| check.label.as_str())
            .collect()
    }
}

/// Quality gate: evaluates checks deterministically.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QualityGate;

impl QualityGate {
    /// Evaluates checks in sorted label order; any failure blocks
    /// performance measurement (`eligible() == false`).
    #[must_use]
    pub fn evaluate(mut checks: Vec<GateCheck>) -> GateVerdict {
        checks.sort_by(|left, right| left.label.cmp(&right.label));
        GateVerdict { checks }
    }

    /// Refuses with `E-HOST-005` when the gate blocks (first failure).
    pub fn require_eligible(verdict: &GateVerdict) -> Result<(), LabError> {
        if verdict.eligible() {
            Ok(())
        } else {
            let (code, label) = verdict
                .first_failure()
                .expect("ineligible verdict has at least one failure");
            Err(LabError::new(
                code,
                format!("quality gate blocked before measurement: {label}"),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passing_checks_open_the_measurement_stage() {
        let verdict = QualityGate::evaluate(vec![
            GateCheck::pass("shape", GateCheckKind::Semantic),
            GateCheck::pass("evidence", GateCheckKind::Evidence),
        ]);
        assert!(verdict.eligible());
        assert!(QualityGate::require_eligible(&verdict).is_ok());
        assert_eq!(verdict.labels(), ["evidence", "shape"]);
    }

    #[test]
    fn correctness_failure_blocks_even_with_passing_evidence() {
        let verdict = QualityGate::evaluate(vec![
            GateCheck::pass("evidence", GateCheckKind::Evidence),
            GateCheck::fail(
                "corpus-correctness",
                GateCheckKind::Correctness,
                "E-HOST-005",
                "absolute error 1e-3 exceeds 1e-10",
            ),
        ]);
        assert!(!verdict.eligible());
        assert_eq!(verdict.failure_codes(), ["E-HOST-005"]);
        let error = QualityGate::require_eligible(&verdict).unwrap_err();
        assert_eq!(error.code, "E-HOST-005");
        assert!(error.message.contains("corpus-correctness"));
    }

    #[test]
    fn evidence_codes_are_preserved_and_sorted() {
        let verdict = QualityGate::evaluate(vec![
            GateCheck::fail(
                "missing-fallback",
                GateCheckKind::Semantic,
                "E-HOST-005",
                "no fallback for kind",
            ),
            GateCheck::fail(
                "stale-cert",
                GateCheckKind::Evidence,
                "E-EVID-121",
                "certificate points at old corpus fingerprint",
            ),
        ]);
        assert_eq!(verdict.failure_codes(), ["E-EVID-121", "E-HOST-005"]);
        // Checks are evaluated in sorted label order: "missing-fallback"
        // runs before "stale-cert".
        assert_eq!(
            verdict.first_failure(),
            Some(("E-HOST-005", "missing-fallback"))
        );
    }
}
