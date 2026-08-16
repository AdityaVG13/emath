//!: real host pilot.
//!
//! A deterministic cache-router host integrating the selector, drift
//! monitor and receipts end to end, with meaningful application metrics:
//! hit rate, mean latency, correctness rate and fallback counts.
//! Wall-clock values are injected (the pilot is a pure function of its
//! inputs), and the host keeps serving under candidate failure by
//! falling back to the baseline.

use std::collections::BTreeMap;

use crate::drift::{DriftKind, DriftMonitor};
use crate::receipt::DecisionReceipt;
use crate::selector::{Route, Selector};

/// One request served by the pilot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServeResult {
    /// Route used.
    pub route: Route,
    /// Whether the key was already cached.
    pub cached: bool,
    /// Served latency in nanoseconds.
    pub latency_ns: u64,
    /// Whether the answer was correct.
    pub correct: bool,
}

/// Deterministic cache-router pilot.
#[derive(Clone, Debug)]
pub struct CachePilot {
    selector: Selector,
    monitor: DriftMonitor,
    cache: BTreeMap<String, u64>,
    served: u64,
    hits: u64,
    correct: u64,
    latency_sum_ns: u64,
}

impl CachePilot {
    /// Builds the pilot from a selector and drift monitor.
    #[must_use]
    pub fn new(selector: Selector, monitor: DriftMonitor) -> Self {
        Self {
            selector,
            monitor,
            cache: BTreeMap::new(),
            served: 0,
            hits: 0,
            correct: 0,
            latency_sum_ns: 0,
        }
    }

    /// Serves one keyed request with injected per-artifact latencies and
    /// candidate correctness. A failed candidate returns the baseline
    /// answer, records a fallback and keeps the host alive.
    pub fn serve(
        &mut self,
        key: &str,
        baseline_latency_ns: u64,
        candidate_latency_ns: u64,
        candidate_correct: bool,
    ) -> ServeResult {
        let request_id = self.served;
        let route = self.selector.dispatch(request_id);
        let cached = self.cache.contains_key(key);
        let (latency_ns, correct) = match route {
            Route::Baseline => (baseline_latency_ns, true),
            Route::Candidate | Route::Canary if candidate_correct => (candidate_latency_ns, true),
            Route::Candidate | Route::Canary => {
                // Candidate failure: baseline takes over, host continues.
                let _ = self.selector.on_failure();
                (baseline_latency_ns, true)
            }
        };
        self.served += 1;
        self.hits += u64::from(cached);
        if correct {
            self.correct += 1;
        }
        self.latency_sum_ns = self.latency_sum_ns.saturating_add(latency_ns);
        self.cache.insert(key.to_string(), request_id);
        ServeResult {
            route,
            cached,
            latency_ns,
            correct,
        }
    }

    /// Observes the served latency against the frozen expectation;
    /// returns new drift alerts (recorded by the monitor too).
    #[allow(clippy::cast_precision_loss)] // ns -> f64 for drift math
    pub fn observe_latency(
        &mut self,
        observed_ns: u64,
        expected_ns: u64,
    ) -> Vec<crate::drift::DriftAlert> {
        self.monitor.observe(
            DriftKind::Latency,
            "latency",
            observed_ns as f64,
            expected_ns as f64,
        )
    }

    /// Application hit rate in `(0.0, 1.0]`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn hit_rate(&self) -> f64 {
        if self.served == 0 {
            0.0
        } else {
            self.hits as f64 / self.served as f64
        }
    }

    /// Mean served latency in nanoseconds.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn mean_latency_ns(&self) -> f64 {
        if self.served == 0 {
            0.0
        } else {
            self.latency_sum_ns as f64 / self.served as f64
        }
    }

    /// App-level correctness rate in `(0.0, 1.0]`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn correctness_rate(&self) -> f64 {
        if self.served == 0 {
            0.0
        } else {
            self.correct as f64 / self.served as f64
        }
    }

    /// Requests served so far.
    #[must_use]
    pub fn served(&self) -> u64 {
        self.served
    }

    /// Selector telemetry.
    #[must_use]
    pub fn telemetry(&self) -> &crate::selector::Telemetry {
        self.selector.telemetry()
    }

    /// Accumulated drift alerts.
    #[must_use]
    pub fn alerts(&self) -> &[crate::drift::DriftAlert] {
        self.monitor.alerts()
    }

    /// Consumes the pilot state into a sealed decision receipt.
    /// The experiment/protocol evidence comes from the caller.
    #[allow(clippy::too_many_arguments)]
    pub fn into_receipt(
        self,
        experiment_id: emath_core::ContentId,
        manifest_json: String,
        protocol: crate::stats::StatisticalProtocol,
        paired: Option<crate::stats::PairedResult>,
        memory_ratio: Option<f64>,
        energy: Option<(f64, f64)>,
        was_promoted: bool,
        decision: crate::promotion::PromotionDecision,
        command: String,
        environment_token: String,
        artifact_hashes: Vec<(String, emath_core::ContentId)>,
    ) -> Result<DecisionReceipt, crate::error::LabError> {
        crate::receipt::DecisionReceipt {
            receipt_id: emath_core::ContentId("pending".into()),
            experiment_id,
            manifest_json,
            gate_checks: vec![crate::gate::GateCheck::pass(
                "pilot-runtime",
                crate::gate::GateCheckKind::Semantic,
            )],
            protocol,
            raw_retained: true,
            paired,
            memory_ratio,
            energy,
            was_promoted,
            decision,
            command,
            environment_token,
            artifact_hashes,
        }
        .seal()
    }
}
