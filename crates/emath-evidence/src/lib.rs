//! Evidence IR, assumption ledger, evidence-level policy, certificate
//! registry, content-addressed store, revalidation, optional proof
//! providers and the certify-the-certifier corpus (Phase 8,
//! ).
//!
//! Authority is explicit: every claim names its kind, producer and
//! checker roles, freshness window and falsifiers; assumptions are
//! classified M/N/S/E/H; the E0–E5 policy maps requirements to
//! admissible producer/checker combinations per claim class; the
//! certificate registry holds versioned checker contracts; the store
//! addresses records by content and keeps revocation/supersession
//! append-only; revalidation sweeps stale evidence; proof kernels are
//! an optional seam; and a fixed unsound-certifier corpus is refused.
//!
//! Stable codes (`E-EVID-*`, evidence/checker area):
//! - `E-EVID-101`..`E-EVID-111` independent artifact checker (emath-checker);
//! - `E-EVID-201` claim-language lint (emath-checker);
//! - `E-EVID-301`/`E-EVID-302` translation validation (emath-checker);
//! - `E-EVID-401` unknown certificate kind;
//! - `E-EVID-402` duplicate versioned checker contract;
//! - `E-EVID-403` checker contract does not admit the claim class;
//! - `E-EVID-404` incomplete computation cannot become resolved
//!   evidence;
//! - `E-EVID-405` assumption already registered under a different class;
//! - `E-EVID-501` unknown evidence record id;
//! - `E-EVID-502` duplicate append-only revocation marker;
//! - `E-EVID-503` content-identity mismatch (tamper);
//! - `E-EVID-504` double supersession (append-only conflict);
//! - `E-EVID-505` stale record refused for promotion;
//! - `E-EVID-506` proof provider unavailable (optional path refusal);
//! - `E-EVID-507` unsound certifier output rejected.

#![forbid(unsafe_code)]

pub mod certify;
pub mod ir;
pub mod ledger;
pub mod policy;
pub mod proof;
pub mod registry;
pub mod revalidation;
pub mod store;

pub use certify::{reject_unsound_certifier_output, UnsoundFixture, CERTIFY_THE_CERTIFIER};
pub use ir::{
    can_become_resolved, CheckerRole, EvidenceKind, EvidenceRecord, Falsifier, FalsifierKind,
    Freshness, Independence, ProducerRole,
};
pub use ledger::{premise_class_token, Assumption, AssumptionLedger, PremiseClass};
pub use policy::{admissible_combos, requirement_for, satisfied_by, EvidenceEntry, EvidencePolicy};
pub use proof::{verify_proof_optional, ProofProvider, ProofVerdict, ProofVerdictKind};
pub use registry::{
    admits_claim_class, lookup_contract, register_contract, CertificateKind, CertificateRegistry,
    CheckerContract,
};
pub use revalidation::{
    require_promotable, revalidation_sweep, RevalidationConfig, RevalidationReport,
    RevalidationTrigger,
};
pub use store::{store_address, EvidenceStore};

/// Shared evidence failure with a stable code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceError {
    /// Stable code (`E-EVID-401`..`E-EVID-507`).
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

/// Shared test fixture: a resolved interval record with a pass verdict.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use emath_ir::{ClaimVerdict, EvidenceClaim, EvidenceLevel};

    pub(crate) fn sample_record() -> EvidenceRecord {
        EvidenceRecord {
            claim: EvidenceClaim {
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
            },
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
            verdict: ClaimVerdict::Pass,
            incomplete: false,
        }
    }
}
