#![forbid(unsafe_code)]

//! Deterministic interpretation portfolios.

pub mod interpretation;
pub mod lock;
pub mod meaning_lock;
pub mod record;
pub mod selection;

pub use interpretation::{
    archive, evaluate, rank_candidates, replay, CollapsePolicy, DisqualificationReason,
    InterpretationPolicy, LedgerEntry, MetricAxis, MetricPolarity, ParetoArchive, PortfolioError,
    PortfolioReceipt, ReceiptInput, RANKING_KEY_SPEC, RECEIPT_SCHEMA, RECEIPT_VERSION,
};
pub use lock::{replay_identity, PortfolioLock};
pub use meaning_lock::{
    apply_portfolio_cap, commit_locked_world, refuse_disqualified, LockEntry, LockError, LockKey,
    MeaningLock, SelectionMethod, DEFAULT_PORTFOLIO_CAP, LOCK_DIR, LOCK_FILE_NAME, LOCK_SCHEMA,
    LOCK_SCHEMA_VERSION, PROVENANCE_USER_LOCKED, WHOLE_TERM_HOLE,
};
pub use record::{
    CandidateRecord, Disqualification, ExampleEvaluation, GuardFailure, LawVerdict, WorldCandidate,
};
pub use selection::{select, SelectionOutcome, SelectionPolicy, SelectionWeights};

use emath_world_ir::translation::{PreservationRelation, WorldMorphism};
use emath_world_ir::WorldId;

/// Meaning authority for a candidate result (defined in `emath-lab-core`,
/// re-exported here as portfolio vocabulary).
pub use emath_lab_core::Authority;

/// Interpretation-portfolio schema version (durable id
/// `emath.interpretation-portfolio` lives in the schema registry). Bump on
/// any change to the portfolio document layout.
pub const PORTFOLIO_SCHEMA_VERSION: u32 = 1;

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

impl InterpretationCandidate {
    /// Projects this bag member onto G7 with uniform cost so `evaluate`
    /// cannot silently drop a `keep: pareto N` world.
    #[must_use]
    pub fn world_candidate(&self) -> WorldCandidate {
        WorldCandidate::bag_member(self.world_id.0, "builtin-seed", self.authority)
    }
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
/// Authority is capped by the preservation relation: `Exact`/`Refinement`
/// transport it; anything weaker degrades to [`Authority::Structural`].
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
