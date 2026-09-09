//! Campaign resource-envelope admission tests.

use emath_genesis::tuning::campaign::{HostMetric, ResourceEnvelope};
use emath_test_harness::Probe;

fn envelope() -> ResourceEnvelope {
    ResourceEnvelope { max_tokens: 1000, max_p95_latency_ms: 500, min_cache_hit_rate_permille: 950 }
}

fn metrics(tokens: u64, latency: u64, hit: u64) -> Vec<HostMetric> {
    vec![HostMetric { name: "token_cost".into(), value: tokens }, HostMetric { name: "p95_latency".into(), value: latency }, HostMetric { name: "cache_hit_rate".into(), value: hit }]
}

#[test]
fn campaign_envelope() {
    let mut p = Probe::new("the envelope admits at the bounds and fails closed on gaps");
    let envelope = envelope();
    p.case("missing-metric-refused", |p| {
        p.demand("no-tokens", !envelope.admits(&metrics(0, 10, 980)[1..].to_vec()), "absent token_cost never admits");
        p.demand("no-latency", !envelope.admits(&vec![metrics(10, 0, 980)[0].clone(), metrics(10, 0, 980)[2].clone()]), "absent p95_latency never admits");
        p.demand("no-hit", !envelope.admits(&metrics(10, 10, 0)[..2].to_vec()), "absent cache_hit_rate never admits");
    });
    p.case("at-bounds-admits", |p| {
        p.demand("bounds", envelope.admits(&metrics(envelope.max_tokens, envelope.max_p95_latency_ms, envelope.min_cache_hit_rate_permille)), "measured at the bounds admits");
    });
    p.finish();
}
