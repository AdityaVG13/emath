//! Full candidate record (spec 11): every axis a portfolio selection can
//! consider, plus explicit disqualifications.

use emath_world_ir::{fnv1a64, WorldId};

use crate::{Authority, ScoreVector};

/// One behavioral-example evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExampleEvaluation {
    /// Example content identity.
    pub example_id: u64,
    /// Whether the candidate satisfied it.
    pub satisfied: bool,
}

/// One law verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawVerdict {
    /// Law name or canonical text.
    pub law: String,
    /// Whether the candidate passed it.
    pub passed: bool,
}

/// An explicit disqualification; a disqualified record is never viable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disqualification {
    /// Stable machine-readable code, e.g. `hard-constraint:violated`.
    pub code: String,
    /// Human-readable detail.
    pub detail: String,
}

/// The full portfolio candidate record (spec 11, "Candidate record").
///
/// `identity` is an FNV-1a64 content identity over the canonical form, so
/// portfolio generation and replay are deterministic.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateRecord {
    /// Candidate world identity.
    pub world_id: WorldId,
    /// Parse candidate label.
    pub parse_candidate: String,
    /// World summary text.
    pub world_summary: String,
    /// Answer class label.
    pub answer_class: String,
    /// Behavioral-example evaluations, sorted by example id.
    pub example_evaluations: Vec<ExampleEvaluation>,
    /// Law verdicts, sorted by law.
    pub law_verdicts: Vec<LawVerdict>,
    /// Hard-constraint verdict.
    pub hard_constraint_verdict: bool,
    /// Meaning authority as measured; selection never raises it.
    pub authority: Authority,
    /// Execution cost in host units.
    pub execution_cost: u64,
    /// Memory cost in host units.
    pub memory_cost: u64,
    /// Whether an execution artifact exists.
    pub artifact_available: bool,
    /// Provider provenance.
    pub provider_provenance: String,
    /// Checker provenance.
    pub checker_provenance: String,
    /// Multi-objective score vector.
    pub score: ScoreVector,
    /// Explicit disqualifications, sorted by code.
    pub disqualifications: Vec<Disqualification>,
    /// Deterministic content identity.
    pub identity: u64,
}

impl CandidateRecord {
    /// Builds a record with deterministic content identity; collections
    /// are sorted internally.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world_id: WorldId,
        parse_candidate: impl Into<String>,
        world_summary: impl Into<String>,
        answer_class: impl Into<String>,
        mut example_evaluations: Vec<ExampleEvaluation>,
        mut law_verdicts: Vec<LawVerdict>,
        hard_constraint_verdict: bool,
        authority: Authority,
        execution_cost: u64,
        memory_cost: u64,
        artifact_available: bool,
        provider_provenance: impl Into<String>,
        checker_provenance: impl Into<String>,
        score: ScoreVector,
        mut disqualifications: Vec<Disqualification>,
    ) -> Self {
        example_evaluations.sort_by_key(|evaluation| evaluation.example_id);
        law_verdicts.sort_by_key(|verdict| verdict.law.clone());
        disqualifications.sort_by_key(|disqualification| disqualification.code.clone());
        let record = Self {
            world_id,
            parse_candidate: parse_candidate.into(),
            world_summary: world_summary.into(),
            answer_class: answer_class.into(),
            example_evaluations,
            law_verdicts,
            hard_constraint_verdict,
            authority,
            execution_cost,
            memory_cost,
            artifact_available,
            provider_provenance: provider_provenance.into(),
            checker_provenance: checker_provenance.into(),
            score,
            disqualifications,
            identity: 0,
        };
        let identity = fnv1a64(record.canonical().as_bytes());
        Self { identity, ..record }
    }

    /// Whether the candidate can be selected at all: the hard-constraint
    /// verdict passes and no disqualification is recorded.
    #[must_use]
    pub fn viable(&self) -> bool {
        self.hard_constraint_verdict && self.disqualifications.is_empty()
    }

    /// Satisfied-law fraction in permille; a record with no law verdicts
    /// gets no credit (0).
    #[must_use]
    pub fn law_permille(&self) -> u64 {
        let total = self.law_verdicts.len();
        if total == 0 {
            return 0;
        }
        let passed = self
            .law_verdicts
            .iter()
            .filter(|verdict| verdict.passed)
            .count();
        (passed as u64 * 1000) / (total as u64)
    }

    /// Deterministic canonical form (identity excluded).
    #[must_use]
    pub fn canonical(&self) -> String {
        let evaluations = self
            .example_evaluations
            .iter()
            .map(|evaluation| format!("{}={}", evaluation.example_id, evaluation.satisfied))
            .collect::<Vec<_>>()
            .join(";");
        let laws = self
            .law_verdicts
            .iter()
            .map(|verdict| format!("{}={}", verdict.law, verdict.passed))
            .collect::<Vec<_>>()
            .join(";");
        let disqualifications = self
            .disqualifications
            .iter()
            .map(|disqualification| {
                format!("{}:{}", disqualification.code, disqualification.detail)
            })
            .collect::<Vec<_>>()
            .join(";");
        format!(
            "record:world={}:parse={}:summary={}:class={}:examples={}:laws={}:hard={}:authority={:?}:exec={}:mem={}:artifact={}:provider={}:checker={}:cost={}:complexity={}:evidence={}:utility={}:disq={}",
            self.world_id.0,
            self.parse_candidate,
            self.world_summary,
            self.answer_class,
            evaluations,
            laws,
            self.hard_constraint_verdict,
            self.authority,
            self.execution_cost,
            self.memory_cost,
            self.artifact_available,
            self.provider_provenance,
            self.checker_provenance,
            self.score.cost,
            self.score.complexity,
            self.score.evidence,
            self.score.utility,
            disqualifications,
        )
    }
}
