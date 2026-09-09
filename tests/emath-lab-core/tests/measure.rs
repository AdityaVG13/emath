//! Measurement spread quarantine tests.

use emath_lab_core::measure::QUARANTINE_CV_PCT;
use emath_lab_core::{Measurement, MeasurementKind, Summary};
use emath_test_harness::Probe;

fn summarized(samples: &[u64]) -> Summary {
    Measurement { metric_id: "test".into(), kind: MeasurementKind::LatencyNs, unit: "ns".into(), samples: samples.to_vec() }.summarize().expect("non-empty samples summarize")
}

#[allow(clippy::float_cmp)]
#[test]
fn measurement_spread() {
    let mut p = Probe::new("noisy cells quarantine, tight and degenerate cells stay eligible");
    p.case("quarantine", |p| {
        let noisy = summarized(&[100, 110, 120]);
        p.demand("noisy-cv", noisy.cv_pct > QUARANTINE_CV_PCT, format!("wide cell crosses threshold: {}", noisy.cv_pct));
        p.demand("noisy-out", noisy.quarantined(), "noisy cell quarantines");
        let tight = summarized(&[100, 101, 99, 100, 102, 101]);
        p.demand("tight-cv", tight.cv_pct < QUARANTINE_CV_PCT, format!("tight cell stays under: {}", tight.cv_pct));
        p.demand("tight-in", !tight.quarantined(), "tight cell stays eligible");
    });
    p.case("degenerate", |p| {
        p.eq("single", summarized(&[42]).cv_pct, 0.0);
        p.demand("single-in", !summarized(&[42]).quarantined(), "single sample never quarantines");
        p.eq("zero-mean", summarized(&[0, 0, 0]).cv_pct, 0.0);
        p.demand("zero-in", !summarized(&[0, 0, 0]).quarantined(), "zero mean never divides by zero");
    });
    p.finish();
}
