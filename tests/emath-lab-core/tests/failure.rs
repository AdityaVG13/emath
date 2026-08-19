//! Failure bundle tests (origin `crates/emath-lab-core/src/failure.rs`).

use emath_lab_core::{
    DriftAlert, DriftBand, DriftKind, DriftMonitor, EngineIdentity, FAILURE_BUNDLE_SCHEMA,
    TRUE_DIVERGENCE_POINTER, true_divergence_bundle,
};

fn monitor_with_fired_alert() -> (DriftMonitor, DriftAlert) {
    let mut monitor = DriftMonitor::new(vec![DriftBand {
        kind: DriftKind::Latency,
        metric_id: "p99".to_string(),
        relative_tolerance: 0.10,
    }])
    .expect("band valid");
    let fired = monitor.observe(DriftKind::Latency, "p99", 150.0, 100.0);
    assert!(!fired.is_empty(), "fixture must fire an alert");
    (monitor, fired[0].clone())
}

#[test]
fn bundle_emitted_with_true_divergence_pointer() {
    let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
    let oracle = EngineIdentity::oracle("emath-spec-oracle");
    let (_, alert) = monitor_with_fired_alert();
    let bundle = true_divergence_bundle(&subject, &oracle, &[alert]);

    assert_eq!(bundle.schema, FAILURE_BUNDLE_SCHEMA);
    assert_eq!(bundle.outcome, "true-divergence");
    assert_eq!(bundle.jsonptr, TRUE_DIVERGENCE_POINTER);
    assert_ne!(
        bundle.jsonptr, "/failure/test_failed",
        "a divergence failure is not a unit-test failure"
    );
    assert_eq!(bundle.subject.token(), "subject:emath-HEAD-a1401c0");
    assert_eq!(bundle.oracle.token(), "oracle:emath-spec-oracle");
    assert_eq!(bundle.drifted_metrics.len(), 1);
    assert_eq!(bundle.drifted_metrics[0].0, "p99");
    assert!(bundle.drifted_metrics[0].1.contains("E-HOST-010"));

    let doc = bundle.to_json();
    assert!(doc.contains("\"bundle_id\""));
    assert!(doc.contains("\"schema\":\"emath.failure-bundle\""));
    assert!(doc.contains("\"jsonptr\":\"/failure/true-divergence\""));
}

#[test]
fn bundle_identity_is_deterministic_and_binds_to_identities() {
    let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
    let oracle = EngineIdentity::oracle("emath-spec-oracle");
    let (_, alert) = monitor_with_fired_alert();
    let one = true_divergence_bundle(&subject, &oracle, std::slice::from_ref(&alert));
    let two = true_divergence_bundle(&subject, &oracle, &[alert]);
    assert_eq!(one.bundle_id, two.bundle_id, "deterministic emission");

    let other_oracle = EngineIdentity::oracle("emath-spec-oracle-alt");
    let three = true_divergence_bundle(&subject, &other_oracle, &[]);
    // Different oracle identity must not produce the same bundle for
    // the same alerts; identity separation is part of the document.
    let mut alerts_doc = one.to_json();
    alerts_doc.push_str(&three.to_json());
    assert_ne!(one.bundle_id, three.bundle_id);
    assert!(alerts_doc.contains("spec-oracle-alt"));
}

#[test]
fn monitor_emits_bundle_only_after_true_divergence() {
    let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
    let oracle = EngineIdentity::oracle("emath-spec-oracle");
    let mut monitor = DriftMonitor::new(vec![DriftBand {
        kind: DriftKind::Latency,
        metric_id: "p99".to_string(),
        relative_tolerance: 0.10,
    }])
    .expect("band valid");
    assert!(monitor.failure_bundle(&subject, &oracle).is_none());
    monitor.observe(DriftKind::Latency, "p99", 150.0, 100.0);
    let bundle = monitor
        .failure_bundle(&subject, &oracle)
        .expect("bundle after divergence");
    assert_eq!(bundle.outcome, "true-divergence");
    assert_eq!(bundle.drifted_metrics.len(), 1);
}
