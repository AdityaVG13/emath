//! Evidence IR, assumption ledger, evidence-level policy and certificate
//! registry (Phase 8, ).
//!
//! Authority is explicit: every claim names its kind, producer and
//! checker roles, freshness window and falsifiers; assumptions are
//! classified M/N/S/E/H; the E0–E5 policy maps requirements to
//! admissible producer/checker combinations per claim class; the
//! certificate registry holds versioned checker contracts.
//!
//! Stable codes (`E-EVID-*`, evidence/checker area):
//! - `E-EVID-401` unknown certificate kind;
//! - `E-EVID-402` duplicate versioned checker contract;
//! - `E-EVID-403` checker contract does not admit the claim class;
//! - `E-EVID-404` incomplete computation cannot become resolved
//!   evidence;
//! - `E-EVID-405` assumption already registered under a different class.

#![forbid(unsafe_code)]

pub mod ir;
pub mod ledger;
pub mod policy;
pub mod registry;

pub use ir::{
    can_become_resolved, CheckerRole, EvidenceKind, EvidenceRecord, Falsifier, FalsifierKind,
    Freshness, Independence, ProducerRole,
};
pub use ledger::{premise_class_token, Assumption, AssumptionLedger, PremiseClass};
pub use policy::{admissible_combos, requirement_for, satisfied_by, EvidenceEntry, EvidencePolicy};
pub use registry::{
    admits_claim_class, lookup_contract, register_contract, CertificateKind, CertificateRegistry,
    CheckerContract,
};

/// Shared evidence failure with a stable code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceError {
    /// Stable code (`E-EVID-401`..`E-EVID-405`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

impl EvidenceError {
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for EvidenceError {}
