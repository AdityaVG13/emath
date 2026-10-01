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
//!   `Rat` slot accepts `Float64`. `Int` stays exact in every world.
//!
//! Disagreement between worlds is information about the definition
//! (rounding, cancellation, a branch flipped by rounding), not a fault
//! of either world. The world is per thread and scoped by `with_world`,
//! so one process can evaluate the same tree under several worlds.

use std::cell::Cell;

use emath_core::tree::{BinaryOp, Expr, ExprKind, GenericArg, TypeExpr, TypeKind, UnaryOp};

use super::prelude::*;
use super::{CValue, ConstructorError, fault, parse_exact, parse_int};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierWorld {
    Reference,
    Exact,
    Float64,
}

impl CarrierWorld {
    pub const ALL: [CarrierWorld; 3] = [Self::Reference, Self::Exact, Self::Float64];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Exact => "exact",
            Self::Float64 => "float64",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|world| world.name() == name)
    }
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
#[must_use]
pub fn world_input(world: CarrierWorld, declared: Option<&TypeExpr>, raw: &str) -> CValue {
    let value = parse_constructor_scalar(raw);
    let exact_decimals = match world {
        CarrierWorld::Exact => true,
        CarrierWorld::Reference => declared.and_then(real_carrier_of) == Some("Rat"),
        CarrierWorld::Float64 => false,
    };
    coerce_reals(value, world, exact_decimals)
}

fn coerce_reals(value: CValue, world: CarrierWorld, exact_decimals: bool) -> CValue {
    match value {
        // `{:?}` is the shortest text that reads back as `x`: the decimal
        // the caller wrote.
        CValue::Float64(x) if exact_decimals => decimal_to_rational(&format!("{:?}", x.abs()))
            .and_then(|(numer, denom)| {
                let num = parse_exact(&numer).ok()?;
                let num = if x < 0.0 {
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
            })
            .unwrap_or(CValue::Float64(x)),
        CValue::Rat { num, den } if world == CarrierWorld::Float64 => {
            CValue::Float64(num.to_f64() / den.to_f64())
        }
        CValue::Sequence(items) => CValue::Sequence(std::sync::Arc::new(
            items
                .iter()
                .cloned()
                .map(|item| coerce_reals(item, world, exact_decimals))
                .collect(),
        )),
        other => other,
    }
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
/// and the binary64 nearest 0.2 disagree.
pub fn answers_agree(left: &CValue, right: &CValue) -> Result<bool, ConstructorError> {
    eq_values(left, right)
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
