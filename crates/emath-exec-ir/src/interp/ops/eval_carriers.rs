//! Option/Result carrier op evaluation.

use super::*;

pub(super) fn eval_carrier_op(
    op: &EmirOp,
    registers: &[Value],
    name: &'static str,
) -> Result<Value, EvalFault> {
    match *op {
        EmirOp::OptionSome(value) => Ok(Value::Option(Some(Box::new(
            registers[value.0 as usize].clone(),
        )))),
        EmirOp::OptionNone => Ok(Value::Option(None)),
        EmirOp::OptionIsSome(option) => {
            let Value::Option(inner) = &registers[option.0 as usize] else {
                return Err(EvalFault::TypeConfusion {
                    register: option.0,
                    op: name,
                });
            };
            Ok(Value::Bool(inner.is_some()))
        }
        EmirOp::OptionUnwrapOr(option, default) => {
            let Value::Option(inner) = &registers[option.0 as usize] else {
                return Err(EvalFault::TypeConfusion {
                    register: option.0,
                    op: name,
                });
            };
            match inner {
                Some(value) => Ok((**value).clone()),
                None => Ok(registers[default.0 as usize].clone()),
            }
        }
        EmirOp::ResultOk(value) => Ok(Value::Result {
            ok: true,
            payload: Box::new(registers[value.0 as usize].clone()),
        }),
        EmirOp::ResultErr(error) => Ok(Value::Result {
            ok: false,
            payload: Box::new(registers[error.0 as usize].clone()),
        }),
        EmirOp::ResultIsOk(result) => {
            let Value::Result { ok, .. } = &registers[result.0 as usize] else {
                return Err(EvalFault::TypeConfusion {
                    register: result.0,
                    op: name,
                });
            };
            Ok(Value::Bool(*ok))
        }
        EmirOp::ResultUnwrapOr(result, default) => {
            let Value::Result { ok, payload } = &registers[result.0 as usize] else {
                return Err(EvalFault::TypeConfusion {
                    register: result.0,
                    op: name,
                });
            };
            if *ok {
                Ok((**payload).clone())
            } else {
                Ok(registers[default.0 as usize].clone())
            }
        }
        EmirOp::ResultErrorOf(result) => {
            // The error as an OPTION: Ok → None, Err → Some(error)
            // (Result errors compose with the Option ops).
            let Value::Result { ok, payload } = &registers[result.0 as usize] else {
                return Err(EvalFault::TypeConfusion {
                    register: result.0,
                    op: name,
                });
            };
            if *ok {
                Ok(Value::Option(None))
            } else {
                Ok(Value::Option(Some(payload.clone())))
            }
        }
        _ => unreachable!("eval_carrier_op routed a non-matching op"),
    }
}
