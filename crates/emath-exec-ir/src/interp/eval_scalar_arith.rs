use super::{EmirProgram, EmirOp, Value, ValueFrame, EvalBudget, EvalFault, scalar_arithmetic, ScalarOp, scalar_binary, scalar_neg, register, f64_of, exact_int_of, value_from_exact, eval_exact_int_call, scalar_unary, comparison, eq_ne, boolean_binary, bool_of};

pub(super) fn eval_scalar_arith(
    _self_program: &EmirProgram,
    op: &EmirOp,
    registers: &[Value],
    _inputs: ValueFrame<'_>,
    _state: ValueFrame<'_>,
    _budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
        EmirOp::F64Add(left, right) => {
            scalar_arithmetic(registers, *left, *right, "f64-add", ScalarOp::Add)
        }
        EmirOp::F64Sub(left, right) => {
            scalar_arithmetic(registers, *left, *right, "f64-sub", ScalarOp::Sub)
        }
        EmirOp::F64Mul(left, right) => {
            scalar_arithmetic(registers, *left, *right, "f64-mul", ScalarOp::Mul)
        }
        EmirOp::F64Div(left, right) => {
            scalar_arithmetic(registers, *left, *right, "f64-div", ScalarOp::Div)
        }
        EmirOp::F64Pow(left, right) => scalar_binary(registers, *left, *right, "pow", f64::powf),
        EmirOp::Neg(value) => scalar_neg(registers, *value),
        EmirOp::ToInt(value) => {
            if let Value::I64(value) = register(registers, *value)? {
                return Ok(Value::I64(*value));
            }
            let value = f64_of(registers, *value, "to-int")?;
            if value >= i64::MIN as f64 && value < -(i64::MIN as f64) && value.fract() == 0.0 {
                Ok(Value::I64(value as i64))
            } else {
                Err(EvalFault::Arithmetic {
                    op: "to-int",
                    detail: "E-SCALAR-CONVERT: value must be an exact integer in i64 range",
                })
            }
        }
        EmirOp::IntegerQuotient(left, right) => {
            let n = exact_int_of(register(registers, *left)?, "integer-quotient")?;
            let d = exact_int_of(register(registers, *right)?, "integer-quotient")?;
            n.quot(&d)
                .map(value_from_exact)
                .map_err(|err| EvalFault::CarrierRefused {
                    op: "integer-quotient",
                    detail: err.to_string(),
                })
        }
        EmirOp::ExactIntCall { name, args } => eval_exact_int_call(registers, name, args),
        EmirOp::SameBits(left, right) => Ok(Value::Bool(
            f64_of(registers, *left, "same-bits")?.to_bits()
                == f64_of(registers, *right, "same-bits")?.to_bits(),
        )),
        EmirOp::UnaryBuiltin(builtin, value) => scalar_unary(registers, *value, *builtin),
        EmirOp::BinaryBuiltin(builtin, left, right) => {
            let left = f64_of(registers, *left, "binary-kernel")?;
            let right = f64_of(registers, *right, "binary-kernel")?;
            builtin
                .eval_binary(left, right)
                .map(Value::F64)
                .ok_or(EvalFault::Arithmetic {
                    op: "binary-kernel",
                    detail: "unary scalar opcode used in binary instruction",
                })
        }
        EmirOp::Lt(left, right) => comparison(registers, *left, *right, "lt", |a, b| a < b),
        EmirOp::Le(left, right) => comparison(registers, *left, *right, "le", |a, b| a <= b),
        EmirOp::Gt(left, right) => comparison(registers, *left, *right, "gt", |a, b| a > b),
        EmirOp::Ge(left, right) => comparison(registers, *left, *right, "ge", |a, b| a >= b),
        EmirOp::Eq(left, right) => eq_ne(registers, *left, *right, "eq", true),
        EmirOp::Ne(left, right) => eq_ne(registers, *left, *right, "ne", false),
        EmirOp::And(left, right) => boolean_binary(registers, *left, *right, "and", |a, b| a && b),
        EmirOp::Or(left, right) => boolean_binary(registers, *left, *right, "or", |a, b| a || b),
        EmirOp::Imply(left, right) => {
            boolean_binary(registers, *left, *right, "imply", |a, b| !a || b)
        }
        EmirOp::Iff(left, right) => boolean_binary(registers, *left, *right, "iff", |a, b| a == b),
        EmirOp::Not(value) => Ok(Value::Bool(!bool_of(registers, *value, "not")?)),
        EmirOp::IsFinite(value) => Ok(Value::Bool(
            f64_of(registers, *value, "is-finite")?.is_finite(),
        )),
        _ => unreachable!("op routed to the wrong eval family"),
    }
}
