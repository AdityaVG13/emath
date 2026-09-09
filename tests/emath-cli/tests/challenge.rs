//! Challenge-loop admission and frontier-handoff tests.
use emath_cli::portfolio::{Authority, InterpretationPortfolio};
use emath_cli_lab::agent_protocol::proposal::{AgentProposal, ProposalKind};
use emath_cli_lab::agent_protocol::{ChallengeLoop, ChallengeOutcome, CheckerSuite};
use emath_genesis::tuning::campaign::{CandidateMeasurement, HostCampaign, HostMetric, HostObjectives, ResourceEnvelope};
use emath_genesis::tuning::{SemanticChange, SemanticVariableKind, WorldDelta};
use emath_test_harness::Probe;
use emath_world_ir::WorldId;
use emath_world_ir::translation::EvidenceHandle;
fn change() -> SemanticChange { SemanticChange { kind: SemanticVariableKind::Operator, symbol: None, description: "nat-add".into(), provenance: "agent-proposal".into() } }
fn proposal(problem: &str, worlds: Vec<WorldId>, authority: &[&str]) -> AgentProposal { AgentProposal::new(problem, ProposalKind::WorldDelta, worlds, vec!["hole-add".into()], WorldDelta::new(WorldId(1), vec![change()]), None, vec![EvidenceHandle { id: 1, provenance: "seed".into(), scope: "obligation:exact".into() }], "1+1=2", Vec::new(), 1, 1, authority.iter().map(|s| s.to_string()).collect(), "test-agent") }
fn valid(authority: &[&str]) -> AgentProposal { proposal("g9-reference", vec![WorldId(1)], authority) }
fn empty_loop() -> ChallengeLoop { ChallengeLoop { evidence_threshold: 0, max_estimated_cost: u64::MAX, checker_suite: CheckerSuite::default(), counterexample_generator: None } }
#[test]
fn probe() {
    let mut p = Probe::new("challenge loop admits valid proposals, refuses authority and schema gaps, hands off only verified");
    p.case("authority", |p| { let q = valid(&["propose", "execute-code"]); match empty_loop().admit(&q) { Err(r) => { p.eq("code", r.code.clone(), "capability:authority-not-admitted".to_string()); p.eq("identity", r.proposal_identity, q.identity); p.eq("run", empty_loop().run(&q, &InterpretationPortfolio::default()), ChallengeOutcome::Refused(r)); } Ok(_) => { p.fail("refuse", "execution authority must be refused"); } } });
    p.case("schema", |p| { let q = proposal("g9-reference", vec![], &["propose"]); match empty_loop().admit(&q) { Err(r) => { p.eq("code", r.code.clone(), "schema:incomplete".to_string()); p.eq("identity", r.proposal_identity, q.identity); } Ok(_) => { p.fail("refuse", "missing base worlds must be refused"); } } });
    p.case("deterministic", |p| { let (a, b) = (valid(&["propose"]), valid(&["propose"])); p.eq("identity", a.identity, b.identity); let (x, y) = (empty_loop().run(&a, &InterpretationPortfolio::default()), empty_loop().run(&b, &InterpretationPortfolio::default())); let (ChallengeOutcome::WorldCandidate(x), ChallengeOutcome::WorldCandidate(y)) = (x, y) else { p.fail("candidate", "valid proposal must run to world candidate"); return; }; p.eq("proposal", x.proposal_identity, a.identity); p.eq("stable", x.identity, y.identity); p.eq("world", x.world_id, WorldId(1)); p.eq("authority", x.authority, Authority::Tested); p.demand("no-exec", !x.execution_granted(), "no execution grant"); p.eq("rank", x.rank, 0); });
    p.case("handoff", |p| {
        let q = valid(&["propose"]); p.demand("admitted", matches!(empty_loop().run(&q, &InterpretationPortfolio::default()), ChallengeOutcome::WorldCandidate(_)), "schema-valid proposal admitted");
        let campaign = HostCampaign { label: "agent-handoff".into(), preserved_symbols: vec![], evidence_threshold: 1, envelope: ResourceEnvelope { max_tokens: 1000, max_p95_latency_ms: 500, min_cache_hit_rate_permille: 900 }, objectives: HostObjectives { maximize: vec!["cache_hit_rate".into()], minimize: vec!["token_cost".into(), "p95_latency".into()] }, fallback_world: Some(WorldId(0xba5e)) };
        let metrics = vec![HostMetric { name: "cache_hit_rate".into(), value: 960 }, HostMetric { name: "token_cost".into(), value: 400 }, HostMetric { name: "p95_latency".into(), value: 120 }];
        let (rej, adm) = (q.to_joint_candidate("unverified", 0, false), q.to_joint_candidate("verified", 2, true));
        let receipt = campaign.run(&[rej.clone(), adm.clone()], &[CandidateMeasurement { candidate_identity: rej.identity, metrics: metrics.clone() }, CandidateMeasurement { candidate_identity: adm.identity, metrics }]);
        let dec = |id: u64| receipt.decisions.iter().find(|d| d.candidate_identity == id).expect("decision").clone();
        let (r, a) = (dec(rej.identity), dec(adm.identity));
        p.demand("refused", !r.promoted, "unverified refused"); p.contains("reason", &r.reason, "semantic-admission:held-out-failed"); p.demand("promoted", a.promoted, "verified promotes"); p.eq("selected", receipt.selected_identity, Some(adm.identity));
    });
    p.finish();
}
