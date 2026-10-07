use super::{EmirProgram, EmirOp, Value, ValueFrame, EvalBudget, EvalFault, eval_tensor_slice, register};

pub(super) fn eval_option_result(
    _self_program: &EmirProgram,
    op: &EmirOp,
    registers: &[Value],
    _inputs: ValueFrame<'_>,
    _state: ValueFrame<'_>,
    _budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
        EmirOp::TensorSlice { tensor, axes } => {
            eval_tensor_slice(registers, *tensor, axes, "tensor-slice")
        }
        EmirOp::ResultIsOk(value) => match register(registers, *value)? {
            Value::Result { ok, .. } => Ok(Value::Bool(*ok)),
            _ => Err(EvalFault::TypeConfusion {
                register: value.0,
                op: "result-is-ok",
            }),
        },
        EmirOp::ResultUnwrapOr(value, default) => match register(registers, *value)? {
            Value::Result { ok: true, payload } => Ok((**payload).clone()),
            Value::Result { ok: false, .. } => register(registers, *default).cloned(),
            _ => Err(EvalFault::TypeConfusion {
                register: value.0,
                op: "result-unwrap-or",
            }),
        },
        _ => unreachable!("op routed to the wrong eval family"),
    }
}
