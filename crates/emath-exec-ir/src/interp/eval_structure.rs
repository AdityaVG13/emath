use super::*;

pub(super) fn eval_structure(
    _self_program: &EmirProgram,
    op: &EmirOp,
    registers: &[Value],
    _inputs: ValueFrame<'_>,
    _state: ValueFrame<'_>,
    _budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => {
            let selected = if bool_of(registers, *condition, "select")? {
                *then_value
            } else {
                *else_value
            };
            register(registers, selected).cloned()
        }
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
        EmirOp::SeriesCreate {
            points,
            interpolation,
            extrapolation,
        } => Ok(Value::Series {
            points: points.clone(),
            interpolation: interpolation.clone(),
            extrapolation: extrapolation.clone(),
        }),
        EmirOp::SeriesSample { series, time } => {
            let Value::Series {
                points,
                interpolation,
                extrapolation,
            } = register(registers, *series)?
            else {
                return Err(EvalFault::TypeConfusion {
                    register: series.0,
                    op: "series-sample",
                });
            };
            sample_series(
                points,
                interpolation,
                extrapolation,
                f64_of(registers, *time, "series-sample")?,
            )
            .map(Value::F64)
        }
        EmirOp::SetCreate { elements, guards } => {
            let mut values = Vec::new();
            for (index, element) in elements.iter().enumerate() {
                let include = match guards.get(index).copied().flatten() {
                    Some(guard) => bool_of(registers, guard, "set-create")?,
                    None => true,
                };
                if include {
                    let value = register(registers, *element)?.clone();
                    if !values.iter().any(|present| present == &value) {
                        values.push(value);
                    }
                }
            }
            Ok(Value::Set(values))
        }
        EmirOp::SetContains { element, set } => {
            let Value::Set(values) = register(registers, *set)? else {
                return Err(EvalFault::TypeConfusion {
                    register: set.0,
                    op: "set-contains",
                });
            };
            let element = register(registers, *element)?;
            Ok(Value::Bool(values.iter().any(|value| value == element)))
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
        EmirOp::VectorLength(value) => match register(registers, *value)? {
            Value::Vector(values) => Ok(Value::I64(values.len() as i64)),
            Value::List(values) => Ok(Value::I64(values.len() as i64)),
            Value::Matrix { data, .. } => Ok(Value::I64(data.len() as i64)),
            Value::Tensor { data, .. } => Ok(Value::I64(data.len() as i64)),
            Value::BigVector(values) => Ok(Value::I64(values.len() as i64)),
            Value::DenseLayout(layout) => Ok(Value::I64(layout.len() as i64)),
            _ => Err(EvalFault::TypeConfusion {
                register: value.0,
                op: "vector-length",
            }),
        },
        EmirOp::VectorCreate(elements) => {
            if elements
                .first()
                .is_some_and(|v| matches!(registers.get(v.0 as usize), Some(Value::Rat { .. })))
            {
                elements
                    .iter()
                    .map(|v| match register(registers, *v)? {
                        value @ Value::Rat { .. } => Ok(value.clone()),
                        _ => Err(EvalFault::TypeConfusion {
                            register: v.0,
                            op: "vector-create",
                        }),
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(Value::List)
            } else {
                elements
                    .iter()
                    .map(|v| f64_of(registers, *v, "vector-create"))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Value::Vector)
            }
        }
        EmirOp::MatrixCreate {
            rows,
            cols,
            elements,
        } => Ok(Value::Matrix {
            rows: *rows,
            cols: *cols,
            data: elements
                .iter()
                .map(|value| f64_of(registers, *value, "matrix-create"))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        EmirOp::MatrixRows(value) | EmirOp::MatrixCols(value) => {
            let Value::Matrix { rows, cols, .. } = register(registers, *value)? else {
                return Err(EvalFault::TypeConfusion {
                    register: value.0,
                    op: "matrix-shape",
                });
            };
            // Metadata access lets authored guards validate the stored length first.
            let size = if matches!(op, EmirOp::MatrixRows(_)) {
                *rows
            } else {
                *cols
            };
            i64::try_from(size)
                .map(Value::I64)
                .map_err(|_| EvalFault::Arithmetic {
                    op: "matrix-shape",
                    detail: "matrix dimension exceeds Int",
                })
        }
        EmirOp::MatrixPack { rows, cols, data } => {
            let fault = || EvalFault::Arithmetic {
                op: "matrix-pack",
                detail: "E-MATRIX-SHAPE: invalid dimensions or data length",
            };
            let rows =
                usize::try_from(i64_of(registers, *rows, "matrix-pack")?).map_err(|_| fault())?;
            let cols =
                usize::try_from(i64_of(registers, *cols, "matrix-pack")?).map_err(|_| fault())?;
            let values = vector_of(registers, *data, "matrix-pack")?;
            if rows.checked_mul(cols) != Some(values.len()) {
                return Err(fault());
            }
            Ok(Value::Matrix {
                rows,
                cols,
                data: values.to_vec(),
            })
        }
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
