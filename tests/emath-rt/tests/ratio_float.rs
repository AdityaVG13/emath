#![forbid(unsafe_code)]
//! `exact_ratio_f64`: an exact ratio rounds once, to nearest with ties to
//! even, into binary64, whatever the size of its numerator and
//! denominator (the quotient of two separately rounded halves overflows
//! or double-rounds).

use emath_rt::{ExactInt, exact_ratio_f64};
use emath_test_harness::Probe;

fn int(text: &str) -> ExactInt {
    ExactInt::parse(text).expect("integer literal")
}

fn two_pow(k: u32) -> ExactInt {
    ExactInt::from(2_i32).pow(&ExactInt::from(i64::from(k))).expect("power of two")
}

fn ten_pow(k: u32) -> ExactInt {
    ExactInt::from(10_i32).pow(&ExactInt::from(i64::from(k))).expect("power of ten")
}

/// A finite binary64 as an exact ratio.
fn exact(x: f64) -> (ExactInt, ExactInt) {
    let bits = x.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = (bits & ((1 << 52) - 1)) as i64;
    let (mantissa, e) = if exponent == 0 { (fraction, -1074) } else { (fraction | (1 << 52), exponent - 1075) };
    let m = ExactInt::from(if x < 0.0 { -mantissa } else { mantissa });
    if e >= 0 { (m.mul(&two_pow(e as u32)).unwrap(), ExactInt::one()) } else { (m, two_pow((-e) as u32)) }
}

fn step(x: f64, up: bool) -> f64 {
    let bits = x.to_bits();
    f64::from_bits(if up { bits + 1 } else { bits - 1 })
}

/// `sign(a/b - c/d)` for positive denominators.
fn cmp(a: &(ExactInt, ExactInt), c: &(ExactInt, ExactInt)) -> std::cmp::Ordering {
    a.0.mul(&c.1).unwrap().cmp(&c.0.mul(&a.1).unwrap())
}

fn mid(a: f64, b: f64) -> (ExactInt, ExactInt) {
    let (an, ad) = exact(a);
    let (bn, bd) = exact(b);
    (an.mul(&bd).unwrap().add(&bn.mul(&ad).unwrap()).unwrap(), ad.mul(&bd).unwrap().mul(&ExactInt::from(2_i32)).unwrap())
}

/// Halfway between `f64::MAX` and 2^1024: the overflow threshold.
fn overflow_mid() -> (ExactInt, ExactInt) {
    (two_pow(1024).sub(&two_pow(970)).unwrap(), ExactInt::one())
}

/// `x` is the nearest binary64 to the positive ratio `r` (ties to even;
/// infinity past the overflow threshold).
fn nearest(x: f64, r: &(ExactInt, ExactInt)) -> bool {
    use std::cmp::Ordering::{Equal, Less};
    if x.is_infinite() {
        return cmp(r, &overflow_mid()) != Less;
    }
    let even = x.to_bits() & 1 == 0;
    let inside = |order: std::cmp::Ordering| order == Less || (even && order == Equal);
    let below = x == 0.0 || inside(cmp(&mid(step(x, false), x), r));
    let upper = if x == f64::MAX { overflow_mid() } else { mid(x, step(x, true)) };
    below && inside(cmp(r, &upper))
}

#[test]
fn ratio_float() {
    let mut p = Probe::new("exact ratios round once to the nearest binary64");
    let three = ten_pow(400).mul(&ExactInt::from(3_i32)).unwrap().add(&ExactInt::one()).unwrap();
    p.eq("huge-num-and-den", exact_ratio_f64(&three, &ten_pow(400)), 3.0);
    p.eq("huge-negative", exact_ratio_f64(&three.checked_neg().unwrap(), &ten_pow(400)), -3.0);
    p.eq("huge-integer-overflows", exact_ratio_f64(&ten_pow(400), &ExactInt::one()), f64::INFINITY);
    p.eq("tiny-underflows", exact_ratio_f64(&ExactInt::one(), &ten_pow(400)), 0.0);
    p.eq("zero", exact_ratio_f64(&ExactInt::zero(), &ten_pow(400)), 0.0);
    p.eq("min-subnormal", exact_ratio_f64(&ExactInt::one(), &two_pow(1074)), f64::from_bits(1));
    // Half the smallest subnormal ties to even (zero); just above it rounds up.
    p.eq("subnormal-tie-to-zero", exact_ratio_f64(&ExactInt::one(), &two_pow(1075)), 0.0);
    let above_half = two_pow(1100).add(&ExactInt::one()).unwrap();
    p.eq("subnormal-above-tie", exact_ratio_f64(&above_half, &two_pow(2175)), f64::from_bits(1));
    // Halfway between f64::MAX and 2^1024 ties to even: 2^1024, infinity.
    let max_mid = two_pow(1024).sub(&two_pow(970)).unwrap();
    p.eq("max-tie-overflows", exact_ratio_f64(&max_mid, &ExactInt::one()), f64::INFINITY);
    let below_mid = max_mid.sub(&ExactInt::one()).unwrap();
    p.eq("below-max-tie", exact_ratio_f64(&below_mid, &ExactInt::one()), f64::MAX);
    // 2^53 + 1 is a tie between 2^53 and 2^53 + 2: even wins.
    p.eq("integer-tie-even", exact_ratio_f64(&int("9007199254740993"), &ExactInt::one()), 9_007_199_254_740_992.0);
    // Both halves past 2^53: each would round on its own before dividing.
    let num = int("36028797018963971");
    let den = int("36028797018963969");
    let x = exact_ratio_f64(&num, &den);
    p.demand("wide-i128-pair", nearest(x, &(num.clone(), den.clone())), format!("{x:e}"));
    let mut wrong = Vec::new();
    let nums = ["1", "2", "3", "7", "10", "9007199254740993", "36028797018963971", "170141183460469231731687303715884105727"];
    let dens = ["3", "7", "10", "49", "9007199254740995", "36028797018963969", "340282366920938463463374607431768211457"];
    for n in nums {
        for d in dens {
            for (scale_num, scale_den) in [(0, 0), (900, 0), (0, 900), (1500, 1500)] {
                let r = (int(n).mul(&two_pow(scale_num)).unwrap(), int(d).mul(&two_pow(scale_den)).unwrap());
                let x = exact_ratio_f64(&r.0, &r.1);
                if !nearest(x, &r) {
                    wrong.push(format!("{n}*2^{scale_num} / {d}*2^{scale_den} -> {x:e}"));
                }
            }
        }
    }
    p.demand("nearest-everywhere", wrong.is_empty(), wrong.join("\n"));
    // ExactInt::to_f64 is the same single rounding.
    p.eq("to-f64-top-binade", two_pow(1023).to_f64(), f64::from_bits(2046 << 52));
    p.eq("to-f64-below-max-tie", below_mid.to_f64(), f64::MAX);
    let wide = two_pow(200).add(&two_pow(147)).unwrap().add(&ExactInt::one()).unwrap();
    p.eq("to-f64-tie-broken-by-low-bits", wide.to_f64(), f64::from_bits((1023 + 200) << 52 | 1));
    p.finish();
}
