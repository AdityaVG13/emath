//! Runtime numeric-body error model: typed refusals, never panics.
//!
//! `simpson` and `sample_limit` refused by assert/panic historically;
//! they now refuse typed (the checked-only policy index kernels already
//! follow). The panicking i64 wrappers (`factorial`, `mod_inv`,
//! `pow_mod`, `sqrt_mod`, `poly_eval_mod`, `rs_encode`,
//! `hamming_distance`) are gone; their `_checked` twins are the surface.

use emath_rt::{factorial_checked, hamming_distance_checked, sample_limit, simpson};
use emath_test_harness::{Probe, boot};

#[test]
fn probe() {
    boot();
    let mut p = Probe::new("numeric body refuses typed, never panics");
    p.case("simpson", |p| {
        p.demand(
            "odd-steps",
            simpson(&|x| x * x, 0.0, 2.0, 5).is_err(),
            "odd panel count must refuse typed",
        );
        p.demand(
            "zero-steps",
            simpson(&|x| x * x, 0.0, 2.0, 0).is_err(),
            "non-positive panel count must refuse typed",
        );
        match simpson(&|x| x * x, 0.0, 2.0, 2) {
            Ok(value) => {
                p.eq("exact-quadratic", value, 8.0 / 3.0);
            }
            Err(_) => {
                p.fail("exact-quadratic", "Simpson is exact on quadratics");
            }
        }
    });
    p.case("sample_limit", |p| {
        p.demand(
            "no-finite",
            sample_limit(&|_| f64::NAN, 0.0, 0.0).is_err(),
            "all-NaN sampling must refuse typed",
        );
        match sample_limit(&|x| x * x, 2.0, 0.0) {
            Ok(value) => {
                p.demand("converges", (value - 4.0).abs() < 0.05, "x^2 at 2 converges to 4");
            }
            Err(_) => {
                p.fail("converges", "x^2 at 2 converges to 4");
            }
        }
    });
    p.case("checked_surface", |p| {
        p.demand(
            "factorial",
            factorial_checked(21).is_err(),
            "factorial past 20 refuses typed",
        );
        p.demand(
            "hamming",
            hamming_distance_checked(&[1.0], &[1.0, 2.0]).is_err(),
            "length mismatch refuses typed",
        );
    });
    p.finish();
}
