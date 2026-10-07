use super::{EmirProgram, EmirOp, Value, ValueFrame, EvalBudget, EvalFault};

pub(super) fn eval_const(
    _self_program: &EmirProgram,
    op: &EmirOp,
    _registers: &[Value],
    inputs: ValueFrame<'_>,
    _state: ValueFrame<'_>,
    _budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
        EmirOp::ConstF64(bits) => Ok(Value::F64(f64::from_bits(*bits))),
        EmirOp::ConstI64(value) => Ok(Value::I64(*value)),
        EmirOp::ConstExactInt(text) => emath_rt::ExactInt::parse(text)
            .map(Value::ExactInt)
            .map_err(|err| EvalFault::CarrierRefused {
                op: "const-exact-int",
                detail: err.to_string(),
            }),
        EmirOp::ConstText(value) => Ok(Value::Text(value.clone())),
        EmirOp::ConstBool(value) => Ok(Value::Bool(*value)),
        EmirOp::LoadInput(index) => inputs
            .get(usize::from(*index))
            .cloned()
            .ok_or(EvalFault::MissingInput(*index)),
        _ => unreachable!("op routed to the wrong eval family"),
    }
}
