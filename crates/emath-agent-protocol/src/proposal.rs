//! Submission schema for agent-native meaning proposals (spec 18).

use emath_term::SymbolId;
use emath_tuning::{ExecutionDelta, JointCandidate, WorldDelta};
use emath_world_ir::{fnv1a64, translation::EvidenceHandle, WorldId};

/// Proposal-scoped authorities granted by schema admission alone.
pub const PROPOSAL_AUTHORITIES: [&str; 2] = ["propose", "revise"];

/// Authorities that are never granted to a proposal at the loop.
pub const EXECUTION_AUTHORITIES: [&str; 2] = ["execute-code", "deploy"];

/// What an agent proposal claims to modify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProposalKind {
    /// Parse hypothesis over a corpus.
    ParseHypothesis,
    /// Signature inference/refinement.
    Signature,
    /// Carrier/domain choice.
    Carrier,
    /// Operator meaning.
    OperatorMeaning,
    /// Law shape.
    Law,
    /// Constructor rule.
    Constructor,
    /// World delta.
    WorldDelta,
    /// Selection policy.
    SelectionPolicy,
    /// Implementation plan.
    ImplementationPlan,
}

impl ProposalKind {
    /// Deterministic canonical name.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::ParseHypothesis => "parse-hypothesis",
            Self::Signature => "signature",
            Self::Carrier => "carrier",
            Self::OperatorMeaning => "operator-meaning",
            Self::Law => "law",
            Self::Constructor => "constructor",
            Self::WorldDelta => "world-delta",
            Self::SelectionPolicy => "selection-policy",
            Self::ImplementationPlan => "implementation-plan",
        }
    }
}

/// An agent-native meaning proposal; this is the submission envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProposal {
    /// Problem identifier.
    pub problem_id: String,
    /// Proposal kind.
    pub kind: ProposalKind,
    /// Base world identities.
    pub base_worlds: Vec<WorldId>,
    /// Hole identifiers the proposal claims to address.
    pub holes: Vec<String>,
    /// Proposed world changes.
    pub world_delta: WorldDelta,
    /// Proposed implementation changes, when implementing a meaning.
    pub execution_delta: Option<ExecutionDelta>,
    /// Claimed preservation obligations, as evidence handles.
    pub claimed_obligations: Vec<EvidenceHandle>,
    /// Supporting derivation or example text.
    pub derivation: String,
    /// Providers the proposal requires.
    pub required_providers: Vec<String>,
    /// Estimated cost in host units.
    pub estimated_cost: u64,
    /// Evidence units backing the proposal.
    pub evidence_units: u32,
    /// Requested authorities. Only `PROPOSAL_AUTHORITIES` are admitted;
    /// anything in `EXECUTION_AUTHORITIES` is refused at admission.
    pub requested_authority: Vec<String>,
    /// Proposing agent identity.
    pub agent_id: String,
    /// Content identity of the proposal.
    pub identity: u64,
}

impl AgentProposal {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        let obligations = self
            .claimed_obligations
            .iter()
            .map(EvidenceHandle::canonical)
            .collect::<Vec<_>>()
            .join(";");
        let authorities = self.requested_authority.join(",");
        format!(
            "proposal:problem={}:kind={}:base={}:holes={}:world={}:exec={}:obligations={}:derivation={}:providers={}:cost={}:evidence={}:authority=[{}]:agent={}",
            self.problem_id,
            self.kind.canonical(),
            self.base_worlds
                .iter()
                .map(|world| world.0.to_string())
                .collect::<Vec<_>>()
                .join(","),
            self.holes.join(","),
            self.world_delta.canonical(),
            self.execution_delta
                .as_ref()
                .map_or_else(String::new, ExecutionDelta::canonical),
            obligations,
            self.derivation,
            self.required_providers.join(","),
            self.estimated_cost,
            self.evidence_units,
            authorities,
            self.agent_id,
        )
    }

    /// Builds a proposal with a deterministic content identity.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        problem_id: impl Into<String>,
        kind: ProposalKind,
        base_worlds: Vec<WorldId>,
        holes: Vec<String>,
        world_delta: WorldDelta,
        execution_delta: Option<ExecutionDelta>,
        claimed_obligations: Vec<EvidenceHandle>,
        derivation: impl Into<String>,
        required_providers: Vec<String>,
        estimated_cost: u64,
        evidence_units: u32,
        requested_authority: Vec<String>,
        agent_id: impl Into<String>,
    ) -> Self {
        let proposal = Self {
            problem_id: problem_id.into(),
            kind,
            base_worlds,
            holes,
            world_delta,
            execution_delta,
            claimed_obligations,
            derivation: derivation.into(),
            required_providers,
            estimated_cost,
            evidence_units,
            requested_authority,
            agent_id: agent_id.into(),
            identity: 0,
        };
        let identity = fnv1a64(proposal.canonical().as_bytes());
        Self {
            identity,
            ..proposal
        }
    }

    /// Symbols the proposal's changes touch.
    #[must_use]
    pub fn touched_symbols(&self) -> Vec<SymbolId> {
        let mut symbols = self
            .world_delta
            .changes
            .iter()
            .filter_map(|change| change.symbol.clone())
            .collect::<Vec<_>>();
        symbols.sort_by(|left, right| left.0.cmp(&right.0));
        symbols.dedup();
        symbols
    }

    /// Whether the proposal requests any execution-scoped authority.
    #[must_use]
    pub fn requests_execution(&self) -> bool {
        self.requested_authority
            .iter()
            .any(|authority| EXECUTION_AUTHORITIES.contains(&authority.as_str()))
    }

    /// Whether the proposal requests only proposal-scoped authorities.
    #[must_use]
    pub fn requests_only_proposal_authority(&self) -> bool {
        !self.requests_execution()
            && self
                .requested_authority
                .iter()
                .all(|authority| PROPOSAL_AUTHORITIES.contains(&authority.as_str()))
    }

    /// Bridge to the joint-tuning surface.
    #[must_use]
    pub fn to_joint_candidate(
        &self,
        label: &str,
        evidence_units: u32,
        held_out_verified: bool,
    ) -> JointCandidate {
        let execution = self.execution_delta.clone().unwrap_or(ExecutionDelta {
            lowering: "none".to_string(),
            precision: "none".to_string(),
            provider: "none".to_string(),
            target: "none".to_string(),
            schedule: "none".to_string(),
        });
        JointCandidate::new(
            label,
            self.world_delta.clone(),
            execution,
            held_out_verified,
            evidence_units,
        )
    }
}
