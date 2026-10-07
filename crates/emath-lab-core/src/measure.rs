//! Measurement harness.
//!
//! Covers latency, throughput, memory, size, energy, cost and application
//! metrics. Deterministic and std-only: timing is injected as raw
//! samples; every summary is a pure function of them; energy/cost come
//! from an explicit frozen model.

use crate::error::LabError;
use crate::stats::{mean, percentile};

/// Kind of a measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MeasurementKind {
    /// Per-run latency in nanoseconds.
    LatencyNs,
    /// Throughput in operations per second.
    ThroughputOpsPerSec,
    /// Startup latency in nanoseconds.
    StartupNs,
    /// Allocation count.
    Allocations,
    /// Peak resident memory in bytes.
    PeakMemoryBytes,
    /// Retained memory in bytes.
    RetainedMemoryBytes,
    /// Artifact binary size in bytes.
    BinarySizeBytes,
    /// Energy in joules (from the frozen model).
    EnergyJoules,
    /// Device cost in USD (from the frozen model).
    DeviceCostUsd,
    /// Application-level counter (custom id).
    Application,
}

impl MeasurementKind {
    /// Stable kind token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LatencyNs => "latency",
            Self::ThroughputOpsPerSec => "throughput",
            Self::StartupNs => "startup",
            Self::Allocations => "allocations",
            Self::PeakMemoryBytes => "peak_memory",
            Self::RetainedMemoryBytes => "retained_memory",
            Self::BinarySizeBytes => "binary_size",
            Self::EnergyJoules => "energy",
            Self::DeviceCostUsd => "device_cost",
            Self::Application => "application",
        }
    }
}

/// Keep-gate quarantine threshold: CV above 5% is too noisy to support
/// any comparison claim.
pub const QUARANTINE_CV_PCT: f64 = 5.0;

/// Deterministic summary of raw samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    /// Sample count.
    pub count: u64,
    /// Minimum.
    pub min: u64,
    /// Maximum.
    pub max: u64,
    /// Arithmetic mean.
    pub mean: f64,
    /// Median.
    pub median: f64,
    /// 90th percentile.
    pub p90: f64,
    /// 99th percentile.
    pub p99: f64,
    /// Coefficient of variation in percent (`stddev / mean * 100`);
    /// cells above [`QUARANTINE_CV_PCT`] are quarantined.
    pub cv_pct: f64,
}

impl Summary {
    /// Too noisy for comparison (`cv_pct > QUARANTINE_CV_PCT`; zero
    /// mean is never quarantined).
    #[must_use]
    pub fn quarantined(&self) -> bool {
        self.mean > 0.0 && self.cv_pct > QUARANTINE_CV_PCT
    }
}

/// One measured quantity with raw samples.
#[derive(Clone, Debug, PartialEq)]
pub struct Measurement {
    /// Metric id from the manifest.
    pub metric_id: String,
    /// Measurement kind.
    pub kind: MeasurementKind,
    /// Unit token (`ns`, `ops/s`, `bytes`, `allocations`, ...).
    pub unit: String,
    /// Repetitions: raw samples in chronological (run) order.
    pub samples: Vec<u64>,
}

impl Measurement {
    /// One-sample measurement (startup, binary size, ...).
    #[must_use]
    pub fn single(
        metric_id: impl Into<String>,
        kind: MeasurementKind,
        unit: impl Into<String>,
        value: u64,
    ) -> Self {
        Self {
            metric_id: metric_id.into(),
            kind,
            unit: unit.into(),
            samples: vec![value],
        }
    }

    /// Deterministic summary; empty samples are `E-HOST-006`.
    pub fn summarize(&self) -> Result<Summary, LabError> {
        if self.samples.is_empty() {
            return Err(LabError::new(
                "E-HOST-006",
                format!("no raw samples for metric {}", self.metric_id),
            ));
        }
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        let mean_value = mean(&sorted)?;
        let cv_pct = Self::coefficient_of_variation_pct(&sorted, mean_value);
        let Some(min) = sorted.first().copied() else {
            return Err(LabError::new(
                "E-HOST-006",
                format!("no raw samples for metric {}", self.metric_id),
            ));
        };
        let Some(max) = sorted.last().copied() else {
            return Err(LabError::new(
                "E-HOST-006",
                format!("no raw samples for metric {}", self.metric_id),
            ));
        };
        Ok(Summary {
            count: sorted.len() as u64,
            min,
            max,
            mean: mean_value,
            median: percentile(&sorted, 0.5)?,
            p90: percentile(&sorted, 0.90)?,
            p99: percentile(&sorted, 0.99)?,
            cv_pct,
        })
    }

    /// Population CV in percent; zero mean yields `0.0`, f64-exactness
    /// of the spread ratio not claimed.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // ns counts -> f64 deviations; exact below 2^53
    pub fn coefficient_of_variation_pct(values: &[u64], mean_value: f64) -> f64 {
        if values.len() < 2 || mean_value == 0.0 {
            return 0.0;
        }
        let variance = values
            .iter()
            .map(|value| {
                let deviation = *value as f64 - mean_value;
                deviation * deviation
            })
            .sum::<f64>()
            / values.len() as f64;
        variance.sqrt() / mean_value * 100.0
    }
}
