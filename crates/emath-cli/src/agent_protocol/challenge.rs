//! Challenge loop for agent-native meaning proposals (spec 18).
//!
//! The loop is deterministic: schema admission, capability admission,
//! a deterministic checker suite, counterexample generation, evidence and
//! cost gates, then portfolio ranking. A proposal either produces a
//! revision request or a world candidate; it never grants execution
//! authority and never receives `Certified`/`Proved` authority from the
//! loop itself (those require external compiler, capability, evidence,
//! and benchmark gates).

use crate::portfolio::{Authority, InterpretationCandidate, InterpretationPortfolio, ScoreVector};
use emath_world_ir::{WorldId, fnv1a64};

use crate::agent_protocol::proposal::AgentProposal;

/// One deterministic check over a proposal.
#[derive(Debug, Clone, Copy)]
pub struct NamedCheck {
    /// Stable check name, surfaced in feedback.
    pub name: &'static str,
    /// Returns `Err` with the smallest counterexample (or unmet-evidence)
    /// text when the check fails.
    pub run: fn(&AgentProposal) -> Result<(), String>,
}

/// A deterministic checker suite, run in order.
#[derive(Debug, Clone, Copy, Default)]
pub struct CheckerSuite {
    /// Checks to run.
    pub checks: &'static [NamedCheck],
}

/// Stable admission refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionRefusal {
    /// Stable machine-readable code, e.g. `schema:missing-problem-id`.
    pub code: String,
    /// Human-readable detail.
    pub detail: String,
    /// Identity of the refused proposal.
    pub proposal_identity: u64,
}

/// Structured feedback the agent receives (spec 18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFeedback {
    /// Holes the proposal solved.
    pub solved_holes: Vec<String>,
    /// Constraints that failed.
    pub failed_constraints: Vec<String>,
    /// Smallest counterexample, when one was found.
    pub smallest_counterexample: Option<String>,
    /// Unmet evidence requirement, when the proposal was too weak.
    pub unmet_evidence: Option<String>,
    /// Cost regression in host units, when the proposal was too expensive.
    pub cost_regression: Option<u64>,
    /// Portfolio dominance note: which candidate outranks the proposal.
    pub portfolio_dominance: Option<String>,
}

impl AgentFeedback {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "feedback:solved={}:failed={}:cx={}:evidence={}:cost={}:dominated={}",
            self.solved_holes.join(","),
            self.failed_constraints.join(","),
            self.smallest_counterexample.as_deref().unwrap_or(""),
            self.unmet_evidence.as_deref().unwrap_or(""),
            self.cost_regression
                .map_or_else(String::new, |cost| cost.to_string()),
            self.portfolio_dominance.as_deref().unwrap_or(""),
        )
    }
}

/// A revision request sent back to the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionRequest {
    /// Identity of the proposal being revised.
    pub proposal_identity: u64,
    /// Structured feedback.
    pub feedback: AgentFeedback,
}

impl RevisionRequest {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "revision:{}:{}",
            self.proposal_identity,
            self.feedback.canonical()
        )
    }
}

/// A world candidate that survived the loop, with its portfolio rank.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldCandidateRef {
    /// Identity of the admitted proposal.
    pub proposal_identity: u64,
    /// Candidate world identity.
    pub world_id: WorldId,
    /// Authority earned in the loop: at most `Tested`. The loop never
    /// grants `Certified` or `Proved`.
    pub authority: Authority,
    /// Position in the ranked portfolio (0 = top).
    pub rank: usize,
    /// Structured feedback.
    pub feedback: AgentFeedback,
    /// Deterministic content identity of the candidate ref.
    pub identity: u64,
}

impl WorldCandidateRef {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "candidate:{}:world={}:authority={:?}:rank={}:{}",
            self.proposal_identity,
            self.world_id.0,
            self.authority,
            self.rank,
            self.feedback.canonical()
        )
    }

    /// Whether the candidate carries any execution authority. Always
    /// false: proposals cannot bypass capability admission.
    #[must_use]
    pub fn execution_granted(&self) -> bool {
        false
    }
}

/// Challenge outcome for one proposal.
#[derive(Debug, Clone, PartialEq)]
pub enum ChallengeOutcome {
    /// Refused by schema or capability admission.
    Refused(AdmissionRefusal),
    /// Revision requested with structured feedback.
    RevisionRequested(RevisionRequest),
    /// The proposal became a world candidate.
    WorldCandidate(WorldCandidateRef),
}

/// The deterministic challenge loop.
#[derive(Debug, Clone, Copy)]
pub struct ChallengeLoop {
    /// Minimum evidence units a surviving proposal must carry.
    pub evidence_threshold: u32,
    /// Maximum estimated cost a surviving proposal may carry.
    pub max_estimated_cost: u64,
    /// Deterministic checker suite.
    pub checker_suite: CheckerSuite,
    /// Optional host counterexample generator (None disables the step).
    pub counterexample_generator: Option<fn(&AgentProposal) -> Option<String>>,
}

impl ChallengeLoop {
    /// Schema + capability admission, in spec order.
    pub fn admit(&self, proposal: &AgentProposal) -> Result<(), AdmissionRefusal> {
        let schema_ok = !proposal.problem_id.trim().is_empty()
            && !proposal.base_worlds.is_empty()
            && !proposal.world_delta.changes.is_empty()
            && !proposal.claimed_obligations.is_empty();
        if !schema_ok {
            return Err(AdmissionRefusal {
                code: "schema:incomplete".to_string(),
                detail: "problem_id, base worlds, changes, and claimed obligations are required"
                    .to_string(),
                proposal_identity: proposal.identity,
            });
        }
        if proposal.execution_delta.is_some() && proposal.required_providers.is_empty() {
            return Err(AdmissionRefusal {
                code: "schema:providers-required".to_string(),
                detail: "implementation proposals require providers".to_string(),
                proposal_identity: proposal.identity,
            });
        }
        if !proposal.requests_only_proposal_authority() {
            return Err(AdmissionRefusal {
                code: "capability:authority-not-admitted".to_string(),
                detail: "requested authority is outside the proposal-scoped set".to_string(),
                proposal_identity: proposal.identity,
            });
        }
        Ok(())
    }

    /// Runs the challenge loop over one proposal; the survivor is ranked
    /// deterministically against the existing `portfolio`.
    #[must_use]
    pub fn run(
        &self,
        proposal: &AgentProposal,
        portfolio: &InterpretationPortfolio,
    ) -> ChallengeOutcome {
        if let Err(refusal) = self.admit(proposal) {
            return ChallengeOutcome::Refused(refusal);
        }

        let mut feedback = AgentFeedback {
            solved_holes: Vec::new(),
            failed_constraints: Vec::new(),
            smallest_counterexample: None,
            unmet_evidence: None,
            cost_regression: None,
            portfolio_dominance: None,
        };

        let mut checks_passed = true;
        for check in self.checker_suite.checks {
            if let Err(counterexample) = (check.run)(proposal) {
                checks_passed = false;
                feedback.failed_constraints.push(check.name.to_string());
                if feedback.smallest_counterexample.is_none() {
                    feedback.smallest_counterexample = Some(counterexample);
                }
            }
        }
        if !checks_passed {
            return Self::revision(proposal, feedback);
        }

        if let Some(generator) = self.counterexample_generator {
            if let Some(counterexample) = generator(proposal) {
                feedback.smallest_counterexample = Some(counterexample);
                return Self::revision(proposal, feedback);
            }
        }

        if proposal.evidence_units < self.evidence_threshold {
            feedback.unmet_evidence = Some(format!(
                "evidence:{}<{}",
                proposal.evidence_units, self.evidence_threshold
            ));
            return Self::revision(proposal, feedback);
        }

        if proposal.estimated_cost > self.max_estimated_cost {
            feedback.cost_regression = Some(proposal.estimated_cost);
            return Self::revision(proposal, feedback);
        }

        let (entry, rank) = Self::rank(proposal, portfolio);
        feedback.solved_holes.clone_from(&proposal.holes);
        if rank > 0 {
            let leader = portfolio.candidates().first().map_or_else(
                || "baseline".to_string(),
                |candidate| candidate.name.clone(),
            );
            feedback.portfolio_dominance = Some(format!("outranked-by:{leader}"));
        }
        let canonical = format!(
            "candidate:{}:world={}:authority={:?}:rank={}:{}",
            proposal.identity,
            entry.world_id.0,
            Authority::Tested,
            rank,
            feedback.canonical()
        );
        ChallengeOutcome::WorldCandidate(WorldCandidateRef {
            proposal_identity: proposal.identity,
            world_id: entry.world_id,
            authority: Authority::Tested,
            rank,
            feedback,
            identity: fnv1a64(canonical.as_bytes()),
        })
    }

    fn revision(proposal: &AgentProposal, feedback: AgentFeedback) -> ChallengeOutcome {
        ChallengeOutcome::RevisionRequested(RevisionRequest {
            proposal_identity: proposal.identity,
            feedback,
        })
    }

    /// Builds the proposal's portfolio entry and its rank in the merged
    /// portfolio. The entry's authority is capped at `Tested`.
    #[allow(clippy::cast_precision_loss)]
    fn rank(
        proposal: &AgentProposal,
        portfolio: &InterpretationPortfolio,
    ) -> (InterpretationCandidate, usize) {
        let world_id = proposal.base_worlds.first().copied().unwrap_or(WorldId(0));
        let name = format!("proposal-{:x}", proposal.identity);
        let entry = InterpretationCandidate {
            world_id,
            name: name.clone(),
            answer: proposal.world_delta.canonical(),
            authority: Authority::Tested,
            score: ScoreVector {
                cost: proposal.estimated_cost as f64,
                complexity: proposal.claimed_obligations.len() as f64,
                evidence: f64::from(proposal.evidence_units),
                utility: 0.0,
            },
            provenance: "agent-proposal".to_string(),
        };
        let mut all = portfolio.candidates().to_vec();
        all.push(entry.clone());
        let ranked = InterpretationPortfolio::new(all);
        let rank = ranked
            .candidates()
            .iter()
            .position(|candidate| candidate.name == name)
            .unwrap_or(0);
        (entry, rank)
    }
}
