#![forbid(unsafe_code)]

//! Deterministic interpretation portfolios.

pub mod lock;
pub mod record;
pub mod selection;

pub use lock::{replay_identity, PortfolioLock};
pub use record::{CandidateRecord, Disqualification, ExampleEvaluation, LawVerdict};
pub use selection::{select, SelectionOutcome, SelectionPolicy, SelectionWeights};

use emath_world_ir::WorldId;

/// Meaning authority for a candidate result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Authority {
    /// Structural preservation only.
    Structural,
    /// Checked against examples or properties.
    Tested,
    /// Bounded or certified by an admitted checker.
    Certified,
    /// Formally proved in a declared system.
    Proved,
}

/// Multi-objective score. Lower cost/complexity and higher evidence/utility are preferred.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreVector {
    /// Abstract execution cost.
    pub cost: f64,
    /// Description complexity.
    pub complexity: f64,
    /// Evidence score.
    pub evidence: f64,
    /// Host utility score.
    pub utility: f64,
}

/// One interpretation candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct InterpretationCandidate {
    /// World identity.
    pub world_id: WorldId,
    /// Display name.
    pub name: String,
    /// Canonical answer representation.
    pub answer: String,
    /// Scoped authority.
    pub authority: Authority,
    /// Score vector.
    pub score: ScoreVector,
    /// Meaning provenance summary.
    pub provenance: String,
}

/// Deterministic candidate collection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InterpretationPortfolio {
    candidates: Vec<InterpretationCandidate>,
}

impl InterpretationPortfolio {
    /// Creates a portfolio sorted by stable policy: authority descending, utility descending,
    /// cost ascending, complexity ascending, then world identity.
    #[must_use]
    pub fn new(mut candidates: Vec<InterpretationCandidate>) -> Self {
        candidates.sort_by(|left, right| {
            right
                .authority
                .cmp(&left.authority)
                .then_with(|| right.score.utility.total_cmp(&left.score.utility))
                .then_with(|| left.score.cost.total_cmp(&right.score.cost))
                .then_with(|| left.score.complexity.total_cmp(&right.score.complexity))
                .then_with(|| left.world_id.cmp(&right.world_id))
        });
        Self { candidates }
    }

    /// Returns candidates in deterministic order.
    #[must_use]
    pub fn candidates(&self) -> &[InterpretationCandidate] {
        &self.candidates
    }
}
