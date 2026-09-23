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
        EmirOp::OptionSome(value) => Ok(Value::Option(Some(Box::new(
            register(registers, *value)?.clone(),
        )))),
        EmirOp::OptionNone => Ok(Value::Option(None)),
        EmirOp::OptionIsSome(value) => match register(registers, *value)? {
            Value::Option(value) => Ok(Value::Bool(value.is_some())),
            _ => Err(EvalFault::TypeConfusion {
                register: value.0,
                op: "option-is-some",
            }),
        },
        EmirOp::OptionUnwrapOr(value, default) => match register(registers, *value)? {
            Value::Option(Some(value)) => Ok((**value).clone()),
            Value::Option(None) => register(registers, *default).cloned(),
            _ => Err(EvalFault::TypeConfusion {
                register: value.0,
                op: "option-unwrap-or",
            }),
        },
        EmirOp::ResultOk(value) => Ok(Value::Result {
            ok: true,
            payload: Box::new(register(registers, *value)?.clone()),
        }),
        EmirOp::ResultErr(value) => Ok(Value::Result {
            ok: false,
            payload: Box::new(register(registers, *value)?.clone()),
        }),
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
        EmirOp::ResultErrorOf(value) => match register(registers, *value)? {
            Value::Result { ok: true, .. } => Ok(Value::Option(None)),
            Value::Result { ok: false, payload } => Ok(Value::Option(Some(payload.clone()))),
            _ => Err(EvalFault::TypeConfusion {
                register: value.0,
                op: "result-error-of",
            }),
        },
        _ => unreachable!("op routed to the wrong eval family"),
    }
}
