use emath_tuning::campaign::{
    CandidateMeasurement, HostCampaign, HostMetric, HostObjectives, ResourceEnvelope,
};
use emath_tuning::frontier::{generate_algebraic_candidates, verify_held_out, RewriteRule};
use emath_tuning::ExecutionDelta;
use emath_term::SymbolId;
use emath_world_ir::WorldId;

fn execution() -> ExecutionDelta {
    ExecutionDelta {
        lowering: "table".to_string(),
        precision: "u64".to_string(),
        provider: "native".to_string(),
        target: "cpu".to_string(),
        schedule: "shadow-first".to_string(),
    }
}

fn rule(label: &str, replacement: &str) -> RewriteRule {
    RewriteRule {
        label: label.to_string(),
        symbol: SymbolId("evict".to_string()),
        replacement: replacement.to_string(),
    }
}

fn campaign() -> HostCampaign {
    HostCampaign {
        label: "cache-policy".to_string(),
        preserved_symbols: vec![SymbolId("pin".to_string())],
        evidence_threshold: 2,
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
    }
}

fn metrics(hit: u64, tokens: u64, latency: u64) -> Vec<HostMetric> {
    vec![
        HostMetric {
            name: "cache_hit_rate".to_string(),
            value: hit,
        },
        HostMetric {
            name: "token_cost".to_string(),
            value: tokens,
        },
        HostMetric {
            name: "p95_latency".to_string(),
            value: latency,
        },
    ]
}

#[test]
fn generation_is_deterministic_deduplicated_and_budget_capped() {
    let base = WorldId(7);
    let rules = vec![
        rule("b-swap", "evict(x) = batched(x)"),
        rule("a-batch", "evict(x) = lazy(x)"),
        rule("b-swap", "evict(x) = batched(x)"),
        rule("c-tail", "evict(x) = tail(x)"),
    ];
    let first = generate_algebraic_candidates(base, &rules, &execution(), 2);
    let second = generate_algebraic_candidates(base, &rules, &execution(), 2);
    assert_eq!(first, second, "generation must be a pure function");
    assert_eq!(first.len(), 2, "duplicate dropped, then budget cap of 2");
    assert_eq!(first[0].label, "a-batch", "sorted by canonical form");
    assert!(
        first
            .iter()
            .all(|c| !c.held_out_verified && c.evidence_units == 0),
        "generated candidates are always unverified with zero evidence"
    );
}

/// Frontier exit gate, end to end on a seeded cache-policy campaign:
/// a mathematically equivalent faster policy is verified, benchmarked,
/// and promoted with a receipt; a wrong policy fails the held-out
/// challenge and is rejected *before* any benchmark exists for it; a
/// correct but host-worse policy is rejected by the envelope; the
/// strict baseline stays available as fallback throughout.
#[test]
fn seeded_campaign_promotes_equivalent_and_rejects_wrong_before_benchmark() {
    let base = WorldId(0xcac4e);
    let rules = vec![
        rule("equivalent-batched", "evict(x) = batched(x)"),
        rule("wrong-evicts-pinned", "evict(x) = drop-pinned(x)"),
        rule("correct-but-slow", "evict(x) = scan-all(x)"),
    ];
    let generated = generate_algebraic_candidates(base, &rules, &execution(), 8);
    assert_eq!(generated.len(), 3);

    // Held-out challenge: the wrong rewrite disagrees with the
    // held-out references, the other two agree.
    let verified: Vec<_> = generated
        .iter()
        .map(|candidate| {
            verify_held_out(
                candidate,
                |change| !change.description.contains("drop-pinned"),
                3,
            )
        })
        .collect();
    let by_label = |label: &str| {
        verified
            .iter()
            .find(|candidate| candidate.label == label)
            .expect("candidate present")
    };
    assert!(!by_label("wrong-evicts-pinned").held_out_verified);

    // Benchmark protocol: only verified candidates are measured. The
    // wrong candidate is never benchmarked — there is no measurement
    // row for it at all.
    let measurements = vec![
        CandidateMeasurement {
            candidate_identity: by_label("equivalent-batched").identity,
            metrics: metrics(960, 400, 120),
        },
        CandidateMeasurement {
            candidate_identity: by_label("correct-but-slow").identity,
            metrics: metrics(960, 400, 4000),
        },
    ];

    let campaign = campaign();
    let receipt = campaign.run(&verified, &measurements);

    let decision_for = |label: &str| {
        let identity = by_label(label).identity;
        receipt
            .decisions
            .iter()
            .find(|decision| decision.candidate_identity == identity)
            .expect("decision present")
    };

    let promoted = decision_for("equivalent-batched");
    assert!(promoted.promoted, "equivalent faster policy promotes");
    assert_eq!(promoted.reason, "promoted");
    assert_eq!(
        receipt.selected_identity,
        Some(promoted.candidate_identity),
        "promotion is receipted with the selected identity"
    );

    let wrong = decision_for("wrong-evicts-pinned");
    assert!(!wrong.promoted);
    assert!(
        wrong.reason.contains("semantic-admission:held-out-failed"),
        "wrong candidate is refused at semantic admission: {}",
        wrong.reason
    );
    assert_eq!(wrong.score_permille, 0, "never scored against benchmarks");

    let slow = decision_for("correct-but-slow");
    assert!(!slow.promoted);
    assert!(
        slow.reason.contains("envelope:out-of-bounds"),
        "host-worse candidate is refused by the protection envelope: {}",
        slow.reason
    );

    // Baseline fallback preserved and receipt deterministic.
    assert_eq!(campaign.fallback_world, Some(WorldId(0xba5e)));
    let rerun = campaign.run(&verified, &measurements);
    assert_eq!(receipt.identity, rerun.identity, "receipt identity stable");
}

/// Without a strict baseline to deopt to, even a correct and
/// host-better candidate must not promote.
#[test]
fn promotion_requires_a_baseline_fallback() {
    let base = WorldId(0xcac4e);
    let generated = generate_algebraic_candidates(
        base,
        &[rule("equivalent-batched", "evict(x) = batched(x)")],
        &execution(),
        1,
    );
    let candidate = verify_held_out(&generated[0], |_| true, 3);
    let measurements = vec![CandidateMeasurement {
        candidate_identity: candidate.identity,
        metrics: metrics(960, 400, 120),
    }];
    let campaign = HostCampaign {
        fallback_world: None,
        ..campaign()
    };
    let receipt = campaign.run(&[candidate], &measurements);
    assert_eq!(receipt.selected_identity, None);
    assert!(receipt.decisions[0].reason.contains("fallback:unavailable"));
}
