//! Stats protocol tests (origin `crates/emath-lab-core/src/stats.rs`).

use emath_lab_core::stats::{mean, percentile, percentile_f64};

#[test]
fn empty_percentile_is_e_host_006_not_an_index_panic() {
    let err = percentile(&[], 0.5).unwrap_err();
    assert_eq!(err.code, "E-HOST-006");
    let err = percentile_f64(&[], 0.5).unwrap_err();
    assert_eq!(err.code, "E-HOST-006");
    let err = mean(&[]).unwrap_err();
    assert_eq!(err.code, "E-HOST-006");
}

// The expected values below are exact by construction (integer casts,
// whole f64 literals, and a midpoint of two integers), so bitwise
// equality is the honest assertion.
#[allow(clippy::float_cmp)]
#[test]
fn single_sample_percentile_is_the_sample() {
    assert_eq!(percentile(&[42], 0.5).unwrap(), 42.0);
    assert_eq!(percentile_f64(&[1.5], 0.99).unwrap(), 1.5);
}

#[allow(clippy::float_cmp)]
#[test]
fn interpolated_percentile_of_two_samples() {
    // Median of {10, 20} is the midpoint; p0/p100 are the extrema.
    assert_eq!(percentile(&[10, 20], 0.5).unwrap(), 15.0);
    assert_eq!(percentile(&[10, 20], 0.0).unwrap(), 10.0);
    assert_eq!(percentile(&[10, 20], 1.0).unwrap(), 20.0);
}
