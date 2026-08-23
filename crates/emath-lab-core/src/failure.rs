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
pub const FAILURE_BUNDLE_SCHEMA: &str = "emath.failure-bundle";

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
        let mut fields = self.body_fields();
        fields.push((
            "bundle_id".to_string(),
            JsonValue::String(self.bundle_id.0.clone()),
        ));
        json::write(&JsonValue::Object(fields))
    }

    fn body_value(&self) -> JsonValue {
        JsonValue::Object(self.body_fields())
    }

    fn body_fields(&self) -> Vec<(String, JsonValue)> {
        vec![
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
        ]
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
