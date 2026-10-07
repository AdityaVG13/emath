use super::{EmirProgram, EmirOp, Value, ValueFrame, EvalBudget, EvalFault, scalar_arithmetic, ScalarOp, scalar_neg, f64_of, eval_exact_int_call, comparison, eq_ne, boolean_binary, bool_of};

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
        EmirOp::Neg(value) => scalar_neg(registers, *value),
        EmirOp::ExactIntCall { name, args } => eval_exact_int_call(registers, name, args),
        EmirOp::SameBits(left, right) => Ok(Value::Bool(
            f64_of(registers, *left, "same-bits")?.to_bits()
                == f64_of(registers, *right, "same-bits")?.to_bits(),
        )),
        EmirOp::Lt(left, right) => comparison(registers, *left, *right, "lt", |a, b| a < b),
        EmirOp::Le(left, right) => comparison(registers, *left, *right, "le", |a, b| a <= b),
        EmirOp::Gt(left, right) => comparison(registers, *left, *right, "gt", |a, b| a > b),
        EmirOp::Ge(left, right) => comparison(registers, *left, *right, "ge", |a, b| a >= b),
        EmirOp::Eq(left, right) => eq_ne(registers, *left, *right, "eq", true),
        EmirOp::Ne(left, right) => eq_ne(registers, *left, *right, "ne", false),
        EmirOp::And(left, right) => boolean_binary(registers, *left, *right, "and", |a, b| a && b),
        EmirOp::Or(left, right) => boolean_binary(registers, *left, *right, "or", |a, b| a || b),
        EmirOp::Not(value) => Ok(Value::Bool(!bool_of(registers, *value, "not")?)),
        EmirOp::IsFinite(value) => Ok(Value::Bool(
            f64_of(registers, *value, "is-finite")?.is_finite(),
        )),
        _ => unreachable!("op routed to the wrong eval family"),
    }
}
