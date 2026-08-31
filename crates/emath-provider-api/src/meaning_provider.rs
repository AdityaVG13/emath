#![forbid(unsafe_code)]

//! Stable contracts for meaning proposal and world checking.

use emath_term::{Signature, Term};
use emath_world_ir::{MeaningHole, WorldIr};

/// Bounded provider request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Maximum proposals.
    pub max_proposals: usize,
    /// Maximum abstract work units.
    pub max_work_units: u64,
    /// Deterministic seed.
    pub seed: u64,
}

/// A provider-neutral meaning problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeaningProblem {
    /// Signature.
    pub signature: Signature,
    /// Root term.
    pub term: Term,
    /// Open semantic holes.
    pub holes: Vec<MeaningHole>,
    /// Hard constraints.
    pub constraints: Vec<String>,
    /// Behavioral examples encoded canonically in the seed API.
    pub examples: Vec<String>,
}

/// A proposed world with no automatic authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldCandidate {
    /// Proposed world.
    pub world: WorldIr,
    /// Provider identity and version.
    pub provider_id: String,
    /// Claims that require checking.
    pub claimed_obligations: Vec<String>,
    /// Provider-local provenance receipt.
    pub proposal_receipt: String,
}

/// World obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldObligation {
    /// Stable obligation name.
    pub id: String,
    /// Canonical statement.
    pub statement: String,
}

/// One checked obligation verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObligationVerdict {
    /// Obligation ID.
    pub obligation_id: String,
    /// Whether it passed.
    pub passed: bool,
    /// Optional counterexample or reason.
    pub detail: String,
}

/// Checker report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldCheckReport {
    /// Checker identity and version.
    pub checker_id: String,
    /// Per-obligation verdicts.
    pub verdicts: Vec<ObligationVerdict>,
    /// Canonical checker receipt.
    pub receipt: String,
}

/// Provider error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    /// Stable code.
    pub code: String,
    /// Human-readable detail.
    pub detail: String,
}

/// Checker error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckerError {
    /// Stable code.
    pub code: String,
    /// Human-readable detail.
    pub detail: String,
}

/// Proposes candidate meanings.
pub trait MeaningProvider {
    /// Stable provider identity.
    fn provider_id(&self) -> &str;

    /// Produces bounded proposals.
    fn propose(
        &self,
        problem: &MeaningProblem,
        budget: Budget,
    ) -> Result<Vec<WorldCandidate>, ProviderError>;
}

/// Independently challenges candidate worlds.
pub trait WorldChecker {
    /// Stable checker identity.
    fn checker_id(&self) -> &str;

    /// Checks obligations under a bounded request.
    fn check(
        &self,
        candidate: &WorldCandidate,
        obligations: &[WorldObligation],
        budget: Budget,
    ) -> Result<WorldCheckReport, CheckerError>;
}
