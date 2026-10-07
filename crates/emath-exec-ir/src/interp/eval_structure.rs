use super::{EmirProgram, EmirOp, Value, ValueFrame, EvalBudget, EvalFault, register, format_text, f64_of, BTreeMap};

pub(super) fn eval_structure(
    _self_program: &EmirProgram,
    op: &EmirOp,
    registers: &[Value],
    _inputs: ValueFrame<'_>,
    _state: ValueFrame<'_>,
    _budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
        EmirOp::FormatText {
            template,
            arguments,
        } => {
            let values = arguments
                .iter()
                .map(|value| register(registers, *value).cloned())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Text(format_text(template, &values)))
        }
        EmirOp::RecordCreate { type_name, fields } => {
            let fields = fields
                .iter()
                .map(|(name, value)| Ok((name.clone(), register(registers, *value)?.clone())))
                .collect::<Result<BTreeMap<_, _>, EvalFault>>()?;
            Ok(Value::Record {
                type_name: type_name.clone(),
                fields,
            })
        }
        EmirOp::ListConcat(values) => {
            let mut parts = Vec::with_capacity(values.len());
            let mut count = 0usize;
            for value in values {
                let Value::List(items) = register(registers, *value)? else {
                    return Err(EvalFault::TypeConfusion {
                        register: value.0,
                        op: "list-concat",
                    });
                };
                count = count.checked_add(items.len()).ok_or(EvalFault::Arithmetic {
                    op: "list-concat",
                    detail: "concatenated length exceeds usize",
                })?;
                parts.push(items.clone());
            }
            let mut output = Vec::new();
            output
                .try_reserve_exact(count)
                .map_err(|_| EvalFault::CarrierRefused {
                    op: "list-concat",
                    detail: "list allocation exceeds available capacity".into(),
                })?;
            for items in parts {
                output.extend(items);
            }
            Ok(Value::List(output))
        }
        EmirOp::VectorLength(value) => match register(registers, *value)? {
            Value::Vector(values) => Ok(Value::I64(values.len() as i64)),
            Value::List(values) => Ok(Value::I64(values.len() as i64)),
            Value::Matrix { data, .. } => Ok(Value::I64(data.len() as i64)),
            Value::Tensor { data, .. } => Ok(Value::I64(data.len() as i64)),
            _ => Err(EvalFault::TypeConfusion {
                register: value.0,
                op: "vector-length",
            }),
        },
        EmirOp::IndexText(value) => Ok(Value::Text(
            (f64_of(registers, *value, "index-text")? as usize).to_string(),
        )),
        EmirOp::RefuseValue(value) => {
            let Value::Text(detail) = register(registers, *value)? else {
                return Err(EvalFault::TypeConfusion {
                    register: value.0,
                    op: "refuse-value",
                });
            };
            Err(EvalFault::CarrierRefused {
                op: "refuse",
                detail: detail.clone(),
            })
        }
        _ => unreachable!("op routed to the wrong eval family"),
    }
}
