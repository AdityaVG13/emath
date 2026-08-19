#![forbid(unsafe_code)]

//! Deterministic interpretation portfolios.

pub mod lock;
pub mod record;
pub mod selection;

pub use lock::{PortfolioLock, replay_identity};
pub use record::{CandidateRecord, Disqualification, ExampleEvaluation, LawVerdict};
pub use selection::{SelectionOutcome, SelectionPolicy, SelectionWeights, select};

use emath_world_ir::WorldId;
use emath_world_ir::translation::{PreservationRelation, WorldMorphism};

/// Interpretation-portfolio schema version (durable id
/// `emath.interpretation-portfolio` lives in the schema registry). Bump on
/// any change to the portfolio document layout.
pub const PORTFOLIO_SCHEMA_VERSION: u32 = 1;

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

/// Admits a candidate translated along `morphism` into the target world.
///
/// The result carries [`WorldMorphism::target`], keeps the base score, and
/// records [`WorldMorphism::identity`] in provenance. Authority is capped by
/// the morphism's preservation relation: `Exact` and `Refinement` transport
/// checked meaning, so they keep the base authority; `Approximation`,
/// `Simulation`, and `ObservationalEquivalence` only guarantee a weaker
/// agreement, so they degrade to [`Authority::Structural`]. When obligations
/// disagree, any non-transporting relation wins (the cap is conservative).
#[must_use]
pub fn translated_candidate(
    morphism: &WorldMorphism,
    base: &InterpretationCandidate,
    answer: String,
) -> InterpretationCandidate {
    InterpretationCandidate {
        world_id: morphism.target,
        name: base.name.clone(),
        answer,
        authority: capped_authority(morphism, base.authority),
        score: base.score,
        provenance: morphism_provenance(base, morphism),
    }
}

fn capped_authority(morphism: &WorldMorphism, base: Authority) -> Authority {
    let transports = morphism.operator_obligations.iter().all(|obligation| {
        matches!(
            obligation.relation,
            PreservationRelation::Exact | PreservationRelation::Refinement
        )
    });
    if transports {
        base
    } else {
        Authority::Structural
    }
}

fn morphism_provenance(base: &InterpretationCandidate, morphism: &WorldMorphism) -> String {
    let handle = format!("morphism:{:x}", morphism.identity());
    if base.provenance.is_empty() {
        handle
    } else {
        format!("{}:{handle}", base.provenance)
    }
}
