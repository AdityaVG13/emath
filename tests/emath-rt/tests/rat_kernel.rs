//! Failure-first kernel tests for the `i128` exact-rational `Rat` in
//! `emath-rt`. Every test asserts exact integer values — no floats anywhere.

use emath_rt::rat::{Rat, RatError};
use emath_test_harness::Probe;

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

#[test]
fn rat_kernel() {
    let mut p = Probe::new("exact-rational Rat stays canonical and exact");
    p.case("construct", |p| {
        p.eq("reduce", (Rat::new(6, 4).unwrap().num(), Rat::new(6, 4).unwrap().den()), (3, 2));
        p.eq("sign", (Rat::new(6, -4).unwrap().num(), Rat::new(6, -4).unwrap().den()), (-3, 2));
        let z = Rat::new(0, 5).unwrap();
        p.eq("zero", (z.num(), z.den()), (0, 1));
        p.eq("zero-den", Rat::new(1, 0), Err(RatError::ZeroDenominator));
    });
    p.case("arith", |p| {
        let a = Rat::new(1, 3).unwrap();
        let b = Rat::new(1, 6).unwrap();
        let s = a.add(b).unwrap();
        p.eq("add", (s.num(), s.den()), (1, 2));
        let d = a.sub(b).unwrap();
        p.eq("sub", (d.num(), d.den()), (1, 6));
        let m = a.mul(b).unwrap();
        p.eq("mul", (m.num(), m.den()), (1, 18));
        let q = a.div(b).unwrap();
        p.eq("div", (q.num(), q.den()), (2, 1));
        let big = 1_000_000_000_000_000_007_i128;
        let r = Rat::new(3, big).unwrap();
        p.eq("big-den", (r.num(), r.den()), (3, big));
        let sum = r.add(Rat::new(2, big).unwrap()).unwrap();
        p.eq("big-add", (sum.num(), sum.den()), (5, big));
    });
    p.case("refuse", |p| {
        let big = Rat::new(i128::MAX / 3, 2).unwrap();
        p.eq("overflow", big.mul(big), Err(RatError::Overflow));
        p.eq("small-mul", Rat::new(2, 1).unwrap().mul(Rat::new(2, 1).unwrap()), Ok(Rat::new(4, 1).unwrap()));
        p.eq("div-zero", Rat::new(1, 3).unwrap().div(Rat::new(0, 7).unwrap()), Err(RatError::ZeroDenominator));
        let min = Rat::new(i128::MIN, 1).unwrap();
        p.eq("min", (min.num(), min.den()), (i128::MIN, 1));
        p.demand("min-mul1", min.mul(Rat::new(1, 1).unwrap()).is_ok(), "MIN*1 is exact");
        p.eq("min-overflow", min.mul(Rat::new(2, 1).unwrap()), Err(RatError::Overflow));
    });
    p.case("assoc", |p| {
        let a = Rat::new(123_456_789, 987_654_321).unwrap();
        let b = Rat::new(-77_777, 103_103).unwrap();
        let c = Rat::new(10_007, 65_537).unwrap();
        let left = a.add(b.add(c).unwrap()).unwrap();
        let right = a.add(b).unwrap().add(c).unwrap();
        p.eq("assoc", left, right);
        p.demand("den-pos", left.den() > 0, "denominator positive");
        p.eq("gcd1", gcd(left.num().unsigned_abs(), left.den().unsigned_abs()), 1);
    });
    p.finish();
}
