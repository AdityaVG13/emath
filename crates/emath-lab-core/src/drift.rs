//!: drift monitoring.
//!
//! Monitors input, quality, error, latency, memory, fallback and
//! provider-health drift against a frozen expectation. Every alert is
//! typed (`E-HOST-010`) and deterministic; the monitor never mutates
//! anything but its own alert log.

use crate::error::LabError;

/// Drift dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DriftKind {
    /// Input distribution/fingerprint drift.
    Input,
    /// Output quality drift.
    Quality,
    /// Error-rate drift.
    Error,
    /// Latency/throughput drift.
    Latency,
    /// Memory drift.
    Memory,
    /// Fallback-rate drift.
    Fallback,
    /// Provider health drift.
    ProviderHealth,
}

impl DriftKind {
    /// Stable kind token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Quality => "quality",
            Self::Error => "error",
            Self::Latency => "latency",
            Self::Memory => "memory",
            Self::Fallback => "fallback",
            Self::ProviderHealth => "provider_health",
        }
    }
}

/// One frozen drift band: kind (+ optional metric) with a relative
/// tolerance against the observed baseline expectation.
#[derive(Clone, Debug, PartialEq)]
pub struct DriftBand {
    /// Drift dimension.
    pub kind: DriftKind,
    /// Metric id; empty matches every metric of the kind.
    pub metric_id: String,
    /// Relative tolerance (`|observed - expected| / expected`).
    pub relative_tolerance: f64,
}

impl DriftBand {
    /// Validates the band (`E-HOST-003`).
    pub fn validate(&self) -> Result<(), LabError> {
        if !self.relative_tolerance.is_finite() || self.relative_tolerance <= 0.0 {
            return Err(LabError::new(
                "E-HOST-003",
                format!(
                    "drift band tolerance must be positive for {}",
                    self.kind.as_str()
                ),
            ));
        }
        Ok(())
    }
}

/// One detected drift alert (`E-HOST-010`).
#[derive(Clone, Debug, PartialEq)]
pub struct DriftAlert {
    /// Drift dimension.
    pub kind: DriftKind,
    /// Metric id.
    pub metric_id: String,
    /// Observed value.
    pub observed: f64,
    /// Expected value (baseline expectation).
    pub expected: f64,
    /// Band tolerance that fired.
    pub relative_tolerance: f64,
}

impl DriftAlert {
    /// Stable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "E-HOST-010"
    }

    /// One-line description.
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "E-HOST-010: {} drift in {}: observed {}, expected {} (tolerance {})",
            self.kind.as_str(),
            self.metric_id,
            self.observed,
            self.expected,
            self.relative_tolerance
        )
    }
}

/// Deterministic drift monitor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DriftMonitor {
    bands: Vec<DriftBand>,
    alerts: Vec<DriftAlert>,
}

impl DriftMonitor {
    /// Builds a monitor; every band must be valid (`E-HOST-003`).
    pub fn new(bands: Vec<DriftBand>) -> Result<Self, LabError> {
        for band in &bands {
            band.validate()?;
        }
        Ok(Self {
            bands,
            alerts: Vec::new(),
        })
    }

    /// Observes one value against its expectation; returns the new
    /// alerts (bands that fired), appended in band order.
    pub fn observe(
        &mut self,
        kind: DriftKind,
        metric_id: &str,
        observed: f64,
        expected: f64,
    ) -> Vec<DriftAlert> {
        let mut fired = Vec::new();
        if expected == 0.0 {
            return fired;
        }
        let deviation = (observed - expected).abs() / expected.abs();
        for band in &self.bands {
            if band.kind == kind
                && (band.metric_id.is_empty() || band.metric_id == metric_id)
                && deviation > band.relative_tolerance
            {
                let alert = DriftAlert {
                    kind,
                    metric_id: metric_id.to_string(),
                    observed,
                    expected,
                    relative_tolerance: band.relative_tolerance,
                };
                self.alerts.push(alert.clone());
                fired.push(alert);
            }
        }
        fired
    }

    /// All accumulated alerts, in observation order.
    #[must_use]
    pub fn alerts(&self) -> &[DriftAlert] {
        &self.alerts
    }

    /// Whether any alert has fired.
    #[must_use]
    pub fn drifted(&self) -> bool {
        !self.alerts.is_empty()
    }

    /// Clears the alert log (bands stay frozen).
    pub fn clear(&mut self) {
        self.alerts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor() -> DriftMonitor {
        DriftMonitor::new(vec![
            DriftBand {
                kind: DriftKind::Latency,
                metric_id: "latency".into(),
                relative_tolerance: 0.1,
            },
            DriftBand {
                kind: DriftKind::Quality,
                metric_id: String::new(),
                relative_tolerance: 0.05,
            },
        ])
        .unwrap()
    }

    #[test]
    fn in_band_observations_stay_silent() {
        let mut monitor = monitor();
        assert!(monitor
            .observe(DriftKind::Latency, "latency", 105.0, 100.0)
            .is_empty());
        assert!(monitor
            .observe(DriftKind::Quality, "score", 1.02, 1.0)
            .is_empty());
        assert!(!monitor.drifted());
    }

    #[test]
    fn out_of_band_observations_fire_typed_alerts() {
        let mut monitor = monitor();
        let alerts = monitor.observe(DriftKind::Latency, "latency", 120.0, 100.0);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].code(), "E-HOST-010");
        assert!(alerts[0].message().contains("E-HOST-010: latency drift"));
        assert!(monitor.drifted());
    }

    #[test]
    fn unrelated_kinds_do_not_fire() {
        let mut monitor = monitor();
        assert!(monitor
            .observe(DriftKind::Memory, "peak", 500.0, 100.0)
            .is_empty());
        assert!(!monitor.drifted());
    }

    #[test]
    fn degenerate_bands_are_refused() {
        let error = DriftMonitor::new(vec![DriftBand {
            kind: DriftKind::Input,
            metric_id: String::new(),
            relative_tolerance: 0.0,
        }])
        .unwrap_err();
        assert_eq!(error.code, "E-HOST-003");
    }
}
