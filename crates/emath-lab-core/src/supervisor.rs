//! Automatic demotion.
//!
//! The supervisor watches monitored observations through the drift
//! monitor; on any drift alert a promoted/canary candidate is demoted to
//! the baseline (typed `E-HOST-010` reason), otherwise the incumbent
//! stays. Rollback under injected drift is demonstrated by the tests.

use crate::drift::{DriftAlert, DriftKind, DriftMonitor};
use crate::promotion::{PromotionOutcome, PromotionReason};
use crate::selector::Selector;

/// One monitored observation from the host.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// Drift dimension.
    pub kind: DriftKind,
    /// Metric id.
    pub metric_id: String,
    /// Observed value.
    pub value: f64,
    /// Frozen expectation.
    pub expected: f64,
}

/// Outcome of a supervisor tick.
#[derive(Clone, Debug, PartialEq)]
pub struct TickOutcome {
    /// Alerts that fired during the tick.
    pub alerts: Vec<DriftAlert>,
    /// Resulting promotion outcome.
    pub outcome: PromotionOutcome,
    /// Reason for the outcome (`DriftDetected` when demoted).
    pub reason: Option<PromotionReason>,
}

/// Automatic demotion supervisor.
#[derive(Clone, Debug)]
pub struct Supervisor {
    selector: Selector,
    monitor: DriftMonitor,
}

impl Supervisor {
    /// Builds a supervisor over a selector and drift monitor.
    #[must_use]
    pub fn new(selector: Selector, monitor: DriftMonitor) -> Self {
        Self { selector, monitor }
    }

    /// Runs one tick: any matching drift alert demotes the candidate to
    /// baseline; otherwise the outcome stays as the selector's.
    pub fn tick(&mut self, observations: &[Observation]) -> TickOutcome {
        let mut alerts = Vec::new();
        for observation in observations {
            alerts.extend(self.monitor.observe(
                observation.kind,
                &observation.metric_id,
                observation.value,
                observation.expected,
            ));
        }
        if alerts.is_empty() {
            return TickOutcome {
                alerts,
                outcome: self.selector.outcome(),
                reason: None,
            };
        }
        let kind = alerts[0].kind;
        let metric = alerts[0].metric_id.clone();
        let reason = PromotionReason::DriftDetected { kind, metric };
        let reverted = self.selector.deoptimize(&reason);
        TickOutcome {
            alerts,
            outcome: if reverted {
                PromotionOutcome::Demote
            } else {
                PromotionOutcome::Retain
            },
            reason: Some(reason),
        }
    }

    /// Current selector outcome.
    #[must_use]
    pub fn outcome(&self) -> PromotionOutcome {
        self.selector.outcome()
    }

    /// Selector telemetry (deoptimization counter).
    #[must_use]
    pub fn telemetry(&self) -> &crate::selector::Telemetry {
        self.selector.telemetry()
    }
}
