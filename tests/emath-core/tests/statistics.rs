//! statistics tests migrated from the in-crate `#[cfg(test)]` module:
//! every symbol they exercise is public crate surface.

use emath_core::statistics::*;

#[test]
fn type7_interpolation_math_is_exact_on_the_classic_sample() {
    // sorted = [2,4,4,4,5,5,7,9]; h = (n-1)p
    let values = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
    let q = quantile(&values, 0.75).expect("computes");
    assert!((q.value - 5.5).abs() < 1e-12);
}

#[test]
fn quantile_probability_bounds_refuse() {
    assert!(quantile(&[1.0], 1.5).is_err());
    assert!(quantile(&[1.0], -0.1).is_err());
    // Boundaries are valid.
    assert!(quantile(&[1.0, 2.0], 0.0).is_ok());
    assert!(quantile(&[1.0, 2.0], 1.0).is_ok());
}
