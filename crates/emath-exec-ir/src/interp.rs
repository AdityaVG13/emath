//! Strict-f64 interpreter for [`EmirProgram`](crate::EmirProgram).
//!
//! Registers are typed exactly like the locals the Rust backend emits
//! (`f64` / `bool`). Type confusion is a typed fault, never a coercion.
//!
//! # Determinism
//!
//! Arithmetic, comparisons, `min`/`max`, `abs`/`floor`/`ceil`, `is_finite`,
//! and boolean ops are bit-exact IEEE-754 binary64 across platforms.
//! Transcendentals (`sin`, `cos`, `tan`, `tanh`, `exp`, `ln`, `powf`,
//! `atan2`) follow the platform libm -- the same caveat as generated Rust
//! (Tier 1). Domain obligations recorded during lowering are assumptions,
//! not runtime checks: division by zero yields inf/NaN per IEEE, matching
//! the emitted Rust which also does not insert those checks.

use crate::{EmirOp, EmirProgram, EmirValue};
use std::fmt;

/// A typed register value. Locals match generated Rust (`f64` / `bool`).
#[derive(Clone, Copy, Debug)]
pub enum Value {
    /// IEEE-754 binary64.
    F64(f64),
    /// Boolean, produced by comparisons, `is_finite`, `and`/`or`/`not`.
    Bool(bool),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::F64(left), Self::F64(right)) => left.to_bits() == right.to_bits(),
            (Self::Bool(left), Self::Bool(right)) => left == right,
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::F64(value) => f.write_str(&format_f64(*value)),
            Self::Bool(value) => write!(f, "{value}"),
        }
    }
}

/// Format an f64 for display and JSON number tokens.
///
/// Finite values use `format!("{v}")`, with a trailing `.0` when that
/// spelling would otherwise look like an integer. Non-finite values are
/// the strings `NaN`, `Infinity`, and `-Infinity`.
#[must_use]
pub fn format_f64(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    let mut text = format!("{value}");
    if !text.contains('.') && !text.contains('e') && !text.contains('E') {
        text.push_str(".0");
    }
    text
}

/// Typed evaluation fault. The interpreter never panics on a well-formed
/// program; every failure is one of these variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalFault {
    /// An operand had the wrong type for `op` (no coercion).
    TypeConfusion {
        /// Register that failed the type check.
        register: u32,
        /// EMIR op name (`EmirOp::name`).
        op: &'static str,
    },
    /// `LoadInput` index was outside the provided input slice.
    MissingInput(u16),
    /// `LoadState` index was outside the provided state slice.
    MissingState(u16),
    /// An operand or the program result named an unwritten register.
    BadRegister(u32),
}

impl fmt::Display for EvalFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeConfusion { register, op } => {
                write!(f, "type confusion at %{register} in {op}")
            }
            Self::MissingInput(index) => write!(f, "missing input {index}"),
            Self::MissingState(index) => write!(f, "missing state {index}"),
            Self::BadRegister(register) => write!(f, "bad register %{register}"),
        }
    }
}

/// Evaluate `program` in a single forward pass over its linear ops.
///
/// `inputs` and `state` are indexed by [`EmirOp::LoadInput`] /
/// [`EmirOp::LoadState`]. Missing slots are faults. IEEE-754 exceptions
/// (division by zero, invalid) are not faults.
///
/// `And` / `Or` evaluate both operands (the linear IR already materialized
/// them as registers) then apply `&&` / `||`, matching the Rust backend
/// which emits `&&` / `||` against those registers.
pub fn evaluate(program: &EmirProgram, inputs: &[f64], state: &[f64]) -> Result<Value, EvalFault> {
    let mut registers: Vec<Value> = Vec::with_capacity(program.ops.len());
    for (op, _) in &program.ops {
        let value = eval_op(op, &registers, inputs, state)?;
        registers.push(value);
    }
    register(&registers, program.result)
}

fn eval_op(
    op: &EmirOp,
    registers: &[Value],
    inputs: &[f64],
    state: &[f64],
) -> Result<Value, EvalFault> {
    let name = op.name();
    match *op {
        EmirOp::ConstF64(bits) => Ok(Value::F64(f64::from_bits(bits))),
        EmirOp::LoadInput(index) => inputs
            .get(usize::from(index))
            .copied()
            .map(Value::F64)
            .ok_or(EvalFault::MissingInput(index)),
        EmirOp::LoadState(index) => state
            .get(usize::from(index))
            .copied()
            .map(Value::F64)
            .ok_or(EvalFault::MissingState(index)),
        EmirOp::F64Add(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)? + f64_of(registers, right, name)?,
        )),
        EmirOp::F64Sub(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)? - f64_of(registers, right, name)?,
        )),
        EmirOp::F64Mul(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)? * f64_of(registers, right, name)?,
        )),
        EmirOp::F64Div(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)? / f64_of(registers, right, name)?,
        )),
        EmirOp::F64Pow(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.powf(f64_of(registers, right, name)?),
        )),
        EmirOp::Neg(value) => Ok(Value::F64(-f64_of(registers, value, name)?)),
        EmirOp::Exp(value) => Ok(Value::F64(f64_of(registers, value, name)?.exp())),
        EmirOp::Ln(value) => Ok(Value::F64(f64_of(registers, value, name)?.ln())),
        EmirOp::Sqrt(value) => Ok(Value::F64(f64_of(registers, value, name)?.sqrt())),
        EmirOp::Sin(value) => Ok(Value::F64(f64_of(registers, value, name)?.sin())),
        EmirOp::Cos(value) => Ok(Value::F64(f64_of(registers, value, name)?.cos())),
        EmirOp::Tan(value) => Ok(Value::F64(f64_of(registers, value, name)?.tan())),
        EmirOp::Tanh(value) => Ok(Value::F64(f64_of(registers, value, name)?.tanh())),
        EmirOp::Abs(value) => Ok(Value::F64(f64_of(registers, value, name)?.abs())),
        EmirOp::Floor(value) => Ok(Value::F64(f64_of(registers, value, name)?.floor())),
        EmirOp::Ceil(value) => Ok(Value::F64(f64_of(registers, value, name)?.ceil())),
        EmirOp::Min(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.min(f64_of(registers, right, name)?),
        )),
        EmirOp::Max(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.max(f64_of(registers, right, name)?),
        )),
        EmirOp::Atan2(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.atan2(f64_of(registers, right, name)?),
        )),
        EmirOp::Lt(left, right) => Ok(Value::Bool(
            f64_of(registers, left, name)? < f64_of(registers, right, name)?,
        )),
        EmirOp::Le(left, right) => Ok(Value::Bool(
            f64_of(registers, left, name)? <= f64_of(registers, right, name)?,
        )),
        EmirOp::Gt(left, right) => Ok(Value::Bool(
            f64_of(registers, left, name)? > f64_of(registers, right, name)?,
        )),
        EmirOp::Ge(left, right) => Ok(Value::Bool(
            f64_of(registers, left, name)? >= f64_of(registers, right, name)?,
        )),
        EmirOp::Eq(left, right) => eq_ne(registers, left, right, name, true),
        EmirOp::Ne(left, right) => eq_ne(registers, left, right, name, false),
        EmirOp::And(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? && bool_of(registers, right, name)?,
        )),
        EmirOp::Or(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? || bool_of(registers, right, name)?,
        )),
        EmirOp::Not(value) => Ok(Value::Bool(!bool_of(registers, value, name)?)),
        EmirOp::IsFinite(value) => Ok(Value::Bool(f64_of(registers, value, name)?.is_finite())),
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => {
            if bool_of(registers, condition, name)? {
                register(registers, then_value)
            } else {
                register(registers, else_value)
            }
        }
    }
}

fn eq_ne(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    equal: bool,
) -> Result<Value, EvalFault> {
    let left_value = register(registers, left)?;
    let right_value = register(registers, right)?;
    let result = match (left_value, right_value) {
        (Value::F64(left), Value::F64(right)) => left == right,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        _ => {
            return Err(EvalFault::TypeConfusion {
                register: left.0,
                op,
            });
        }
    };
    Ok(Value::Bool(if equal { result } else { !result }))
}

fn register(registers: &[Value], value: EmirValue) -> Result<Value, EvalFault> {
    registers
        .get(value.0 as usize)
        .copied()
        .ok_or(EvalFault::BadRegister(value.0))
}

fn f64_of(registers: &[Value], value: EmirValue, op: &'static str) -> Result<f64, EvalFault> {
    match register(registers, value)? {
        Value::F64(number) => Ok(number),
        Value::Bool(_) => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

fn bool_of(registers: &[Value], value: EmirValue, op: &'static str) -> Result<bool, EvalFault> {
    match register(registers, value)? {
        Value::Bool(flag) => Ok(flag),
        Value::F64(_) => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EmirOp;
    use emath_core::Span;

    fn program(ops: Vec<EmirOp>) -> EmirProgram {
        let last = u32::try_from(ops.len().saturating_sub(1)).unwrap_or(0);
        EmirProgram {
            ops: ops.into_iter().map(|op| (op, Span::default())).collect(),
            result: EmirValue(last),
            input_count: 0,
            state_count: 0,
            domain_obligations: Vec::new(),
        }
    }

    fn const_bits(value: f64) -> EmirOp {
        EmirOp::ConstF64(value.to_bits())
    }

    #[test]
    fn add_spot() {
        let program = program(vec![
            const_bits(2.0),
            const_bits(3.0),
            EmirOp::F64Add(EmirValue(0), EmirValue(1)),
        ]);
        assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::F64(5.0));
    }

    #[test]
    fn pow_spot() {
        let program = program(vec![
            const_bits(2.0),
            const_bits(3.0),
            EmirOp::F64Pow(EmirValue(0), EmirValue(1)),
        ]);
        assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::F64(8.0));
    }

    #[test]
    fn select_spot() {
        let program = program(vec![
            const_bits(1.0),
            const_bits(0.0),
            const_bits(2.0),
            const_bits(1.0),
            EmirOp::Gt(EmirValue(0), EmirValue(1)),
            EmirOp::Select {
                condition: EmirValue(4),
                then_value: EmirValue(2),
                else_value: EmirValue(3),
            },
        ]);
        assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::F64(2.0));
    }

    #[test]
    fn is_finite_spot() {
        let program = program(vec![const_bits(1.0), EmirOp::IsFinite(EmirValue(0))]);
        assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn div_by_zero_is_inf() {
        let program = program(vec![
            const_bits(1.0),
            const_bits(0.0),
            EmirOp::F64Div(EmirValue(0), EmirValue(1)),
        ]);
        match evaluate(&program, &[], &[]).unwrap() {
            Value::F64(value) => assert!(value.is_infinite() && value.is_sign_positive()),
            other => panic!("expected +inf, got {other:?}"),
        }
    }

    #[test]
    fn eq_nan_is_false() {
        let nan = f64::NAN.to_bits();
        let program = program(vec![
            EmirOp::ConstF64(nan),
            EmirOp::ConstF64(nan),
            EmirOp::Eq(EmirValue(0), EmirValue(1)),
        ]);
        assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn type_confusion_and_on_f64() {
        let program = program(vec![
            const_bits(1.0),
            const_bits(0.0),
            EmirOp::And(EmirValue(0), EmirValue(1)),
        ]);
        assert_eq!(
            evaluate(&program, &[], &[]).unwrap_err(),
            EvalFault::TypeConfusion {
                register: 0,
                op: "and",
            }
        );
    }
}
