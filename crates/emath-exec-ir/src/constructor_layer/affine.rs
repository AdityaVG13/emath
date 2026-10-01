//! The affine carrier of the `affine` world.
//!
//! An affine form `c + a1·e1 + ... + an·en` stands for a real that
//! depends on noise symbols `ei`, each an unknown in [-1, 1] shared by
//! every value computed from it. The center and coefficients are exact
//! rationals, so `+`, `-` and scaling are exact and correlated
//! quantities cancel (`x - x` is 0 for a range `x`, where an interval
//! gives a band twice as wide as `x`). A product keeps its linear part
//! and bounds the quadratic remainder by `rad(x)·rad(y)` on one fresh
//! symbol; a quotient multiplies by the min-range linearization of
//! `1/y` (Stolfi and de Figueiredo, "Self-validated numerical methods
//! and applications", 1997, section 3.4). Every form contains the exact
//! answer for every assignment of its symbols, so for every input
//! inside the input ranges.
//!
//! A form with no symbols left is its exact center. An order the form
//! cannot decide refuses `undecided_comparison`; a divisor whose range
//! contains zero refuses `division_by_zero`.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;

use emath_core::tree::BinaryOp;

use super::prelude::*;
use super::world::{with_world, CarrierWorld};
use super::{CValue, ConstructorError, ExactInt, fault};

#[derive(Clone, Debug)]
pub struct AffineForm {
    /// Exact `Int` or `Rat`.
    pub center: CValue,
    /// Noise symbol -> nonzero exact coefficient.
    pub terms: BTreeMap<u64, CValue>,
}

thread_local! {
    static NEXT_SYMBOL: Cell<u64> = const { Cell::new(0) };
}

/// A noise symbol no live value on this thread uses.
fn fresh() -> u64 {
    NEXT_SYMBOL.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    })
}

/// Keep fresh symbols past `id` (a form restored from a checkpoint).
pub(super) fn reserve(id: u64) {
    NEXT_SYMBOL.with(|next| next.set(next.get().max(id + 1)));
}

/// Exact rational arithmetic on `Int`/`Rat`, whatever world is installed.
fn exact(op: BinaryOp, left: &CValue, right: &CValue) -> Result<CValue, ConstructorError> {
    with_world(CarrierWorld::Reference, || binary(op, left.clone(), right.clone()))
}

fn int(n: i64) -> CValue {
    CValue::Int(ExactInt::from(n))
}

fn sign(value: &CValue) -> Result<std::cmp::Ordering, ConstructorError> {
    cmp_numeric(value, &int(0))
}

fn abs(value: &CValue) -> Result<CValue, ConstructorError> {
    if sign(value)?.is_lt() {
        exact(BinaryOp::Sub, &int(0), value)
    } else {
        Ok(value.clone())
    }
}

/// The center and terms of a real: a point has no terms.
fn parts(value: &CValue) -> Result<(CValue, BTreeMap<u64, CValue>), ConstructorError> {
    match value {
        CValue::Affine(form) => Ok((form.center.clone(), form.terms.clone())),
        CValue::Int(_) | CValue::Rat { .. } => Ok((value.clone(), BTreeMap::new())),
        CValue::Float64(x) => {
            let (num, den) = f64_exact_rat(*x)
                .ok_or_else(|| fault("non_finite_scalar", "non-finite Float64 operand"))?;
            Ok((CValue::Rat { num, den }.canon()?, BTreeMap::new()))
        }
        other => Err(fault("type", format!("{other} is not a real number"))),
    }
}

/// The form `center + terms`, or the exact center when no symbol is left.
/// A binary64 center or coefficient enters as its exact value, so a form
/// is always exact.
pub(super) fn form(
    center: CValue,
    terms: BTreeMap<u64, CValue>,
) -> Result<CValue, ConstructorError> {
    let center = parts(&center)?.0;
    let mut kept = BTreeMap::new();
    for (id, coefficient) in terms {
        let coefficient = parts(&coefficient)?.0;
        if !sign(&coefficient)?.is_eq() {
            kept.insert(id, coefficient);
        }
    }
    let terms = kept;
    if terms.is_empty() {
        return Ok(center);
    }
    Ok(CValue::Affine(Arc::new(AffineForm { center, terms })))
}

/// The form of a real known only to lie in `[lo, hi]` (exact ends): its
/// own fresh symbol.
pub(super) fn from_range(lo: &CValue, hi: &CValue) -> Result<CValue, ConstructorError> {
    let (lo, hi) = (&parts(lo)?.0, &parts(hi)?.0);
    let center = exact(BinaryOp::Div, &exact(BinaryOp::Add, lo, hi)?, &int(2))?;
    let radius = exact(BinaryOp::Div, &exact(BinaryOp::Sub, hi, lo)?, &int(2))?;
    form(center, BTreeMap::from([(fresh(), radius)]))
}

fn radius(terms: &BTreeMap<u64, CValue>) -> Result<CValue, ConstructorError> {
    terms
        .values()
        .try_fold(int(0), |sum, coefficient| exact(BinaryOp::Add, &sum, &abs(coefficient)?))
}

/// The exact range `[center - radius, center + radius]` of a real.
pub(super) fn bounds(value: &CValue) -> Result<(CValue, CValue), ConstructorError> {
    let (center, terms) = parts(value)?;
    let radius = radius(&terms)?;
    Ok((
        exact(BinaryOp::Sub, &center, &radius)?,
        exact(BinaryOp::Add, &center, &radius)?,
    ))
}

/// `(center, radius)` of a form, for display and width.
pub(super) fn center_radius(form: &AffineForm) -> Result<(CValue, CValue), ConstructorError> {
    Ok((form.center.clone(), radius(&form.terms)?))
}

/// `a·x + b·y` term by term.
fn combine(
    a: &CValue,
    x: &BTreeMap<u64, CValue>,
    b: &CValue,
    y: &BTreeMap<u64, CValue>,
) -> Result<BTreeMap<u64, CValue>, ConstructorError> {
    let mut out = BTreeMap::new();
    for id in x.keys().chain(y.keys()) {
        if out.contains_key(id) {
            continue;
        }
        let mut sum = int(0);
        if let Some(c) = x.get(id) {
            sum = exact(BinaryOp::Add, &sum, &exact(BinaryOp::Mul, a, c)?)?;
        }
        if let Some(c) = y.get(id) {
            sum = exact(BinaryOp::Add, &sum, &exact(BinaryOp::Mul, b, c)?)?;
        }
        out.insert(*id, sum);
    }
    Ok(out)
}

fn product(left: &CValue, right: &CValue) -> Result<CValue, ConstructorError> {
    let (xc, xt) = parts(left)?;
    let (yc, yt) = parts(right)?;
    let mut terms = combine(&yc, &xt, &xc, &yt)?;
    let remainder = exact(BinaryOp::Mul, &radius(&xt)?, &radius(&yt)?)?;
    if !sign(&remainder)?.is_eq() {
        terms.insert(fresh(), remainder);
    }
    form(exact(BinaryOp::Mul, &xc, &yc)?, terms)
}

/// `1/y` for a `y` whose range excludes zero: on `[a, b]`, `0 < a`, the
/// min-range fit `alpha·y + zeta ± delta` with
/// `alpha = -1/b²` (the slope at `b`): `1/t - alpha·t` falls from
/// `1/a + a/b²` at `a` to `2/b` at `b`, and `zeta`, `delta` are that
/// span's midpoint and half-width.
fn reciprocal(value: &CValue, original: &CValue) -> Result<CValue, ConstructorError> {
    let (lo, hi) = bounds(value)?;
    if !sign(&lo)?.is_gt() && !sign(&hi)?.is_lt() {
        return Err(fault(
            "division_by_zero",
            format!("the divisor {original} may be zero"),
        ));
    }
    if sign(&hi)?.is_lt() {
        let flipped = arithmetic(BinaryOp::Sub, &int(0), value)?;
        return arithmetic(BinaryOp::Sub, &int(0), &reciprocal(&flipped, original)?);
    }
    let (center, terms) = parts(value)?;
    if terms.is_empty() {
        return exact(BinaryOp::Div, &int(1), &center);
    }
    let one = int(1);
    let alpha = exact(
        BinaryOp::Sub,
        &int(0),
        &exact(BinaryOp::Div, &one, &exact(BinaryOp::Mul, &hi, &hi)?)?,
    )?;
    let at_lo = exact(
        BinaryOp::Sub,
        &exact(BinaryOp::Div, &one, &lo)?,
        &exact(BinaryOp::Mul, &alpha, &lo)?,
    )?;
    let at_hi = exact(BinaryOp::Div, &int(2), &hi)?;
    let zeta = exact(BinaryOp::Div, &exact(BinaryOp::Add, &at_lo, &at_hi)?, &int(2))?;
    let delta = exact(BinaryOp::Div, &exact(BinaryOp::Sub, &at_lo, &at_hi)?, &int(2))?;
    let mut scaled = combine(&alpha, &terms, &int(0), &BTreeMap::new())?;
    if !sign(&delta)?.is_eq() {
        scaled.insert(fresh(), delta);
    }
    form(
        exact(BinaryOp::Add, &exact(BinaryOp::Mul, &alpha, &center)?, &zeta)?,
        scaled,
    )
}

/// `left op right` where either operand is an affine form.
pub(super) fn arithmetic(
    op: BinaryOp,
    left: &CValue,
    right: &CValue,
) -> Result<CValue, ConstructorError> {
    match op {
        BinaryOp::Add | BinaryOp::Sub => {
            let (xc, xt) = parts(left)?;
            let (yc, yt) = parts(right)?;
            let b = if op == BinaryOp::Add { int(1) } else { int(-1) };
            form(exact(op, &xc, &yc)?, combine(&int(1), &xt, &b, &yt)?)
        }
        BinaryOp::Mul => product(left, right),
        BinaryOp::Div => product(left, &reciprocal(right, right)?),
        other => Err(fault(
            "implementation_unavailable",
            format!("operator {other:?} is not an affine operation"),
        )),
    }
}

/// The order of two reals, at least one an affine form, when every
/// assignment of the symbols gives the same order.
pub(super) fn order(left: &CValue, right: &CValue) -> Result<std::cmp::Ordering, ConstructorError> {
    let gap = arithmetic(BinaryOp::Sub, left, right)?;
    let (lo, hi) = bounds(&gap)?;
    if sign(&lo)?.is_gt() {
        Ok(std::cmp::Ordering::Greater)
    } else if sign(&hi)?.is_lt() {
        Ok(std::cmp::Ordering::Less)
    } else if !matches!(gap, CValue::Affine(_)) {
        Ok(std::cmp::Ordering::Equal)
    } else {
        Err(fault(
            "undecided_comparison",
            format!("{left} and {right} overlap: their order is not determined"),
        ))
    }
}

/// Whether the range of `outer` contains the range of `inner`.
pub(super) fn contains(outer: &CValue, inner: &CValue) -> Result<bool, ConstructorError> {
    let (ol, oh) = bounds(outer)?;
    let (il, ih) = bounds(inner)?;
    Ok(cmp_numeric(&ol, &il)?.is_le() && cmp_numeric(&ih, &oh)?.is_le())
}

/// The exact `(num, den)` of an `Int`/`Rat` (checkpoint text).
pub(super) fn ratio(value: &CValue) -> Result<(ExactInt, ExactInt), ConstructorError> {
    as_rat(value.clone())
}
