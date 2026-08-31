use emath_genesis::tuning::campaign::{HostMetric, ResourceEnvelope};

fn envelope() -> ResourceEnvelope {
    ResourceEnvelope {
        max_tokens: 1000,
        max_p95_latency_ms: 500,
        min_cache_hit_rate_permille: 950,
    }
}

#[test]
fn admits_fails_closed_on_missing_cost_or_latency() {
    let envelope = envelope();
    // `token_cost` omitted: never in-bounds by absence.
    let without_tokens = vec![
        HostMetric {
            name: "p95_latency".into(),
            value: 10,
        },
        HostMetric {
            name: "cache_hit_rate".into(),
            value: 980,
        },
    ];
    assert!(!envelope.admits(&without_tokens));
    // `p95_latency` omitted: same.
    let without_latency = vec![
        HostMetric {
            name: "token_cost".into(),
            value: 10,
        },
        HostMetric {
            name: "cache_hit_rate".into(),
            value: 980,
        },
    ];
    assert!(!envelope.admits(&without_latency));
    // `cache_hit_rate` omission was already fail-closed; pinned here
    // so the three bound metrics share one rule.
    let without_hit = vec![
        HostMetric {
            name: "token_cost".into(),
            value: 10,
        },
        HostMetric {
            name: "p95_latency".into(),
            value: 10,
        },
    ];
    assert!(!envelope.admits(&without_hit));
}

#[test]
fn admits_at_the_bounds_with_all_three_measured() {
    let envelope = envelope();
    let metrics = vec![
        HostMetric {
            name: "token_cost".into(),
            value: envelope.max_tokens,
        },
        HostMetric {
            name: "p95_latency".into(),
            value: envelope.max_p95_latency_ms,
        },
        HostMetric {
            name: "cache_hit_rate".into(),
            value: envelope.min_cache_hit_rate_permille,
        },
    ];
    assert!(envelope.admits(&metrics), "measured at the bounds admits");
}
