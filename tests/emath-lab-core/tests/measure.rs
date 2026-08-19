//! Measure tests (origin `crates/emath-lab-core/src/measure.rs`).

use emath_lab_core::measure::QUARANTINE_CV_PCT;
use emath_lab_core::{Measurement, MeasurementKind, Summary};

fn summarized(samples: &[u64]) -> Summary {
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
