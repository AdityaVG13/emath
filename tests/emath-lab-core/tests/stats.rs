//! Stats-protocol percentile and mean tests.

use emath_lab_core::stats::{mean, percentile, percentile_f64};
use emath_test_harness::Probe;

#[allow(clippy::float_cmp)]
#[test]
fn stats_protocol() {
    let mut p = Probe::new("empty inputs refuse E-HOST-006; percentiles interpolate exactly");
    p.case("empty-refused", |p| {
        p.eq("u64", percentile(&[], 0.5).unwrap_err().code, "E-HOST-006");
        p.eq("f64", percentile_f64(&[], 0.5).unwrap_err().code, "E-HOST-006");
        p.eq("mean", mean(&[]).unwrap_err().code, "E-HOST-006");
    });
    p.case("single-is-sample", |p| {
        p.eq("u64", percentile(&[42], 0.5).unwrap(), 42.0);
        p.eq("f64", percentile_f64(&[1.5], 0.99).unwrap(), 1.5);
    });
    p.case("interpolated", |p| {
        p.eq("median", percentile(&[10, 20], 0.5).unwrap(), 15.0);
        p.eq("min", percentile(&[10, 20], 0.0).unwrap(), 10.0);
        p.eq("max", percentile(&[10, 20], 1.0).unwrap(), 20.0);
    });
    p.finish();
}
