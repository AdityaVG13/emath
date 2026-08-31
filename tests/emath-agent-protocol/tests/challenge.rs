//! Challenge-loop admission and frontier-handoff tests.
//!
//! Moved from #[cfg(test)] in crates/emath-agent-protocol/src/challenge.rs.

use emath_agent_protocol::proposal::{AgentProposal, ProposalKind};
use emath_agent_protocol::{ChallengeLoop, ChallengeOutcome, CheckerSuite};
use emath_portfolio::{Authority, InterpretationPortfolio};
use emath_genesis::tuning::campaign::{
    CandidateMeasurement, HostCampaign, HostMetric, HostObjectives, ResourceEnvelope,
};
use emath_genesis::tuning::{SemanticChange, SemanticVariableKind, WorldDelta};
use emath_world_ir::translation::EvidenceHandle;
use emath_world_ir::WorldId;

fn change() -> SemanticChange {
    SemanticChange {
        kind: SemanticVariableKind::Operator,
        symbol: None,
        description: "nat-add".to_string(),
        provenance: "agent-proposal".to_string(),
    }
}

fn obligation() -> EvidenceHandle {
    EvidenceHandle {
        id: 1,
        provenance: "seed".to_string(),
        scope: "obligation:exact".to_string(),
    }
}

fn proposal(
    problem_id: &str,
    worlds: Vec<WorldId>,
    changes: Vec<SemanticChange>,
    obligations: Vec<EvidenceHandle>,
    authority: &[&str],
) -> AgentProposal {
    AgentProposal::new(
        problem_id,
        ProposalKind::WorldDelta,
        worlds,
        vec!["hole-add".to_string()],
        WorldDelta::new(WorldId(1), changes),
        None,
        obligations,
        "1+1=2",
        Vec::new(),
        1,
        1,
        authority.iter().map(|item| (*item).to_string()).collect(),
        "test-agent",
    )
}

fn valid_proposal(authority: &[&str]) -> AgentProposal {
    proposal(
        "g9-reference",
        vec![WorldId(1)],
        vec![change()],
        vec![obligation()],
        authority,
    )
}

fn empty_loop() -> ChallengeLoop {
    ChallengeLoop {
        evidence_threshold: 0,
        max_estimated_cost: u64::MAX,
        checker_suite: CheckerSuite::default(),
        counterexample_generator: None,
    }
}

#[test]
fn admit_refuses_execution_authority() {
    let proposal = valid_proposal(&["propose", "execute-code"]);
    let refusal = empty_loop()
        .admit(&proposal)
        .expect_err("execution authority must be refused");
    assert_eq!(refusal.code, "capability:authority-not-admitted");
    assert_eq!(refusal.proposal_identity, proposal.identity);
    assert_eq!(
        empty_loop().run(&proposal, &InterpretationPortfolio::default()),
        ChallengeOutcome::Refused(refusal)
    );
}

#[test]
fn admit_refuses_incomplete_schema() {
    let proposal = proposal(
        "g9-reference",
        Vec::new(),
        vec![change()],
        vec![obligation()],
        &["propose"],
    );
    let refusal = empty_loop()
        .admit(&proposal)
        .expect_err("missing base worlds must be refused");
    assert_eq!(refusal.code, "schema:incomplete");
    assert_eq!(refusal.proposal_identity, proposal.identity);
}

#[test]
fn valid_proposal_runs_to_deterministic_world_candidate() {
    let first_proposal = valid_proposal(&["propose"]);
    let second_proposal = valid_proposal(&["propose"]);
    assert_eq!(first_proposal.identity, second_proposal.identity);

    let portfolio = InterpretationPortfolio::default();
    let first = empty_loop().run(&first_proposal, &portfolio);
    let second = empty_loop().run(&second_proposal, &portfolio);
    let ChallengeOutcome::WorldCandidate(first) = first else {
        panic!("expected world candidate, got {first:?}");
    };
    let ChallengeOutcome::WorldCandidate(second) = second else {
        panic!("expected world candidate, got {second:?}");
    };
    assert_eq!(first.proposal_identity, first_proposal.identity);
    assert_eq!(first.identity, second.identity);
    assert_eq!(first.world_id, WorldId(1));
    assert_eq!(first.authority, Authority::Tested);
    assert!(!first.execution_granted());
    assert_eq!(first.rank, 0);
}

/// Agent-proposal → frontier handoff: the challenge loop admits a
/// valid proposal, then `to_joint_candidate` is the only path into
/// campaign admission. An unverified candidate is refused at
/// semantic admission; a held-out-verified candidate promotes.
#[test]
fn admitted_proposal_enters_frontier_only_when_held_out_verified() {
    let proposal = valid_proposal(&["propose"]);
    assert!(
        matches!(
            empty_loop().run(&proposal, &InterpretationPortfolio::default()),
            ChallengeOutcome::WorldCandidate(_)
        ),
        "schema-valid proposal must be admitted by the challenge loop"
    );

    let campaign = HostCampaign {
        label: "agent-handoff".to_string(),
        preserved_symbols: Vec::new(),
        evidence_threshold: 1,
        envelope: ResourceEnvelope {
            max_tokens: 1000,
            max_p95_latency_ms: 500,
            min_cache_hit_rate_permille: 900,
        },
        objectives: HostObjectives {
            maximize: vec!["cache_hit_rate".to_string()],
            minimize: vec!["token_cost".to_string(), "p95_latency".to_string()],
        },
        fallback_world: Some(WorldId(0xba5e)),
    };
    let metrics = vec![
        HostMetric {
            name: "cache_hit_rate".to_string(),
            value: 960,
        },
        HostMetric {
            name: "token_cost".to_string(),
            value: 400,
        },
        HostMetric {
            name: "p95_latency".to_string(),
            value: 120,
        },
    ];
    let rejected = proposal.to_joint_candidate("unverified", 0, false);
    let admitted = proposal.to_joint_candidate("verified", 2, true);
    let measurements = vec![
        CandidateMeasurement {
            candidate_identity: rejected.identity,
            metrics: metrics.clone(),
        },
        CandidateMeasurement {
            candidate_identity: admitted.identity,
            metrics,
        },
    ];
    let receipt = campaign.run(&[rejected.clone(), admitted.clone()], &measurements);
    let decision_for = |identity: u64| {
        receipt
            .decisions
            .iter()
            .find(|decision| decision.candidate_identity == identity)
            .expect("decision present")
    };

    let refused = decision_for(rejected.identity);
    assert!(!refused.promoted);
    assert!(
        refused
            .reason
            .contains("semantic-admission:held-out-failed"),
        "unverified proposal must be refused at frontier semantic admission: {}",
        refused.reason
    );

    let promoted = decision_for(admitted.identity);
    assert!(promoted.promoted, "held-out-verified proposal must promote");
    assert_eq!(receipt.selected_identity, Some(admitted.identity));
}
