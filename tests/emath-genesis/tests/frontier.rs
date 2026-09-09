//! Host-campaign frontier exit-gate tests.

use emath_genesis::tuning::ExecutionDelta;
use emath_genesis::tuning::campaign::{CandidateMeasurement, HostCampaign, HostMetric, HostObjectives, ResourceEnvelope};
use emath_genesis::tuning::frontier::{RewriteRule, generate_algebraic_candidates, verify_held_out};
use emath_term::SymbolId;
use emath_world_ir::WorldId;
use emath_test_harness::Probe;

fn execution() -> ExecutionDelta {
    ExecutionDelta { lowering: "table".to_string(), precision: "u64".to_string(), provider: "native".to_string(), target: "cpu".to_string(), schedule: "shadow-first".to_string() }
}

fn rule(label: &str, replacement: &str) -> RewriteRule {
    RewriteRule { label: label.to_string(), symbol: SymbolId("evict".to_string()), replacement: replacement.to_string() }
}

fn campaign() -> HostCampaign {
    HostCampaign { label: "cache-policy".to_string(), preserved_symbols: vec![SymbolId("pin".to_string())], evidence_threshold: 2, envelope: ResourceEnvelope { max_tokens: 1000, max_p95_latency_ms: 500, min_cache_hit_rate_permille: 900 }, objectives: HostObjectives { maximize: vec!["cache_hit_rate".to_string()], minimize: vec!["token_cost".to_string(), "p95_latency".to_string()] }, fallback_world: Some(WorldId(0xba5e)) }
}

fn metrics(hit: u64, tokens: u64, latency: u64) -> Vec<HostMetric> {
    vec![HostMetric { name: "cache_hit_rate".to_string(), value: hit }, HostMetric { name: "token_cost".to_string(), value: tokens }, HostMetric { name: "p95_latency".to_string(), value: latency }]
}

#[test]
fn campaign_frontier() {
    let mut p = Probe::new("the frontier promotes equivalent policies and rejects the rest");
    p.case("generation", |p| {
        let base = WorldId(7);
        let rules = vec![rule("b-swap", "evict(x) = batched(x)"), rule("a-batch", "evict(x) = lazy(x)"), rule("b-swap", "evict(x) = batched(x)"), rule("c-tail", "evict(x) = tail(x)")];
        let first = generate_algebraic_candidates(base, &rules, &execution(), 2);
        p.eq("deterministic", first.clone(), generate_algebraic_candidates(base, &rules, &execution(), 2));
        p.eq("capped", first.len(), 2);
        p.eq("sorted", first[0].label.clone(), "a-batch".to_string());
        p.demand("unverified", first.iter().all(|c| !c.held_out_verified && c.evidence_units == 0), "generated candidates start unverified");
    });
    p.case("seeded-campaign", |p| {
        let base = WorldId(0xcac4e);
        let rules = vec![rule("equivalent-batched", "evict(x) = batched(x)"), rule("wrong-evicts-pinned", "evict(x) = drop-pinned(x)"), rule("correct-but-slow", "evict(x) = scan-all(x)")];
        let generated = generate_algebraic_candidates(base, &rules, &execution(), 8);
        p.eq("count", generated.len(), 3);
        let verified: Vec<_> = generated.iter().map(|c| verify_held_out(c, |change| !change.description.contains("drop-pinned"), 3)).collect();
        let by_label = |label: &str| verified.iter().find(|c| c.label == label).expect("candidate present");
        p.demand("wrong-fails", !by_label("wrong-evicts-pinned").held_out_verified, "wrong rewrite fails held-out");
        let measurements = vec![CandidateMeasurement { candidate_identity: by_label("equivalent-batched").identity, metrics: metrics(960, 400, 120) }, CandidateMeasurement { candidate_identity: by_label("correct-but-slow").identity, metrics: metrics(960, 400, 4000) }];
        let campaign = campaign();
        let receipt = campaign.run(&verified, &measurements);
        let decision_for = |label: &str| { let id = by_label(label).identity; receipt.decisions.iter().find(|d| d.candidate_identity == id).expect("decision present") };
        let promoted = decision_for("equivalent-batched");
        p.demand("promotes", promoted.promoted, "equivalent faster policy promotes");
        p.eq("reason", promoted.reason.clone(), "promoted".to_string());
        p.eq("selected", receipt.selected_identity, Some(promoted.candidate_identity));
        let wrong = decision_for("wrong-evicts-pinned");
        p.demand("wrong-refused", !wrong.promoted, "wrong candidate refused");
        p.contains("held-out", &wrong.reason, "semantic-admission:held-out-failed");
        p.eq("unscored", wrong.score_permille, 0);
        let slow = decision_for("correct-but-slow");
        p.demand("slow-refused", !slow.promoted, "host-worse refused");
        p.contains("envelope", &slow.reason, "envelope:out-of-bounds");
        p.eq("fallback", campaign.fallback_world, Some(WorldId(0xba5e)));
        p.eq("receipt-stable", receipt.identity, campaign.run(&verified, &measurements).identity);
    });
    p.case("needs-baseline", |p| {
        let generated = generate_algebraic_candidates(WorldId(0xcac4e), &[rule("equivalent-batched", "evict(x) = batched(x)")], &execution(), 1);
        let candidate = verify_held_out(&generated[0], |_| true, 3);
        let measurements = vec![CandidateMeasurement { candidate_identity: candidate.identity, metrics: metrics(960, 400, 120) }];
        let receipt = HostCampaign { fallback_world: None, ..campaign() }.run(&[candidate], &measurements);
        p.eq("selected", receipt.selected_identity, None);
        p.contains("fallback", &receipt.decisions[0].reason, "fallback:unavailable");
    });
    p.finish();
}
