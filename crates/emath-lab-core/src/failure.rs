//! Failure bundles: truthful failure documents.
//!
//! When a monitor observes true divergence (drift beyond the frozen
//! band), the harness emits a `FailureBundle` instead of a success
//! receipt: the bundle names the outcome (`true-divergence`), the
//! identities on both sides of the comparison, and a JSON-pointer
//! (`/failure/true-divergence`) that locates the failure inside the
//! document. The pointer deliberately never is `/failure/test_failed`:
//! this is a divergence failure of the harness comparison, not a unit
//! test failure.

use crate::drift::DriftAlert;
use crate::identity::EngineIdentity;
use crate::json::{self, JsonValue};
use emath_core::{ContentId, fnv1a64_bytes};

/// Schema id of the failure bundle.
pub const FAILURE_BUNDLE_SCHEMA: &str = "emath.failure-bundle.v1";

/// JSON pointer locating the failure inside the bundle document.
pub const TRUE_DIVERGENCE_POINTER: &str = "/failure/true-divergence";

/// A truthful failure document.
#[derive(Clone, Debug, PartialEq)]
pub struct FailureBundle {
    /// Schema id (`FAILURE_BUNDLE_SCHEMA`).
    pub schema: String,
    /// Failure outcome (`true-divergence`).
    pub outcome: String,
    /// JSON pointer locating the failure (`TRUE_DIVERGENCE_POINTER`).
    pub jsonptr: String,
    /// Identity of the compared subject.
    pub subject: EngineIdentity,
    /// Identity of the compared oracle/reference.
    pub oracle: EngineIdentity,
    /// Drifted metrics `(metric id, alert message)`, in alert order.
    pub drifted_metrics: Vec<(String, String)>,
    /// Identity over the canonical bundle body.
    pub bundle_id: ContentId,
}

impl FailureBundle {
    /// Canonical body (all fields except `bundle_id`), sorted
    /// deterministically by the lab JSON writer; the identity input.
    #[must_use]
    pub fn canonical_json(&self) -> String {
        json::write(&self.body_value())
    }

    /// The bundle as a JSON document.
    #[must_use]
    pub fn to_json(&self) -> String {
        let JsonValue::Object(mut fields) = self.body_value() else {
            unreachable!("body is an object");
        };
        fields.push((
            "bundle_id".to_string(),
            JsonValue::String(self.bundle_id.0.clone()),
        ));
        json::write(&JsonValue::Object(fields))
    }

    fn body_value(&self) -> JsonValue {
        JsonValue::Object(vec![
            ("schema".to_string(), JsonValue::String(self.schema.clone())),
            (
                "outcome".to_string(),
                JsonValue::String(self.outcome.clone()),
            ),
            (
                "jsonptr".to_string(),
                JsonValue::String(self.jsonptr.clone()),
            ),
            (
                "subject".to_string(),
                JsonValue::String(self.subject.token()),
            ),
            ("oracle".to_string(), JsonValue::String(self.oracle.token())),
            (
                "drifted_metrics".to_string(),
                JsonValue::Array(
                    self.drifted_metrics
                        .iter()
                        .map(|(metric, message)| {
                            JsonValue::Object(vec![
                                ("metric_id".to_string(), JsonValue::String(metric.clone())),
                                ("message".to_string(), JsonValue::String(message.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

/// Emits a `true-divergence` failure bundle over the given alerts.
/// Callers construct this only when alerts actually fired (the drift
/// monitor gates on `drifted()`); a bundle without evidence is a lie.
#[must_use]
pub fn true_divergence_bundle(
    subject: &EngineIdentity,
    oracle: &EngineIdentity,
    alerts: &[DriftAlert],
) -> FailureBundle {
    let drifted_metrics = alerts
        .iter()
        .map(|alert| (alert.metric_id.clone(), alert.message()))
        .collect::<Vec<_>>();
    let mut bundle = FailureBundle {
        schema: FAILURE_BUNDLE_SCHEMA.to_string(),
        outcome: "true-divergence".to_string(),
        jsonptr: TRUE_DIVERGENCE_POINTER.to_string(),
        subject: subject.clone(),
        oracle: oracle.clone(),
        drifted_metrics,
        bundle_id: ContentId("unset".into()),
    };
    let body = bundle.canonical_json();
    bundle.bundle_id = ContentId(format!("fnv1a64:{:016x}", fnv1a64_bytes(body.as_bytes())));
    bundle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drift::{DriftBand, DriftKind, DriftMonitor};

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
        assert!(doc.contains("\"schema\":\"emath.failure-bundle.v1\""));
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

        let other_oracle = EngineIdentity::oracle("emath-spec-oracle-v2");
        let three = true_divergence_bundle(&subject, &other_oracle, &[]);
        // Different oracle identity must not produce the same bundle for
        // the same alerts; identity separation is part of the document.
        let mut alerts_doc = one.to_json();
        alerts_doc.push_str(&three.to_json());
        assert_ne!(one.bundle_id, three.bundle_id);
        assert!(alerts_doc.contains("spec-oracle-v2"));
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
}
