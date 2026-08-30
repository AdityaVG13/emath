//! Failure-first kernel tests for the `i128` exact-rational `Rat` in
//! `emath-rt`. Every test asserts exact integer values — no floats anywhere.

use emath_rt::rat::{Rat, RatError};

/// Canonical-form invariant probe: `den > 0`, `gcd(|num|, den) == 1`,
/// zero is exactly `0/1`.
fn assert_canonical(r: Rat) {
    assert!(r.den() > 0, "denominator must be strictly positive");
    let (mut a, mut b) = (r.num().unsigned_abs(), r.den().unsigned_abs());
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    assert_eq!(a, 1, "gcd(|num|, den) must be 1 for {r:?}");
}

#[test]
fn construction_normalizes_sign_and_gcd() {
    let r = Rat::new(6, 4).unwrap();
    assert_eq!((r.num(), r.den()), (3, 2));

    let r = Rat::new(6, -4).unwrap();
    assert_eq!((r.num(), r.den()), (-3, 2)); // sign lives on num

    let r = Rat::new(0, 5).unwrap();
    assert_eq!((r.num(), r.den()), (0, 1)); // zero is 0/1
}

#[test]
fn zero_denominator_is_typed_error() {
    assert_eq!(Rat::new(1, 0), Err(RatError::ZeroDenominator));
}

#[test]
fn arithmetic_exact_values() {
    let a = Rat::new(1, 3).unwrap();
    let b = Rat::new(1, 6).unwrap();
    let s = a.add(b).unwrap();
    assert_eq!((s.num(), s.den()), (1, 2));

    let d = a.sub(b).unwrap();
    assert_eq!((d.num(), d.den()), (1, 6));

    let m = a.mul(b).unwrap();
    assert_eq!((m.num(), m.den()), (1, 18));

    let q = a.div(b).unwrap();
    assert_eq!((q.num(), q.den()), (2, 1));
}

#[test]
fn f64_unrepresentable_denominator_stays_exact() {
    // 10^18 + 7 — f64 cannot represent this denominator exactly.
    let big = 1_000_000_000_000_000_007_i128;
    let r = Rat::new(3, big).unwrap();
    assert_eq!((r.num(), r.den()), (3, big));

    let s = Rat::new(2, big).unwrap();
    let sum = r.add(s).unwrap();
    assert_eq!((sum.num(), sum.den()), (5, big));
}

#[test]
fn mul_near_i128_max_refuses_to_wrap() {
    let big = Rat::new(i128::MAX / 3, 2).unwrap();
    let two = Rat::new(2, 1).unwrap();
    // (2^127-1)/2 squared exceeds i128::MAX — must be Overflow, never wrap.
    assert_eq!(big.mul(big), Err(RatError::Overflow));
    assert_eq!(two.mul(two), Ok(Rat::new(4, 1).unwrap()));
}

#[test]
fn div_by_zero_rational_is_typed_error() {
    let a = Rat::new(1, 3).unwrap();
    let z = Rat::new(0, 7).unwrap();
    assert_eq!(a.div(z), Err(RatError::ZeroDenominator));
}

#[test]
fn extreme_extrema_are_refused_not_panicked() {
    // i128::MIN numerator: |MIN| == MAX + 1, negation and scaling overflow.
    let min = Rat::new(i128::MIN, 1).unwrap();
    let one = Rat::new(1, 1).unwrap();
    // gcd collapses MIN/1 to itself; mul by 1 is exact — both must be Ok.
    assert_eq!((min.num(), min.den()), (i128::MIN, 1));
    assert!(min.mul(one).is_ok());
    // MIN scaled by 2 overflows i128 — refused.
    let two = Rat::new(2, 1).unwrap();
    assert_eq!(min.mul(two), Err(RatError::Overflow));
}

#[test]
fn associativity_spot_samples_on_ugly_denominators() {
    let a = Rat::new(123_456_789, 987_654_321).unwrap();
    let b = Rat::new(-77_777, 103_103).unwrap();
    let c = Rat::new(10_007, 65_537).unwrap();
    let left = a.add(b.add(c).unwrap()).unwrap();
    let right = a.add(b).unwrap().add(c).unwrap();
    assert_eq!(left, right);
    assert_canonical(left);
}
