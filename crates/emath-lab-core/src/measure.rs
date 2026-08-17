//! Measurement harness.
//!
//! Latency, throughput, startup, allocations, peak/retained memory, binary
//! size, energy, device cost and application metrics. The harness itself is
//! deterministic and std-only: wall-clock timing is injected as raw samples
//! (the pilot runtime supplies them), and every summary/derived value is a
//! pure function of those samples. Derived energy/cost values come from an
//! explicit, frozen model.

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

/// Keep-gate quarantine threshold: a cell with coefficient of variation
/// above 5% is too noisy to support any comparison claim and is
/// quarantined instead of being read as an honest baseline.
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
    /// Whether the cell is too noisy for comparison claims
    /// (`cv_pct > QUARANTINE_CV_PCT`; a zero mean has no relative
    /// spread and is never quarantined).
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

    /// Runs the deterministic summary; empty samples are `E-HOST-006`.
    pub fn summarize(&self) -> Result<Summary, LabError> {
        if self.samples.is_empty() {
            return Err(LabError::new(
                "E-HOST-006",
                format!("no raw samples for metric {}", self.metric_id),
            ));
        }
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        let mean_value = mean(&sorted);
        let cv_pct = Self::coefficient_of_variation_pct(&sorted, mean_value);
        Ok(Summary {
            count: sorted.len() as u64,
            min: *sorted.first().expect("non-empty checked above"),
            max: *sorted.last().expect("non-empty checked above"),
            mean: mean_value,
            median: percentile(&sorted, 0.5).expect("non-empty checked above"),
            p90: percentile(&sorted, 0.90).expect("non-empty checked above"),
            p99: percentile(&sorted, 0.99).expect("non-empty checked above"),
            cv_pct,
        })
    }

    /// Population coefficient of variation in percent. A zero mean has no
    /// relative spread (`0.0`); exactness is not claimed for the f64
    /// spread ratio.
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

    /// Throughput from a frozen operation count and total elapsed
    /// nanoseconds (`ops/s`). Ratios are inherently approximate.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn throughput_ops_per_sec(operations: u64, elapsed_ns: u64) -> f64 {
        let operations = operations as f64;
        let elapsed = elapsed_ns as f64;
        if elapsed == 0.0 {
            f64::INFINITY
        } else {
            operations * 1e9 / elapsed
        }
    }
}

/// Frozen energy/cost model (deterministic, explicit).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnergyCostModel {
    /// Joules per operation on the measured path.
    pub joules_per_operation: f64,
    /// Active power draw in watts.
    pub watts_active: f64,
    /// Device cost per kilowatt-hour in USD.
    pub usd_per_kwh: f64,
}

impl Default for EnergyCostModel {
    fn default() -> Self {
        Self {
            joules_per_operation: 0.3e-9,
            watts_active: 12.0,
            usd_per_kwh: 0.15,
        }
    }
}

impl EnergyCostModel {
    /// Energy for `operations` executed over `active_seconds`. Counts are
    /// model inputs, so approximation at extreme magnitudes is acceptable.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn energy_joules(&self, operations: u64, active_seconds: f64) -> f64 {
        self.joules_per_operation * operations as f64 + self.watts_active * active_seconds
    }

    /// Device cost for an energy quantity.
    #[must_use]
    pub fn device_cost_usd(&self, joules: f64) -> f64 {
        joules / 3.6e6 * self.usd_per_kwh
    }
}

/// Derived deterministic metric.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedMetric {
    /// Metric id.
    pub id: String,
    /// Value (unit depends on the metric).
    pub value: f64,
    /// Unit token.
    pub unit: String,
}

/// Deterministic harness report over both artifacts.
#[derive(Clone, Debug, PartialEq)]
pub struct HarnessReport {
    /// Baseline measurements (sorted by metric id).
    pub baseline: Vec<Measurement>,
    /// Candidate measurements (sorted by metric id).
    pub candidate: Vec<Measurement>,
    /// Derived metrics (sorted by id).
    pub derived: Vec<DerivedMetric>,
    /// Whether raw samples were retained for receipts.
    pub raw_retained: bool,
}

impl HarnessReport {
    /// Sorts measurements/derived deterministically by id.
    pub fn sort(&mut self) {
        self.baseline
            .sort_by(|left, right| left.metric_id.cmp(&right.metric_id));
        self.candidate
            .sort_by(|left, right| left.metric_id.cmp(&right.metric_id));
        self.derived.sort_by(|left, right| left.id.cmp(&right.id));
    }

    /// Peak-memory ratio candidate/baseline from peak-memory metrics.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // byte counts -> ratio; exact below 2^53
    pub fn peak_memory_ratio(&self) -> Option<f64> {
        let baseline = self
            .baseline
            .iter()
            .find(|measurement| measurement.kind == MeasurementKind::PeakMemoryBytes)?;
        let candidate = self
            .candidate
            .iter()
            .find(|measurement| measurement.kind == MeasurementKind::PeakMemoryBytes)?;
        let baseline_peak = *baseline.samples.iter().max()?;
        let candidate_peak = *candidate.samples.iter().max()?;
        if baseline_peak == 0 {
            None
        } else {
            Some(candidate_peak as f64 / baseline_peak as f64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Measurement, MeasurementKind, QUARANTINE_CV_PCT};

    fn summarized(samples: &[u64]) -> super::Summary {
        Measurement {
            metric_id: "test".into(),
            kind: MeasurementKind::LatencyNs,
            unit: "ns".into(),
            samples: samples.to_vec(),
        }
        .summarize()
        .expect("non-empty samples summarize")
    }

    /// Keep-gate `cv_pct`: a wide-spread cell (mean 110, sd ~8.16) crosses
    /// the 5% quarantine threshold; a tight cell stays eligible.
    #[allow(clippy::float_cmp)]
    #[test]
    fn cv_pct_quarantines_noisy_cells() {
        let noisy = summarized(&[100, 110, 120]);
        assert!(noisy.cv_pct > QUARANTINE_CV_PCT, "{}", noisy.cv_pct);
        assert!(noisy.quarantined(), "noisy cell must quarantine");
        let tight = summarized(&[100, 101, 99, 100, 102, 101]);
        assert!(tight.cv_pct < QUARANTINE_CV_PCT, "{}", tight.cv_pct);
        assert!(!tight.quarantined(), "tight cell must stay eligible");
    }

    /// Degenerate cases never quarantine and never divide by zero.
    #[allow(clippy::float_cmp)]
    #[test]
    fn degenerate_samples_have_zero_cv() {
        assert_eq!(summarized(&[42]).cv_pct, 0.0);
        assert!(!summarized(&[42]).quarantined());
        let zero_mean = summarized(&[0, 0, 0]);
        assert_eq!(zero_mean.cv_pct, 0.0);
        assert!(!zero_mean.quarantined());
    }
}
