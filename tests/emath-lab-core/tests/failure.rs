//! True-divergence failure-bundle tests.

use emath_lab_core::{DriftAlert, DriftBand, DriftKind, DriftMonitor, EngineIdentity, FAILURE_BUNDLE_SCHEMA, TRUE_DIVERGENCE_POINTER, true_divergence_bundle};
use emath_test_harness::Probe;

fn fired_alert(p: &mut Probe) -> (DriftMonitor, DriftAlert) {
    let mut monitor = DriftMonitor::new(vec![DriftBand { kind: DriftKind::Latency, metric_id: "p99".to_string(), relative_tolerance: 0.10 }]).expect("band valid");
    let fired = monitor.observe(DriftKind::Latency, "p99", 150.0, 100.0);
    p.demand("\"fixture must fire an alert\"", !fired.is_empty(), "fixture must fire an alert");
    (monitor, fired[0].clone())
}

#[test]
fn failure_bundle() {
    let mut p = Probe::new("true divergence emits a deterministic bundle, never a test failure");
    p.case("emitted", |p| {
        let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
        let oracle = EngineIdentity::oracle("emath-spec-oracle");
        let (_, alert) = fired_alert(p, );
        let bundle = true_divergence_bundle(&subject, &oracle, &[alert]);
        p.eq("schema", bundle.schema, FAILURE_BUNDLE_SCHEMA);
        p.eq("outcome", bundle.outcome, "true-divergence");
        p.eq("pointer", bundle.jsonptr, TRUE_DIVERGENCE_POINTER);
        p.ne("not-unit", bundle.jsonptr, "/failure/test_failed");
        p.eq("subject", bundle.subject.token(), "subject:emath-HEAD-a1401c0");
        p.eq("oracle", bundle.oracle.token(), "oracle:emath-spec-oracle");
        p.eq("metrics", bundle.drifted_metrics.len(), 1);
        p.eq("metric", bundle.drifted_metrics[0].0.clone(), "p99");
        p.contains("code", &bundle.drifted_metrics[0].1, "E-HOST-010");
        let doc = bundle.to_json();
        for needle in ["\"bundle_id\"", "\"schema\":\"emath.failure-bundle\"", "\"jsonptr\":\"/failure/true-divergence\""] {
            p.contains(needle, &doc, needle);
        }
    });
    p.case("identity-deterministic", |p| {
        let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
        let oracle = EngineIdentity::oracle("emath-spec-oracle");
        let (_, alert) = fired_alert(p, );
        let one = true_divergence_bundle(&subject, &oracle, std::slice::from_ref(&alert));
        let two = true_divergence_bundle(&subject, &oracle, &[alert]);
        p.eq("stable", one.bundle_id, two.bundle_id);
        let three = true_divergence_bundle(&subject, &EngineIdentity::oracle("emath-spec-oracle-alt"), &[]);
        p.ne("oracle-binds", one.bundle_id, three.bundle_id);
        p.contains("alt", &(one.to_json() + &three.to_json()), "spec-oracle-alt");
    });
    p.case("only-after-divergence", |p| {
        let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
        let oracle = EngineIdentity::oracle("emath-spec-oracle");
        let mut monitor = DriftMonitor::new(vec![DriftBand { kind: DriftKind::Latency, metric_id: "p99".to_string(), relative_tolerance: 0.10 }]).expect("band valid");
        p.demand("quiet", monitor.failure_bundle(&subject, &oracle).is_none(), "no bundle before divergence");
        monitor.observe(DriftKind::Latency, "p99", 150.0, 100.0);
        let bundle = monitor.failure_bundle(&subject, &oracle).expect("bundle after divergence");
        p.eq("outcome", bundle.outcome, "true-divergence");
        p.eq("metrics", bundle.drifted_metrics.len(), 1);
    });
    p.finish();
}
