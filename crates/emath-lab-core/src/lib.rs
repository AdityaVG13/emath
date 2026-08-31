//! Laboratory core: manifests, quality gates, measurement, statistics and
//! promotion policy for the Phase 10 laboratory. Std-only and
//! deterministic; wall-clock timing enters only as injected samples.
//!
//! Key modules: `manifest` (frozen experiment), `gate` (correctness
//! before performance), `measure` (deterministic harness), `stats`
//! (paired protocol, MAD outliers), `promotion::decide` (typed reasons),
//! `selector`, `drift` (`E-HOST-010`), `receipt` (recomputable), and
//! `adversarial` (cheating/poison detectors).

#![forbid(unsafe_code)]

pub mod adversarial;
pub mod candidate;
pub mod drift;
pub mod error;
pub mod failure;
pub mod gate;
pub mod identity;
pub mod json;
pub mod manifest;
pub mod measure;
pub mod pilot;
pub mod promotion;
pub mod receipt;
pub mod selector;
pub mod sha256;
pub mod stats;
pub mod supervisor;

pub use adversarial::{AdversarialCheck, RunFacts, require_comparable, run_all};
pub use candidate::{Candidate, CandidateLoop, ParetoArchive, dominates};
pub use drift::{DriftAlert, DriftBand, DriftKind, DriftMonitor};
pub use error::LabError;
pub use failure::{
    FAILURE_BUNDLE_SCHEMA, FailureBundle, TRUE_DIVERGENCE_POINTER, true_divergence_bundle,
};
pub use gate::{GateCheck, GateCheckKind, GateVerdict, QualityGate};
pub use identity::{EngineIdentity, EngineRole};
pub use manifest::LabManifest;
pub use measure::{DerivedMetric, HarnessReport, Measurement, MeasurementKind, Summary};
pub use pilot::{CachePilot, ServeResult};
pub use promotion::{EnginePolicy, PromotionDecision, PromotionOutcome, PromotionReason, decide};
pub use receipt::DecisionReceipt;
pub use selector::{Route, Selector, Telemetry};
pub use sha256::{digest, hex};
pub use stats::{
    OutlierPolicy, PairedObservation, PairedResult, StatisticalProtocol, evaluate_paired,
};
pub use supervisor::{Supervisor, TickOutcome};
// Note: `supervisor::Observation` is not re-exported at the root: the
// legacy numeric `Observation` owns that name.

use emath_core::{ContentId, Span};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct ExperimentManifest {
    pub schema: String,
    pub experiment_id: ContentId,
    /// Baseline semantic package identity the experiment admits against.
    pub baseline: ContentId,
    /// Candidate package identity under evaluation.
    pub candidate: ContentId,
    pub admission_policy: AdmissionPolicy,
    pub promotion: PromotionPolicy,
    pub created_by: String,
    pub source: Option<Span>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdmissionPolicy {
    pub absolute_tolerance: f64,
    pub relative_tolerance: f64,
    pub sample_count: u64,
    /// Whether the candidate must be no more expensive than the baseline
    /// (operation count) on the sampled workload.
    pub require_non_increasing_operations: bool,
    pub seed: u64,
}

impl Default for AdmissionPolicy {
    fn default() -> Self {
        Self {
            absolute_tolerance: 1e-10,
            relative_tolerance: 1e-10,
            sample_count: 4_096,
            require_non_increasing_operations: true,
            seed: 0xE3A7_0F1D_C0FF_EE42,
        }
    }
}

/// One recorded evaluation: numeric comparison between a baseline and a
/// candidate implementation on a sampled input.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub metric: MetricDefinition,
    pub samples: u64,
    pub max_absolute_error: f64,
    pub max_relative_error: f64,
    pub median_ratio: f64,
    pub p99_ratio: f64,
    pub operations_candidate: u64,
    pub operations_baseline: u64,
    pub evidence: Option<ContentId>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MetricDefinition {
    pub id: String,
    pub kind: MetricKind,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MetricKind {
    Numeric,
    Performance,
    Robustness,
}

impl MetricKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Numeric => "numeric",
            Self::Performance => "performance",
            Self::Robustness => "robustness",
        }
    }
}

/// Deterministic order for observations in a report.
pub fn sort_observations(observations: &mut [Observation]) {
    observations.sort_by(|a, b| a.metric.id.cmp(&b.metric.id));
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidateAdmission {
    pub candidate: ContentId,
    pub status: AdmissionStatus,
    pub samples_attempted: u64,
    pub samples_compared: u64,
    pub max_absolute_error: f64,
    pub max_relative_error: f64,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionStatus {
    Admitted,
    Rejected,
    Pending,
}

impl AdmissionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Rejected => "rejected",
            Self::Pending => "pending",
        }
    }
}

/// Numeric and performance gates a candidate must satisfy to be promoted.
#[derive(Clone, Debug, PartialEq)]
pub struct PromotionPolicy {
    pub maximum_absolute_error: f64,
    pub maximum_relative_error: f64,
    pub minimum_median_speedup: f64,
    pub maximum_p99_regression: f64,
}

impl Default for PromotionPolicy {
    fn default() -> Self {
        Self {
            maximum_absolute_error: 1e-10,
            maximum_relative_error: 1e-10,
            minimum_median_speedup: 1.02,
            maximum_p99_regression: 1.05,
        }
    }
}

impl PromotionPolicy {
    #[must_use]
    pub fn passes_numeric_gates(&self, observation: &Observation) -> bool {
        observation.max_absolute_error <= self.maximum_absolute_error
            && observation.max_relative_error <= self.maximum_relative_error
    }

    #[must_use]
    pub fn passes_performance_gates(&self, observation: &Observation) -> bool {
        observation.median_ratio >= self.minimum_median_speedup
            && observation.p99_ratio <= self.maximum_p99_regression
    }
}

/// Deterministic pseudo-random samples (seed fixed, runs reproducible).
pub struct Sampler {
    state: u64,
}

impl Sampler {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next f64 in `[-1, 1)` via a splitmix64-style mix; the high 53
    /// bits are f64-exact.
    #[allow(clippy::cast_precision_loss)]
    pub fn next_unit(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        ((z >> 11) as f64 / (1_u64 << 53) as f64) * 2.0 - 1.0
    }
}

/// Name → identity index for experiment bookkeeping.
pub type ExperimentIndex = BTreeMap<String, ContentId>;

pub mod calibration;

pub mod law_check;

pub mod holes;

pub mod search;
