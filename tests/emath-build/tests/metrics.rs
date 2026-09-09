//! Metrics receipt format and accumulation.
use emath_build::MetricsCollector;
use emath_test_harness::Probe;

#[test]
fn probe() {
    let mut p = Probe::new("metrics receipts stay byte-stable sorted and accumulate re-entered phases");
    p.case("stable", |p| {
        let mut c = MetricsCollector::new();
        c.record_duration_ns("check_plan", 1200);
        c.record_duration_ns("artifact_pipeline", 3400);
        c.record_count("plan_count", 2);
        c.record_count("artifact_bytes", 999);
        let (first, second) = (c.benchmark_receipt("spec.emath", "fnv1a64:abc"), c.benchmark_receipt("spec.emath", "fnv1a64:abc"));
        p.eq("stable", first.clone(), second);
        p.contains("schema", &first, "\"schema\": \"emath.benchmark-receipt\"");
        p.contains("version", &first, "\"version\": 1");
        p.contains("dur", &first, "\"duration_ns.check_plan\": 1200");
        p.contains("count", &first, "\"count.plan_count\": 2");
        p.demand("sorted", first.find("duration_ns.artifact_pipeline").unwrap() < first.find("duration_ns.check_plan").unwrap(), "keys must be sorted");
    });
    p.case("accumulate", |p| {
        let mut c = MetricsCollector::new();
        c.record_duration_ns("check_plan", 10);
        c.record_duration_ns("check_plan", 5);
        c.record_count("semantic_rejected", 1);
        c.record_count("semantic_rejected", 2);
        let r = c.benchmark_receipt("s", "a");
        p.contains("dur-sum", &r, "\"duration_ns.check_plan\": 15");
        p.contains("count-sum", &r, "\"count.semantic_rejected\": 3");
    });
    p.finish();
}
