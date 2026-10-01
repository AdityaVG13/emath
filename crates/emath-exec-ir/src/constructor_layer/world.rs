//! Carrier worlds: configurations of the reference engine that read the
//! same definitions under a different real-number carrier.
//!
//! A world never changes the program text or adds an operation. It
//! changes how the engine realizes the real-number carriers:
//!
//! - `Reference`: as authored. `Rat` is exact, `Float64` is binary64.
//! - `Exact`: every real is an exact rational. Decimal literals denote
//!   their exact decimal value and a `Float64` slot accepts `Rat`/`Int`.
//!   A Float64 value can still appear (an input the caller supplied as a
//!   float, or an operation with no exact result); it is reported as
//!   rounded, never relabeled exact.
//! - `Float64`: real-valued DATA is binary64: real inputs, and every
//!   quotient of integers computed at run time (`1 / n`). The program's
//!   own constants - expressions built only from number literals
//!   (`1 / 6`, `0 / 1`) - and exact arithmetic among them stay exact,
//!   because they define the method (a Runge-Kutta tableau and its
//!   order conditions), not its data. A constant that meets a binary64
//!   rounds to nearest there, as a constant does in a float program. A
//!   `Rat` slot accepts `Float64`. `Int` stays exact in every float world.
//! - `Float32`, `BFloat16`: the `Float64` world with real data rounded
//!   to IEEE binary32 (24-bit significand) or bfloat16 (8-bit
//!   significand, binary32 exponent range). Values are held in binary64;
//!   every input, decimal literal, and arithmetic result is rounded once
//!   to the narrower format. For `+ - * /` computing in binary64 and
//!   rounding once equals computing in the narrow format, because
//!   53 >= 2p + 2 (no double-rounding error). Decimals and rational
//!   inputs round from their exact value. Overflow is
//!   `non_finite_scalar`.
//! - `Int64`: reals as in `Reference`; every `Int` arithmetic result
//!   must fit a signed 64-bit machine integer, and one that does not is
//!   refused as `machine_overflow` instead of growing. `Rat` stays exact.
//! - `Interval`: real data is an enclosure (`CValue::Interval`, binary64
//!   endpoints rounded outward, see `interval`): a decimal or rational
//!   input or decimal literal reads as the tightest binary64 interval
//!   around its exact value, a caller may pass a range `[lo, hi]`, and
//!   every operation encloses its exact result. Program constants stay
//!   exact as in the float worlds. The answer encloses the exact answer
//!   for every input inside the input intervals; a branch the
//!   enclosures cannot decide refuses `undecided_comparison`.
//!
//! Disagreement between worlds is information about the definition
//! (rounding, cancellation, a branch flipped by rounding), not a fault
//! of either world. The world is per thread and scoped by `with_world`,
//! so one process can evaluate the same tree under several worlds.

use std::cell::Cell;

use emath_core::tree::{BinaryOp, Expr, ExprKind, GenericArg, TypeExpr, TypeKind, UnaryOp};

use super::prelude::*;
use super::{CValue, ConstructorError, ExactInt, fault, parse_exact, parse_int};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierWorld {
    Reference,
    Exact,
    Float64,
    Float32,
    BFloat16,
    Int64,
    Interval,
}

impl CarrierWorld {
    pub const ALL: [CarrierWorld; 7] = [
        Self::Reference,
        Self::Exact,
        Self::Float64,
        Self::Float32,
        Self::BFloat16,
        Self::Int64,
        Self::Interval,
    ];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Exact => "exact",
            Self::Float64 => "float64",
            Self::Float32 => "float32",
            Self::BFloat16 => "bfloat16",
            Self::Int64 => "int64",
            Self::Interval => "interval",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|world| world.name() == name)
    }

    /// Real data is a rounded binary float in this world.
    pub(super) fn is_float(self) -> bool {
        matches!(self, Self::Float64 | Self::Float32 | Self::BFloat16)
    }

    /// Real data is inexact here (a float or an enclosure) while the
    /// program's literal constants stay exact.
    pub(super) fn splits_data(self) -> bool {
        self.is_float() || self == Self::Interval
    }

    /// (significand bits, minimum normal exponent, maximum exponent) of
    /// a float format narrower than binary64.
    fn narrow_format(self) -> Option<(i32, i32, i32)> {
        match self {
            Self::Float32 => Some((24, -126, 127)),
            Self::BFloat16 => Some((8, -126, 127)),
            _ => None,
        }
    }

    /// Round a binary64 to this world's float format (identity outside
    /// the narrow float worlds).
    pub(super) fn round(self, x: f64) -> f64 {
        match self.narrow_format() {
            Some((precision, emin, emax)) => round_binary(x, precision, emin, emax),
            None => x,
        }
    }

    /// The value `num / den` rounded once to this world's float format.
    /// The binary64 quotient is within half a binary64 ulp; when it lands
    /// exactly on a midpoint of the narrow format, the exact value
    /// decides the side, so the result is the correctly rounded one.
    pub(super) fn round_rational(self, num: &ExactInt, den: &ExactInt) -> f64 {
        let mut x = num.to_f64() / den.to_f64();
        if let Some((precision, emin, _)) = self.narrow_format() {
            if x != 0.0 && x.is_finite() && (x / quantum(x, precision, emin)).fract().abs() == 0.5 {
                let exact = CValue::Rat {
                    num: num.clone(),
                    den: den.clone(),
                };
                if let Ok(order) = cmp_numeric(&exact, &CValue::Float64(x)) {
                    // Step one binary64 ulp toward the exact value: off
                    // the midpoint, on its side.
                    let away = (order == std::cmp::Ordering::Greater) == (x > 0.0);
                    if order != std::cmp::Ordering::Equal {
                        let bits = x.to_bits();
                        x = f64::from_bits(if away { bits + 1 } else { bits - 1 });
                    }
                }
            }
        }
        self.round(x)
    }
}

fn pow2(k: i32) -> f64 {
    // `k` stays inside the binary64 normal range for every narrow format.
    f64::from_bits(((k + 1023) as u64) << 52)
}

/// The spacing of the narrow format's representable values around `x`.
fn quantum(x: f64, precision: i32, emin: i32) -> f64 {
    let exponent = ((x.abs().to_bits() >> 52) as i32) - 1023;
    pow2(exponent.max(emin) - (precision - 1))
}

/// Round to nearest, ties to even, in a binary format with `precision`
/// significand bits and exponents `emin..=emax` (gradual underflow
/// below `emin`, infinity past the largest finite value).
fn round_binary(x: f64, precision: i32, emin: i32, emax: i32) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let step = quantum(x, precision, emin);
    let rounded = (x / step).round_ties_even() * step;
    let largest = (2.0 - pow2(1 - precision)) * pow2(emax);
    if rounded.abs() > largest {
        f64::INFINITY.copysign(x)
    } else {
        rounded
    }
}

/// An `Int` arithmetic result under the current world: in `Int64` it
/// must fit a signed 64-bit integer.
pub(super) fn machine_int(n: ExactInt) -> Result<CValue, ConstructorError> {
    if current() == CarrierWorld::Int64 && n.to_i64().is_none() {
        return Err(fault(
            "machine_overflow",
            format!("{n} does not fit a signed 64-bit integer"),
        ));
    }
    Ok(CValue::Int(n))
}

thread_local! {
    static WORLD: Cell<CarrierWorld> = const { Cell::new(CarrierWorld::Reference) };
}

/// Run `body` with `world` installed on this thread; the previous world
/// is restored on return, including on unwind.
pub fn with_world<T>(world: CarrierWorld, body: impl FnOnce() -> T) -> T {
    struct Restore(CarrierWorld);
    impl Drop for Restore {
        fn drop(&mut self) {
            WORLD.with(|cell| cell.set(self.0));
        }
    }
    let _restore = Restore(WORLD.with(|cell| cell.replace(world)));
    body()
}

pub(super) fn current() -> CarrierWorld {
    WORLD.with(Cell::get)
}

/// An expression built only from integer and rational literals with
/// `+ - * /` and sign: a constant of the program's definition
/// (`1 / 6`, `-(1 / 2)`, `0 / 1`), not run-time data.
pub(super) fn is_literal_constant(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Int(_) | ExprKind::Rational { .. } => true,
        ExprKind::Unary {
            op: UnaryOp::Neg | UnaryOp::Pos,
            value,
        } => is_literal_constant(value),
        ExprKind::Binary { op, left, right } => {
            matches!(
                op,
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div
            ) && is_literal_constant(left)
                && is_literal_constant(right)
        }
        _ => false,
    }
}

/// The exact value of a literal constant, computed under the reference
/// carrier laws whatever world is installed. Pure: no engine step, so a
/// budget suspension can never land inside a constant.
pub(super) fn exact_constant(expr: &Expr) -> Result<CValue, ConstructorError> {
    fn go(expr: &Expr) -> Result<CValue, ConstructorError> {
        match &expr.kind {
            ExprKind::Int(text) => parse_int(text),
            ExprKind::Rational { numer, denom } => CValue::Rat {
                num: parse_exact(numer)?,
                den: parse_exact(denom)?,
            }
            .canon(),
            ExprKind::Unary { op, value } => apply_unary(*op, go(value)?),
            ExprKind::Binary { op, left, right } => binary(*op, go(left)?, go(right)?),
            _ => Err(fault("type", "not a literal constant")),
        }
    }
    with_world(CarrierWorld::Reference, || go(expr))
}

/// Read a caller-supplied input value in `world`. `declared` is the
/// entry's declared input type when known.
///
/// - Exact: a decimal reads as its exact decimal rational.
/// - Float64: a rational reads as its nearest binary64.
/// - Reference: a decimal for a `Rat` input reads exactly, as a `given`
///   in authored tests does; everything else reads as `emath run` does.
/// - Interval: a real reads as its tightest enclosure; `[lo, hi]` for a
///   scalar real input is the range itself, rounded outward.
pub fn world_input(
    world: CarrierWorld,
    declared: Option<&TypeExpr>,
    raw: &str,
) -> Result<CValue, ConstructorError> {
    if world == CarrierWorld::Interval && declared.is_some_and(is_scalar_real) {
        if let Some(range) = raw
            .trim()
            .strip_prefix('[')
            .and_then(|r| r.strip_suffix(']'))
        {
            return read_range(range);
        }
    }
    let value = parse_constructor_scalar(raw);
    let exact_decimals = match world {
        // The narrow float worlds round a decimal from its exact value,
        // and the interval world encloses it.
        CarrierWorld::Exact
        | CarrierWorld::Float32
        | CarrierWorld::BFloat16
        | CarrierWorld::Interval => true,
        CarrierWorld::Reference | CarrierWorld::Int64 => {
            declared.and_then(real_carrier_of) == Some("Rat")
        }
        CarrierWorld::Float64 => false,
    };
    // A scalar decimal reads from the caller's text: a binary64 reading
    // first would lose digits past the 17th.
    let value = match value {
        CValue::Float64(_) if exact_decimals => exact_decimal(raw.trim()).unwrap_or(value),
        other => other,
    };
    coerce_reals(value, world, exact_decimals)
}

/// `lo, hi` as an interval: each end read exactly, then rounded outward.
fn read_range(range: &str) -> Result<CValue, ConstructorError> {
    let invalid = || {
        fault(
            "invalid_input",
            format!("`[{range}]` is not a range `[lo, hi]` of two numbers"),
        )
    };
    let (lo, hi) = range.split_once(',').ok_or_else(invalid)?;
    let end = |text: &str| match parse_constructor_scalar(text.trim()) {
        CValue::Float64(_) => exact_decimal(text.trim()).ok_or_else(invalid),
        value @ (CValue::Int(_) | CValue::Rat { .. }) => Ok(value),
        _ => Err(invalid()),
    };
    let (lo, hi) = (end(lo)?, end(hi)?);
    if cmp_numeric(&lo, &hi)?.is_gt() {
        return Err(fault(
            "invalid_input",
            format!("the range `[{range}]` has its lower end above its upper end"),
        ));
    }
    super::interval::interval(
        super::interval::enclose(&lo)?.0,
        super::interval::enclose(&hi)?.1,
    )
}

/// A declared real scalar type (`Rat`, `Float64`), not a container.
fn is_scalar_real(ty: &TypeExpr) -> bool {
    matches!(&ty.kind, TypeKind::Path { segments, generic_args }
        if generic_args.is_empty()
            && matches!(segments.last().map(String::as_str), Some("Rat" | "Float64")))
}

/// The exact rational a signed decimal text denotes.
fn exact_decimal(text: &str) -> Option<CValue> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (numer, denom) = decimal_to_rational(digits)?;
    let num = parse_exact(&numer).ok()?;
    let num = if negative {
        num.checked_neg().ok()?
    } else {
        num
    };
    CValue::Rat {
        num,
        den: parse_exact(&denom).ok()?,
    }
    .canon()
    .ok()
}

fn coerce_reals(
    value: CValue,
    world: CarrierWorld,
    exact_decimals: bool,
) -> Result<CValue, ConstructorError> {
    Ok(match value {
        // Inside a sequence the caller's text is gone; `{:?}` is the
        // shortest text that reads back as `x`.
        CValue::Float64(x) if exact_decimals => match exact_decimal(&format!("{x:?}")) {
            Some(exact) => coerce_reals(exact, world, false)?,
            None => coerce_reals(CValue::Float64(x), world, false)?,
        },
        CValue::Rat { num, den } if world.is_float() => {
            CValue::Float64(world.round_rational(&num, &den))
        }
        value @ (CValue::Rat { .. } | CValue::Float64(_)) if world == CarrierWorld::Interval => {
            let (lo, hi) = super::interval::enclose(&value)?;
            super::interval::interval(lo, hi)?
        }
        CValue::Float64(x) => CValue::Float64(world.round(x)),
        CValue::Sequence(items) => CValue::Sequence(std::sync::Arc::new(
            items
                .iter()
                .cloned()
                .map(|item| coerce_reals(item, world, exact_decimals))
                .collect::<Result<_, _>>()?,
        )),
        other => other,
    })
}

/// The real-number carrier a declared type names, looking through
/// sequence element types.
fn real_carrier_of(ty: &TypeExpr) -> Option<&'static str> {
    match &ty.kind {
        TypeKind::Path {
            segments,
            generic_args,
        } => match segments.last().map(String::as_str) {
            Some("Rat") => Some("Rat"),
            Some("Float64") => Some("Float64"),
            _ => generic_args.iter().find_map(|arg| match arg {
                GenericArg::Type(inner) => real_carrier_of(inner),
                _ => None,
            }),
        },
        TypeKind::List(items) => items.iter().find_map(real_carrier_of),
        _ => None,
    }
}

/// Whether two answers denote the same value. Numeric carriers compare
/// exactly by value (a finite binary64 is an exact rational), so `1/5`
/// and the binary64 nearest 0.2 disagree. An interval agrees with a
/// number it encloses, and with an identical interval.
pub fn answers_agree(left: &CValue, right: &CValue) -> Result<bool, ConstructorError> {
    use super::interval::contains;
    match (left, right) {
        (CValue::Interval { lo: a, hi: b }, CValue::Interval { lo: c, hi: d }) => {
            Ok(a == c && b == d)
        }
        (CValue::Interval { .. }, other) => contains(left, other),
        (other, CValue::Interval { .. }) => contains(right, other),
        (CValue::Sequence(a), CValue::Sequence(b)) => all_agree(a, b),
        (CValue::Tuple(a), CValue::Tuple(b)) => all_agree(a, b),
        _ => eq_values(left, right),
    }
}

fn all_agree(left: &[CValue], right: &[CValue]) -> Result<bool, ConstructorError> {
    if left.len() != right.len() {
        return Ok(false);
    }
    for (a, b) in left.iter().zip(right) {
        if !answers_agree(a, b)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// An upper bound on the width of an interval answer.
#[must_use]
pub fn interval_width(value: &CValue) -> Option<f64> {
    match value {
        CValue::Interval { lo, hi } => Some(super::interval::width(*lo, *hi)),
        _ => None,
    }
}

/// `left - right` for two numeric scalars, computed exactly and rounded
/// once to binary64. `None` when either side is not a finite number.
#[must_use]
pub fn answer_gap(left: &CValue, right: &CValue) -> Option<f64> {
    let exact = |value: &CValue| match value {
        CValue::Float64(x) => f64_exact_rat(*x),
        CValue::Int(_) | CValue::Rat { .. } => as_rat(value.clone()).ok(),
        _ => None,
    };
    let (ln, ld) = exact(left)?;
    let (rn, rd) = exact(right)?;
    let num = ln.mul(&rd).ok()?.sub(&rn.mul(&ld).ok()?).ok()?;
    let den = ld.mul(&rd).ok()?;
    let gap = num.to_f64() / den.to_f64();
    if gap.is_finite() {
        Some(gap)
    } else {
        Some(float_of(left).ok()? - float_of(right).ok()?)
    }
}
