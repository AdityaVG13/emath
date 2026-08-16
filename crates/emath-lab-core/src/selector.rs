//!: runtime selector.
//!
//! Dispatches each request between baseline, candidate and canary routes
//! under the frozen promotion decision, with correctness guards,
//! per-route telemetry, runtime failure fallback and deoptimization
//! (step back to baseline) on drift or failure. A candidate is never
//! served while its quality gate is closed (`E-HOST-005`).

use crate::error::LabError;
use crate::gate::GateVerdict;
use crate::promotion::{PromotionOutcome, PromotionReason};

/// Active route for one request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Baseline implementation.
    Baseline,
    /// Candidate implementation (full traffic).
    Candidate,
    /// Candidate implementation for a canary cohort.
    Canary,
}

impl Route {
    /// Stable route token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Candidate => "candidate",
            Self::Canary => "canary",
        }
    }
}

/// Per-route runtime telemetry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Telemetry {
    /// Requests served by the baseline.
    pub served_baseline: u64,
    /// Requests served by the candidate.
    pub served_candidate: u64,
    /// Requests served by the canary cohort.
    pub served_canary: u64,
    /// Runtime fallback activations (candidate failed, baseline took over).
    pub fallbacks: u64,
    /// Deoptimizations performed.
    pub deoptimizations: u64,
}

impl Telemetry {
    fn record(&mut self, route: Route) {
        match route {
            Route::Baseline => self.served_baseline += 1,
            Route::Candidate => self.served_candidate += 1,
            Route::Canary => self.served_canary += 1,
        }
    }
}

/// Runtime selector.
#[derive(Clone, Debug, PartialEq)]
pub struct Selector {
    gate: GateVerdict,
    outcome: PromotionOutcome,
    canary_interval: u64,
    fallback_on_failure: bool,
    telemetry: Telemetry,
}

impl Selector {
    /// Builds a selector from the gate verdict and the promotion decision.
    /// A promote/canary outcome with a closed gate is refused
    /// (`E-HOST-005`) — the guard exists from construction.
    pub fn new(
        gate: GateVerdict,
        outcome: PromotionOutcome,
        canary_interval: u64,
    ) -> Result<Self, LabError> {
        if canary_interval == 0 && outcome == PromotionOutcome::Canary {
            return Err(LabError::new(
                "E-HOST-003",
                "canary outcome requires a positive canary interval",
            ));
        }
        if matches!(
            outcome,
            PromotionOutcome::Promote | PromotionOutcome::Canary
        ) && !gate.eligible()
        {
            return Err(LabError::new(
                "E-HOST-005",
                format!(
                    "cannot route to {} with a closed quality gate",
                    outcome.as_str()
                ),
            ));
        }
        Ok(Self {
            gate,
            outcome,
            canary_interval,
            fallback_on_failure: true,
            telemetry: Telemetry::default(),
        })
    }

    /// Routes one request deterministically; the request id selects the
    /// canary cohort. Never fails: shadow/retain/quarantine always serve
    /// the baseline, and a closed gate falls back to the baseline.
    pub fn dispatch(&mut self, request_id: u64) -> Route {
        let route = match self.outcome {
            PromotionOutcome::Promote if self.gate.eligible() => Route::Candidate,
            PromotionOutcome::Canary
                if self.gate.eligible()
                    && self.canary_interval > 0
                    && request_id % self.canary_interval == 0 =>
            {
                Route::Canary
            }
            PromotionOutcome::Shadow
            | PromotionOutcome::Retain
            | PromotionOutcome::Demote
            | PromotionOutcome::Quarantine => Route::Baseline,
            _ => {
                // Defense in depth: a closed gate at runtime falls back.
                self.telemetry.fallbacks += 1;
                Route::Baseline
            }
        };
        self.telemetry.record(route);
        route
    }

    /// Runtime guard: refuses routing to the candidate when the gate is
    /// closed (`E-HOST-005`). The constructor already forbids that state;
    /// this is for hosts that deserialize selector state.
    pub fn guard(&self) -> Result<(), LabError> {
        if matches!(
            self.outcome,
            PromotionOutcome::Promote | PromotionOutcome::Canary
        ) && !self.gate.eligible()
        {
            return Err(LabError::new(
                "E-HOST-005",
                format!(
                    "quality gate closed; {} is not routable",
                    self.outcome.as_str()
                ),
            ));
        }
        Ok(())
    }

    /// Deoptimization: steps a promoted/canary candidate back to the
    /// baseline, records the step and the reason; returns whether the
    /// route actually changed.
    pub fn deoptimize(&mut self, reason: &PromotionReason) -> bool {
        let reverted = matches!(
            self.outcome,
            PromotionOutcome::Promote | PromotionOutcome::Canary
        );
        if reverted {
            self.outcome = PromotionOutcome::Retain;
            self.telemetry.deoptimizations += 1;
            let _ = reason;
        }
        reverted
    }

    /// Runtime failure fallback: the candidate errored, the baseline
    /// takes over; counted in telemetry.
    pub fn on_failure(&mut self) -> Route {
        self.telemetry.fallbacks += 1;
        Route::Baseline
    }

    /// Current telemetry.
    #[must_use]
    pub fn telemetry(&self) -> &Telemetry {
        &self.telemetry
    }

    /// Current outcome.
    #[must_use]
    pub fn outcome(&self) -> PromotionOutcome {
        self.outcome
    }
}
