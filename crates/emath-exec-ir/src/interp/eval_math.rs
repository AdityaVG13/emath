use super::{EmirProgram, EmirOp, Value, ValueFrame, EvalBudget, EvalFault, f64_of, register, checked_index, vector_of};

pub(super) fn eval_math(
    _self_program: &EmirProgram,
    op: &EmirOp,
    registers: &[Value],
    _inputs: ValueFrame<'_>,
    _state: ValueFrame<'_>,
    _budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
        EmirOp::TextTrim(value) | EmirOp::TextLength(value) | EmirOp::ParseF64(value) => {
            let Value::Text(text) = register(registers, *value)? else {
                return Err(EvalFault::TypeConfusion {
                    register: value.0,
                    op: "text",
                });
            };
            match op {
                EmirOp::TextTrim(_) => Ok(Value::Text(text.trim().to_owned())),
                EmirOp::TextLength(_) => {
                    i64::try_from(text.len())
                        .map(Value::I64)
                        .map_err(|_| EvalFault::Arithmetic {
                            op: "text-length",
                            detail: "text length exceeds Int",
                        })
                }
                _ => text
                    .parse::<f64>()
                    .map(Value::F64)
                    .map_err(|_| EvalFault::Arithmetic {
                        op: "parse-f64",
                        detail: "E-SCALAR-CONVERT: invalid Float64 text",
                    }),
            }
        }
        EmirOp::TextByte(text, index) => {
            let Value::Text(text) = register(registers, *text)? else {
                return Err(EvalFault::TypeConfusion {
                    register: text.0,
                    op: "text-byte",
                });
            };
            let index = checked_index(registers, *index, text.len(), "text-byte")?;
            Ok(Value::I64(i64::from(text.as_bytes()[index])))
        }
        EmirOp::FormatScientific(value, precision) => {
            let Value::I64(precision) = register(registers, *precision)? else {
                return Err(EvalFault::TypeConfusion {
                    register: precision.0,
                    op: "format-scientific",
                });
            };
            emath_rt::format_scientific(f64_of(registers, *value, "format-scientific")?, *precision)
                .map(Value::Text)
                .map_err(|detail| EvalFault::Arithmetic {
                    op: "format-scientific",
                    detail,
                })
        }
        EmirOp::VectorIndex { vector, index } => {
            if let Value::List(values) = register(registers, *vector)? {
                let index = checked_index(registers, *index, values.len(), "vector-index")?;
                return Ok(values[index].clone());
            }
            let values = vector_of(registers, *vector, "vector-index")?;
            let index = checked_index(registers, *index, values.len(), "vector-index")?;
            Ok(Value::F64(values[index]))
        }
        _ => unreachable!("op routed to the wrong eval family"),
    }
}
