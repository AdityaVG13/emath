//! Runtime numeric-body error model: typed refusals, never panics.
//!
//! The former panicking i64 wrappers (`factorial`, `pow_mod`,
//! `poly_eval_mod`, `rs_encode`, `hamming_distance`) are gone; their
//! `_checked` twins are the surface. `simpson`, `sample_limit`, and the
//! `mod_inv`/`sqrt_mod` kernels are gone entirely — quadrature and
//! limits are authored `.emath` (`calculus.float64_quadrature`,
//! `numerics.limits`), inverses and modular square roots are authored
//! lanes (`cryptology.modular`, `exact.quadratic`).

use emath_rt::{factorial_checked, hamming_distance_checked};
use emath_test_harness::{Probe, boot};

#[test]
fn exact_integer_order_crosses_storage_boundaries() {
    use emath_rt::ExactInt;
    let values = [
        "-340282366920938463463374607431768211456",
        "-170141183460469231731687303715884105729",
        "-170141183460469231731687303715884105728",
        "-4294967296",
        "-1",
        "0",
        "1",
        "4294967296",
        "170141183460469231731687303715884105727",
        "170141183460469231731687303715884105728",
        "340282366920938463463374607431768211456",
    ]
    .map(|text| ExactInt::parse(text).unwrap());
    for (i, a) in values.iter().enumerate() {
        for (j, b) in values.iter().enumerate() {
            assert_eq!(a.cmp(b), i.cmp(&j), "{a} vs {b}");
        }
    }
    for (small, neg, limbs) in [
        (0, false, vec![]),
        (7, false, vec![7]),
        (-7, true, vec![7]),
        (i128::MIN, true, vec![0, 0, 0, 0x8000_0000]),
    ] {
        let a = ExactInt::from(small);
        let b = ExactInt::Big { neg, limbs };
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
        assert_eq!(b.cmp(&a), std::cmp::Ordering::Equal);
    }
}

#[test]
fn probe() {
    boot();
    let mut p = Probe::new("numeric body refuses typed, never panics");
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
