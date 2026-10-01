//! The interval carrier of the `interval` world.
//!
//! An interval `[lo, hi]` holds finite binary64 endpoints and stands for
//! a real known only to lie between them. Every operation returns an
//! interval that contains every result of the exact operation on members
//! of its operands, so the final interval encloses the answer the exact
//! computation would give from any inputs inside the input intervals.
//!
//! Directed rounding without changing the FPU mode: each endpoint is
//! computed round-to-nearest together with the sign of its exact error
//! (`TwoSum` for `+`, a fused multiply-add for `*` and `/`), then stepped
//! one ulp outward only when the rounding went the wrong way. Results
//! too close to the subnormal range for the error to be exact step
//! outward on both sides.
//!
//! An order between overlapping intervals is not determined by the
//! enclosures; it refuses `undecided_comparison` rather than guess. A
//! divisor interval that contains zero refuses `division_by_zero`.

use std::cmp::Ordering;

use emath_core::tree::BinaryOp;

use super::prelude::*;
use super::{CValue, ConstructorError, ExactInt, fault};

/// Below this magnitude the error terms can underflow and stop being
/// exact (2^-969 = `f64::MIN_POSITIVE * 2^53`).
const TINY: f64 = f64::MIN_POSITIVE * 9_007_199_254_740_992.0;

pub(super) fn interval(lo: f64, hi: f64) -> Result<CValue, ConstructorError> {
    if !lo.is_finite() || !hi.is_finite() {
        return Err(fault(
            "non_finite_scalar",
            "an interval endpoint overflowed binary64",
        ));
    }
    Ok(CValue::Interval { lo, hi })
}

/// The least binary64 above `x` (`f64::next_up` is newer than the MSRV).
fn next_up(x: f64) -> f64 {
    if x.is_nan() || x == f64::INFINITY {
        x
    } else if x == 0.0 {
        f64::from_bits(1)
    } else if x > 0.0 {
        f64::from_bits(x.to_bits() + 1)
    } else {
        f64::from_bits(x.to_bits() - 1)
    }
}

fn next_down(x: f64) -> f64 {
    -next_up(-x)
}

/// The tightest binary64 interval containing `num / den`.
pub(super) fn enclose_rational(
    num: &ExactInt,
    den: &ExactInt,
) -> Result<(f64, f64), ConstructorError> {
    let exact = CValue::Rat {
        num: num.clone(),
        den: den.clone(),
    };
    let x = crate::exact_int::exact_ratio_f64(num, den);
    if !x.is_finite() {
        return Err(fault(
            "non_finite_scalar",
            format!("{num}/{den} has no finite binary64 enclosure"),
        ));
    }
    let order = |y: f64| cmp_numeric(&exact, &CValue::Float64(y));
    // `x` is the nearest binary64 to the exact value: walk to the floats
    // that bracket it.
    let mut lo = x;
    while order(lo)?.is_lt() {
        lo = next_down(lo);
    }
    while order(next_up(lo))?.is_ge() {
        lo = next_up(lo);
    }
    let mut hi = x;
    while order(hi)?.is_gt() {
        hi = next_up(hi);
    }
    while order(next_down(hi))?.is_le() {
        hi = next_down(hi);
    }
    Ok((lo, hi))
}

/// The interval a numeric value stands for.
pub(super) fn enclose(value: &CValue) -> Result<(f64, f64), ConstructorError> {
    match value {
        CValue::Interval { lo, hi } => Ok((*lo, *hi)),
        CValue::Float64(x) if x.is_finite() => Ok((*x, *x)),
        CValue::Int(n) => enclose_rational(n, &ExactInt::one()),
        CValue::Rat { num, den } => enclose_rational(num, den),
        CValue::Affine(_) => {
            let (lo, hi) = super::affine::bounds(value)?;
            Ok((enclose(&lo)?.0, enclose(&hi)?.1))
        }
        other => Err(fault("type", format!("{other} is not a real number"))),
    }
}

/// `(down, up)`: the nearest result `r` and the sign of its exact error
/// `err` (exact result = r + err) give the two directed roundings.
fn directed(r: f64, err: f64, exact_error: bool) -> (f64, f64) {
    if !exact_error {
        (next_down(r), next_up(r))
    } else if err > 0.0 {
        (r, next_up(r))
    } else if err < 0.0 {
        (next_down(r), r)
    } else {
        (r, r)
    }
}

fn add(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bv = s - a;
    let err = (a - (s - bv)) + (b - bv);
    directed(s, err, true)
}

fn mul(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let err = a.mul_add(b, -p);
    directed(p, err, p.abs() >= TINY || a == 0.0 || b == 0.0)
}

fn div(a: f64, b: f64) -> (f64, f64) {
    let q = a / b;
    // a = q*b + rem exactly; the exact quotient is q + rem/b.
    let rem = -q.mul_add(b, -a);
    let err = if b > 0.0 { rem } else { -rem };
    directed(q, err, q.abs() >= TINY && a.abs() >= TINY || a == 0.0)
}

/// `left op right` where either operand is an interval (or the world
/// reads a quotient of integers as data).
pub(super) fn arithmetic(
    op: BinaryOp,
    left: &CValue,
    right: &CValue,
) -> Result<CValue, ConstructorError> {
    let (al, ah) = enclose(left)?;
    let (bl, bh) = enclose(right)?;
    match op {
        BinaryOp::Add => interval(add(al, bl).0, add(ah, bh).1),
        BinaryOp::Sub => interval(add(al, -bh).0, add(ah, -bl).1),
        BinaryOp::Mul | BinaryOp::Div => {
            if op == BinaryOp::Div && bl <= 0.0 && bh >= 0.0 {
                return Err(fault(
                    "division_by_zero",
                    format!("the divisor {right} may be zero"),
                ));
            }
            let corner = if op == BinaryOp::Mul { mul } else { div };
            let corners = [
                corner(al, bl),
                corner(al, bh),
                corner(ah, bl),
                corner(ah, bh),
            ];
            let lo = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
            let hi = corners
                .iter()
                .map(|c| c.1)
                .fold(f64::NEG_INFINITY, f64::max);
            interval(lo, hi)
        }
        other => Err(fault(
            "implementation_unavailable",
            format!("operator {other:?} is not an interval operation"),
        )),
    }
}

/// The order of two reals, at least one an interval, when the
/// enclosures decide it.
pub(super) fn order(left: &CValue, right: &CValue) -> Result<Ordering, ConstructorError> {
    let (al, ah) = enclose(left)?;
    let (bl, bh) = enclose(right)?;
    if ah < bl {
        Ok(Ordering::Less)
    } else if al > bh {
        Ok(Ordering::Greater)
    } else if al == ah && bl == bh && al == bl {
        Ok(Ordering::Equal)
    } else {
        Err(fault(
            "undecided_comparison",
            format!("{left} and {right} overlap: their order is not determined"),
        ))
    }
}

/// Whether `outer` (an interval) contains every value `inner` stands for.
pub(super) fn contains(outer: &CValue, inner: &CValue) -> Result<bool, ConstructorError> {
    let (ol, oh) = enclose(outer)?;
    let (il, ih) = enclose(inner)?;
    Ok(ol <= il && ih <= oh)
}

/// An upper bound on `hi - lo`.
pub(super) fn width(lo: f64, hi: f64) -> f64 {
    add(hi, -lo).1
}
