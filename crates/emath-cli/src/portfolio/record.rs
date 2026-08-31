//! Full candidate record: every axis a portfolio selection can
//! consider, plus explicit disqualifications.

use std::collections::BTreeMap;

use emath_world_ir::{fnv1a64, WorldId};

use crate::portfolio::{Authority, ScoreVector};

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

/// The full portfolio candidate record ("Candidate record").
///
/// `identity` is an FNV-1a64 content identity over the canonical form.
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

    /// Projects this genesis-era record onto the G7 [`WorldCandidate`].
    ///
    /// Floats become milli-unit integers ([`milli_units`]); `world_id` is the
    /// fingerprint, `identity` the artifact hash, first disqualification a guard failure.
    #[must_use]
    pub fn world_candidate(&self) -> WorldCandidate {
        let mut metrics = BTreeMap::new();
        metrics.insert("cost".to_string(), milli_units(self.score.cost));
        metrics.insert("complexity".to_string(), milli_units(self.score.complexity));
        metrics.insert("evidence".to_string(), milli_units(self.score.evidence));
        metrics.insert("utility".to_string(), milli_units(self.score.utility));
        metrics.insert("exec_cost".to_string(), i64_from_u64(self.execution_cost));
        metrics.insert("mem_cost".to_string(), i64_from_u64(self.memory_cost));
        metrics.insert(
            "law_permille".to_string(),
            i64_from_u64(self.law_permille()),
        );
        WorldCandidate {
            world_fingerprint: self.world_id.0,
            provider_id: self.provider_provenance.clone(),
            evidence_authority: self.authority,
            labeled_authority: self.authority,
            metrics,
            artifact_hash: self.identity,
            guard_failure: self.disqualifications.first().map(|entry| GuardFailure {
                code: entry.code.clone(),
                detail: entry.detail.clone(),
            }),
        }
    }
}

/// Pre-selection guard failure; a failed guard never enters ranking or Pareto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardFailure {
    /// Stable machine-readable code, e.g. `hard-constraint:violated`.
    pub code: String,
    /// Human-readable detail.
    pub detail: String,
}

/// G7 interpretation candidate record: world fingerprint, provider, evidence
/// authority, integer metrics, artifact hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldCandidate {
    /// World content fingerprint (bit-exact tie-break key).
    pub world_fingerprint: u64,
    /// Meaning-provider identifier.
    pub provider_id: String,
    /// Authority supported by admitted evidence. Ranking never raises this.
    pub evidence_authority: Authority,
    /// Presented label. Must be `<= evidence_authority` or evaluate refuses.
    pub labeled_authority: Authority,
    /// Deterministic integer-scaled metrics, keyed by axis name.
    pub metrics: BTreeMap<String, i64>,
    /// Artifact content hash.
    pub artifact_hash: u64,
    /// Optional failed applicability guard.
    pub guard_failure: Option<GuardFailure>,
}

impl WorldCandidate {
    /// Builds a candidate whose presented label equals its evidence authority.
    #[must_use]
    pub fn new(
        world_fingerprint: u64,
        provider_id: impl Into<String>,
        evidence_authority: Authority,
        metrics: BTreeMap<String, i64>,
        artifact_hash: u64,
    ) -> Self {
        Self {
            world_fingerprint,
            provider_id: provider_id.into(),
            evidence_authority,
            labeled_authority: evidence_authority,
            metrics,
            artifact_hash,
            guard_failure: None,
        }
    }

    /// G7 view of a `keep: pareto N` bag member: uniform `cost=1` so
    /// domination cannot drop a kept world. Ranking of the genesis bag
    /// stays on [`crate::portfolio::InterpretationPortfolio::new`].
    #[must_use]
    pub fn bag_member(
        world_fingerprint: u64,
        provider_id: impl Into<String>,
        evidence_authority: crate::portfolio::Authority,
    ) -> Self {
        let mut metrics = BTreeMap::new();
        metrics.insert("cost".to_string(), 1);
        Self::new(
            world_fingerprint,
            provider_id,
            evidence_authority,
            metrics,
            world_fingerprint,
        )
    }

    /// Attempts to present `claimed` as the authority label.
    ///
    /// Refuses when `claimed` is strictly above [`Self::evidence_authority`].
    /// Ranking and selection never call this to raise a label.
    pub fn with_claimed_label(mut self, claimed: Authority) -> Result<Self, crate::portfolio::PortfolioError> {
        if claimed > self.evidence_authority {
            return Err(crate::portfolio::PortfolioError::AuthorityEscalation {
                fingerprint: self.world_fingerprint,
                evidence: self.evidence_authority,
                claimed,
            });
        }
        self.labeled_authority = claimed;
        Ok(self)
    }

    /// Deterministic canonical form (identity excluded).
    #[must_use]
    pub fn canonical(&self) -> String {
        let metrics = self
            .metrics
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(",");
        let guard = self.guard_failure.as_ref().map_or_else(
            || "-".to_string(),
            |failure| format!("{}:{}", failure.code, failure.detail),
        );
        format!(
            "world:fp={:016x}:provider={}:evidence={}:labeled={}:metrics={}:artifact={:016x}:guard={guard}",
            self.world_fingerprint,
            self.provider_id,
            self.evidence_authority.as_str(),
            self.labeled_authority.as_str(),
            metrics,
            self.artifact_hash,
        )
    }

    /// FNV-1a64 of [`Self::canonical`].
    #[must_use]
    pub fn identity(&self) -> u64 {
        fnv1a64(self.canonical().as_bytes())
    }
}

/// Converts a finite `f64` to milli-units. NaN maps to `i64::MIN`; infinities
/// saturate. Used only when projecting [`ScoreVector`] into G7 integer metrics.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub fn milli_units(value: f64) -> i64 {
    if value.is_nan() {
        return i64::MIN;
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            i64::MAX
        } else {
            i64::MIN
        };
    }
    let scaled = value * 1000.0;
    if scaled >= i64::MAX as f64 {
        i64::MAX
    } else if scaled <= i64::MIN as f64 {
        i64::MIN
    } else {
        scaled as i64
    }
}

#[allow(clippy::cast_possible_wrap)]
fn i64_from_u64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
