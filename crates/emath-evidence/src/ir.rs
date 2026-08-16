//!: evidence IR.
//!
//! Claims, assumptions, producer/checker roles, evidence kinds,
//! freshness, falsifiers and verdicts. A record whose computation is
//! incomplete can never become resolved evidence (`E-EVID-404`).

use emath_ir::{ClaimVerdict, EvidenceClaim};

/// Evidence kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvidenceKind {
    /// Machine-checked formal proof.
    FormalProof,
    /// Residual/error-term bound.
    Residual,
    /// Interval enclosure.
    Interval,
    /// Explicit witness (certificate object).
    Witness,
    /// Differential comparison against a reference.
    Differential,
    /// Empirical measurement.
    Measurement,
    /// Structural/static analysis.
    Structural,
}

impl EvidenceKind {
    /// Stable kind token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FormalProof => "formal-proof",
            Self::Residual => "residual",
            Self::Interval => "interval",
            Self::Witness => "witness",
            Self::Differential => "differential",
            Self::Measurement => "measurement",
            Self::Structural => "structural",
        }
    }
}

/// How independent a checker is from the producer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Independence {
    /// Independent implementation, no shared code paths.
    Independent,
    /// Shares infrastructure with the producer (cooperating).
    Cooperating,
    /// No checker involved.
    None,
}

impl Independence {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Independent => "independent",
            Self::Cooperating => "cooperating",
            Self::None => "none",
        }
    }
}

/// Producer role (who produced the evidence).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProducerRole {
    /// Producer id.
    pub id: String,
    /// Evidence kind produced.
    pub kind: EvidenceKind,
    /// Producer version.
    pub version: String,
}

/// Checker role (who admits the evidence).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckerRole {
    /// Checker id.
    pub id: String,
    /// Checks this evidence kind.
    pub kind: EvidenceKind,
    /// Checker version.
    pub version: String,
    /// Independence from the producer.
    pub independence: Independence,
}

/// Freshness window and renewal triggers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Freshness {
    /// Issued timestamp (RFC3339).
    pub issued: String,
    /// Valid until timestamp (RFC3339).
    pub valid_until: String,
    /// What invalidates and renews the evidence.
    pub renews_with: Vec<String>,
}

/// Falsifier kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FalsifierKind {
    /// A counterexample witness.
    Counterexample,
    /// A precision-bound violation.
    PrecisionBound,
    /// A mutant that changed semantics.
    Mutant,
    /// A differential mismatch.
    Differential,
    /// A static/structure violation.
    StaticViolation,
}

impl FalsifierKind {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Counterexample => "counterexample",
            Self::PrecisionBound => "precision-bound",
            Self::Mutant => "mutant",
            Self::Differential => "differential",
            Self::StaticViolation => "static-violation",
        }
    }
}

/// One falsifier that must not fire for the claim to hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Falsifier {
    /// Falsifier id.
    pub id: String,
    /// Falsifier kind.
    pub kind: FalsifierKind,
    /// Detail (the condition that would falsify the claim).
    pub detail: String,
}

/// Completed evidence record over an IR claim.
#[derive(Clone, Debug, PartialEq)]
pub struct EvidenceRecord {
    /// The claim being evidenced.
    pub claim: EvidenceClaim,
    /// Evidence kind.
    pub kind: EvidenceKind,
    /// Producer role.
    pub producer: ProducerRole,
    /// Optional checker role.
    pub checker: Option<CheckerRole>,
    /// Freshness window.
    pub freshness: Freshness,
    /// Falsifiers that were checked and did not fire.
    pub falsifiers: Vec<Falsifier>,
    /// Verdict.
    pub verdict: ClaimVerdict,
    /// Whether the computation behind the claim is incomplete
    /// (interrupted, partial, or not re-runnable).
    pub incomplete: bool,
}

impl EvidenceRecord {
    /// Whether this record can count as resolved evidence: the verdict
    /// must be `Pass` and the computation must be complete
    /// (`E-EVID-404` otherwise).
    #[must_use]
    pub fn resolved(&self) -> bool {
        if self.incomplete {
            return false;
        }
        self.verdict == ClaimVerdict::Pass
    }

    /// Refusal entry when the record cannot be resolved evidence.
    #[must_use]
    pub fn refusal(&self) -> Option<crate::EvidenceError> {
        if self.resolved() {
            None
        } else if self.incomplete {
            Some(crate::EvidenceError::new(
                "E-EVID-404",
                format!(
                    "claim {} is backed by an incomplete computation",
                    self.claim.id
                ),
            ))
        } else {
            Some(crate::EvidenceError::new(
                "E-EVID-404",
                format!(
                    "claim {} verdict is {}",
                    self.claim.id,
                    self.verdict.as_str()
                ),
            ))
        }
    }

    /// Deterministic canonical record token for receipts.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut falsifiers: Vec<&Falsifier> = self.falsifiers.iter().collect();
        falsifiers.sort_by(|left, right| left.id.cmp(&right.id));
        let falsifier_token: Vec<String> = falsifiers
            .iter()
            .map(|falsifier| format!("{}:{}", falsifier.id, falsifier.kind.as_str()))
            .collect();
        format!(
            "record:v1:{}:{}:{}:{}:{}:{}:{}:{}:[{}]:{}",
            self.claim.id,
            self.kind.as_str(),
            self.producer.id,
            self.checker
                .as_ref()
                .map_or_else(|| "-".to_string(), |checker| checker.id.clone()),
            self.checker
                .as_ref()
                .map_or("none", |checker| checker.independence.as_str()),
            self.freshness.issued,
            self.freshness.valid_until,
            self.verdict.as_str(),
            falsifier_token.join(";"),
            self.incomplete,
        )
    }
}

/// Free-standing gate matching the acceptance rule.
#[must_use]
pub fn can_become_resolved(record: &EvidenceRecord) -> bool {
    record.resolved()
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_ir::{ClaimVerdict, EvidenceLevel};

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
    fn complete_pass_claim_is_resolved_evidence() {
        let record = record(false, ClaimVerdict::Pass);
        assert!(can_become_resolved(&record));
        assert!(record.refusal().is_none());
    }

    #[test]
    fn incomplete_computation_cannot_become_resolved_evidence() {
        let record = record(true, ClaimVerdict::Pass);
        assert!(!can_become_resolved(&record));
        assert_eq!(record.refusal().unwrap().code, "E-EVID-404");
    }

    #[test]
    fn non_pass_verdict_is_not_resolved() {
        let mut record = record(false, ClaimVerdict::Inconclusive);
        assert!(!record.resolved());
        record.verdict = ClaimVerdict::Fail;
        assert!(!record.resolved());
        assert!(record.refusal().is_some());
    }

    #[test]
    fn canonical_token_is_stable_and_sensitive() {
        let first = record(false, ClaimVerdict::Pass);
        let mut second = first.clone();
        second.freshness.valid_until = "2098-01-01T00:00:00Z".into();
        assert_eq!(first.canonical(), first.canonical());
        assert_ne!(first.canonical(), second.canonical());
    }
}
