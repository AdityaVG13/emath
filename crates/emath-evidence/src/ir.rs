//! Evidence IR.
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
            "record:{}:{}:{}:{}:{}:{}:{}:{}:[{}]:{}",
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
