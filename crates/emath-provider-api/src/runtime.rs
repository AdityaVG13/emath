//! Runtime outcome model: budgets, cancellation, evidence handles,
//! continuations and explicit `Outcome::Unresolved`.

#![forbid(unsafe_code)]

use emath_core::{ContentId, SchemaId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub evaluations: u64,
    pub iterations: u64,
    pub work_units: u128,
    pub memory_bytes: u64,
    pub output_bytes: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            evaluations: 1_000_000,
            iterations: 10_000,
            work_units: 100_000_000,
            memory_bytes: 1 << 30,
            output_bytes: 64 << 20,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceHandle {
    pub schema: SchemaId,
    pub identity: ContentId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContinuationHandle {
    pub schema: SchemaId,
    pub identity: ContentId,
    pub provider_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnresolvedReason {
    MissingProvider,
    UnsupportedSemanticSubset,
    BudgetExhausted,
    InconclusiveEvidence,
    TargetUnavailable,
    PermissionDenied,
}

impl UnresolvedReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingProvider => "missing-provider",
            Self::UnsupportedSemanticSubset => "unsupported-semantic-subset",
            Self::BudgetExhausted => "budget-exhausted",
            Self::InconclusiveEvidence => "inconclusive-evidence",
            Self::TargetUnavailable => "target-unavailable",
            Self::PermissionDenied => "permission-denied",
        }
    }
}

/// Provider execution outcomes: only `Resolved` carries value authority.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome<T, E> {
    Resolved {
        value: T,
        evidence: EvidenceHandle,
    },
    Unresolved {
        reason: UnresolvedReason,
        partial: Option<T>,
        continuation: Option<ContinuationHandle>,
        evidence: EvidenceHandle,
    },
    Failed(E),
}

impl<T, E> Outcome<T, E> {
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved { .. })
    }
}

pub trait Cancellation {
    fn is_cancelled(&self) -> bool;
}

pub struct NeverCancel;
impl Cancellation for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}
