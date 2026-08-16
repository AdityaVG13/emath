//!: automatic demotion.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drift::DriftBand;
    use crate::gate::{GateCheck, GateCheckKind, QualityGate};
    use crate::promotion::PromotionOutcome;
    use crate::selector::Selector;

    fn supervisor(outcome: PromotionOutcome) -> Supervisor {
        let gate = QualityGate::evaluate(vec![GateCheck::pass(
            "correctness",
            GateCheckKind::Correctness,
        )]);
        let canary_interval = if outcome == PromotionOutcome::Canary { 1 } else { 0 };
        let selector = Selector::new(gate, outcome, canary_interval).unwrap();
        let monitor = DriftMonitor::new(vec![DriftBand {
            kind: DriftKind::Latency,
            metric_id: "latency".into(),
            relative_tolerance: 0.1,
        }])
        .unwrap();
        Supervisor::new(selector, monitor)
    }

    fn observation(value: f64, expected: f64) -> Observation {
        Observation {
            kind: DriftKind::Latency,
            metric_id: "latency".into(),
            value,
            expected,
        }
    }

    #[test]
    fn drift_demotes_the_candidate_to_baseline() {
        let mut supervisor = supervisor(PromotionOutcome::Promote);
        let outcome = supervisor.tick(&[observation(150.0, 100.0)]);
        assert_eq!(outcome.outcome, PromotionOutcome::Demote);
        assert!(matches!(
            outcome.reason,
            Some(PromotionReason::DriftDetected { .. })
        ));
        assert_eq!(outcome.reason.as_ref().unwrap().code(), Some("E-HOST-010"));
        assert_eq!(supervisor.outcome(), PromotionOutcome::Retain);
        assert_eq!(supervisor.telemetry().deoptimizations, 1);
    }

    #[test]
    fn quiet_runs_leave_the_promotion_untouched() {
        let mut supervisor = supervisor(PromotionOutcome::Promote);
        let outcome = supervisor.tick(&[observation(102.0, 100.0)]);
        assert!(outcome.alerts.is_empty());
        assert_eq!(outcome.outcome, PromotionOutcome::Promote);
        assert_eq!(supervisor.outcome(), PromotionOutcome::Promote);
    }

    #[test]
    fn baseline_is_retained_on_drift() {
        let mut supervisor = supervisor(PromotionOutcome::Retain);
        let outcome = supervisor.tick(&[observation(999.0, 100.0)]);
        assert_eq!(outcome.outcome, PromotionOutcome::Retain);
        assert_eq!(supervisor.telemetry().deoptimizations, 0);
    }

    #[test]
    fn multiple_observations_aggregate_into_one_tick() {
        let mut supervisor = supervisor(PromotionOutcome::Canary);
        let outcome = supervisor.tick(&[observation(200.0, 100.0), observation(300.0, 100.0)]);
        assert_eq!(outcome.alerts.len(), 2);
        assert_eq!(outcome.outcome, PromotionOutcome::Demote);
    }
}
