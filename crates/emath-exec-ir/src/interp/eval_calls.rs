use super::*;

pub(super) fn eval_calls(
    self_program: &EmirProgram,
    op: &EmirOp,
    registers: &[Value],
    _inputs: ValueFrame<'_>,
    _state: ValueFrame<'_>,
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
        EmirOp::CallFrame {
            body,
            inputs,
            state,
            ..
        } => {
            if inputs.len() != usize::from(body.input_count)
                || state.len() != usize::from(body.state_count)
            {
                return Err(EvalFault::Arithmetic {
                    op: "call-frame",
                    detail: "frame argument count mismatch",
                });
            }
            let inputs = ValueFrame::mapped(registers, inputs)?;
            let state = ValueFrame::mapped(registers, state)?;
            evaluate_frames(body, inputs, state, budget)
        }
        EmirOp::CallSelf { inputs, .. } => {
            if inputs.len() != usize::from(self_program.input_count) {
                return Err(EvalFault::Arithmetic {
                    op: "call-self",
                    detail: "self argument count mismatch",
                });
            }
            let mapped = ValueFrame::mapped(registers, inputs)?;
            let values = mapped.to_vec();
            evaluate_frames_in(
                self_program,
                self_program,
                ValueFrame::direct(&values),
                ValueFrame::direct(&[]),
                budget,
            )
        }
        EmirOp::CallValue { program, inputs } => {
            let Value::Program(callee) = register(registers, *program)?.clone() else {
                return Err(EvalFault::TypeConfusion {
                    register: program.0,
                    op: "call-value",
                });
            };
            let mut arguments = Vec::with_capacity(inputs.len());
            for value in inputs {
                arguments.push(register(registers, *value)?.clone());
            }
            if arguments.is_empty() {
                return Err(EvalFault::Arithmetic {
                    op: "call-value",
                    detail: "call requires at least one argument",
                });
            }
            // Curried fold: apply one argument at a time; a partial
            // application yields another program value for the next
            // argument, so the house application form `f(a, b)` is one op.
            let mut result = Value::Program(callee);
            for argument in arguments {
                let Value::Program(current) = result else {
                    return Err(EvalFault::CarrierRefused {
                        op: "call-value",
                        detail: "curried application reached a non-program value".into(),
                    });
                };
                if current.vector_input || current.body.state_count != 0 {
                    return Err(EvalFault::Arithmetic {
                        op: "call-value",
                        detail: "expected a closed typed program",
                    });
                }
                let mut frame = Vec::with_capacity(1 + current.captures.len());
                frame.push(argument);
                frame.extend(current.captures.iter().cloned());
                if frame.len() != usize::from(current.body.input_count) {
                    return Err(EvalFault::Arithmetic {
                        op: "call-value",
                        detail: "program input count mismatch",
                    });
                }
                result = evaluate_with_budget(&current.body, &frame, &[], budget)?;
            }
            Ok(result)
        }
        EmirOp::DenseLayout(value) => register(registers, *value)?
            .dense_layout()
            .map(Value::DenseLayout)
            .ok_or(EvalFault::TypeConfusion {
                register: value.0,
                op: "dense-layout",
            }),
        EmirOp::VectorSlice {
            vector,
            offset,
            count,
        } => {
            let data = vector_of(registers, *vector, "vector-slice")?;
            let index = |value: EmirValue| match register(registers, value)? {
                Value::I64(value) => usize::try_from(*value).map_err(|_| EvalFault::Arithmetic {
                    op: "vector-slice",
                    detail: "slice index must be nonnegative and fit usize",
                }),
                _ => Err(EvalFault::TypeConfusion {
                    register: value.0,
                    op: "vector-slice",
                }),
            };
            let offset = index(*offset)?;
            let count = index(*count)?;
            let slice = offset
                .checked_add(count)
                .and_then(|end| data.get(offset..end))
                .ok_or(EvalFault::Arithmetic {
                    op: "vector-slice",
                    detail: "slice is outside vector storage",
                })?;
            Ok(Value::Vector(slice.to_vec()))
        }
        EmirOp::F64SortTotal(value) => {
            let mut values = vector_of(registers, *value, "f64-sort-total")?.to_vec();
            values.sort_by(f64::total_cmp);
            Ok(Value::Vector(values))
        }
        EmirOp::VectorConcat(values) => {
            let count = values.iter().try_fold(0usize, |count, value| {
                count
                    .checked_add(vector_of(registers, *value, "vector-concat")?.len())
                    .ok_or(EvalFault::Arithmetic {
                        op: "vector-concat",
                        detail: "concatenated length exceeds usize",
                    })
            })?;
            let mut output = Vec::new();
            output
                .try_reserve_exact(count)
                .map_err(|_| EvalFault::CarrierRefused {
                    op: "vector-concat",
                    detail: "vector allocation exceeds available capacity".into(),
                })?;
            for value in values {
                output.extend_from_slice(vector_of(registers, *value, "vector-concat")?);
            }
            Ok(Value::Vector(output))
        }
        EmirOp::SameDenseShape(left, right) => {
            let left = register(registers, *left)?;
            let right = register(registers, *right)?;
            Ok(Value::Bool(match (left, right) {
                (Value::DenseLayout(layout), value) | (value, Value::DenseLayout(layout)) => {
                    value.matches_dense_layout(layout)
                }
                (Value::F64(_) | Value::I64(_), Value::F64(_) | Value::I64(_)) => true,
                (Value::Vector(a), Value::Vector(b)) => a.len() == b.len(),
                (
                    Value::Matrix {
                        rows: ar,
                        cols: ac,
                        data: a,
                    },
                    Value::Matrix {
                        rows: br,
                        cols: bc,
                        data: b,
                    },
                ) => ar == br && ac == bc && a.len() == b.len(),
                (Value::Tensor { shape: a, data: av }, Value::Tensor { shape: b, data: bv }) => {
                    a == b && av.len() == bv.len()
                }
                _ => false,
            }))
        }
        EmirOp::ToF64(value) => register(registers, *value)?
            .as_real_f64()
            .map(Value::F64)
            .ok_or(EvalFault::TypeConfusion {
                register: value.0,
                op: "to-f64",
            }),
        EmirOp::DenseValues(value) => {
            let values = match register(registers, *value)? {
                Value::F64(value) => vec![*value],
                Value::I64(value) => vec![*value as f64],
                Value::Vector(data) | Value::Matrix { data, .. } | Value::Tensor { data, .. } => {
                    data.clone()
                }
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: value.0,
                        op: "dense-values",
                    });
                }
            };
            Ok(Value::Vector(values))
        }
        EmirOp::DenseRepack { template, data } => {
            let data = vector_of(registers, *data, "dense-repack")?;
            match register(registers, *template)? {
                Value::DenseLayout(layout) if layout.len() == data.len() => Ok(match layout {
                    emath_rt::DenseLayout::Scalar => Value::F64(data[0]),
                    emath_rt::DenseLayout::Vector(_) => Value::Vector(data.to_vec()),
                    emath_rt::DenseLayout::Matrix { rows, cols, .. } => Value::Matrix {
                        rows: *rows,
                        cols: *cols,
                        data: data.to_vec(),
                    },
                    emath_rt::DenseLayout::Tensor { shape, .. } => Value::Tensor {
                        shape: shape.clone(),
                        data: data.to_vec(),
                    },
                }),
                _ => Err(EvalFault::CarrierRefused {
                    op: "dense-repack",
                    detail: "numeric storage does not match template".into(),
                }),
            }
        }
        EmirOp::ProgramLiteral {
            body,
            captures,
            vector_input,
            ..
        } => Ok(Value::Program(ProgramValue {
            body: body.clone(),
            captures: captures
                .iter()
                .map(|value| register(registers, *value).cloned())
                .collect::<Result<_, _>>()?,
            vector_input: *vector_input,
        })),
        EmirOp::CallProgram { program, inputs }
        | EmirOp::CallScalarProgram { program, inputs }
        | EmirOp::CallRealProgram { program, inputs }
        | EmirOp::TryCallRealProgram { program, inputs } => {
            let Value::Program(body) = register(registers, *program)? else {
                return Err(EvalFault::TypeConfusion {
                    register: program.0,
                    op: op.name(),
                });
            };
            let numeric_inputs = vector_of(registers, *inputs, op.name())?;
            let argument_count = if body.vector_input {
                1
            } else {
                numeric_inputs.len()
            };
            if body.body.state_count != 0
                || argument_count.checked_add(body.captures.len())
                    != Some(usize::from(body.body.input_count))
            {
                return Err(EvalFault::Arithmetic {
                    op: op.name(),
                    detail: "expected a closed program with matching argument count",
                });
            }
            let mut arguments = Vec::with_capacity(usize::from(body.body.input_count));
            if body.vector_input {
                arguments.push(Value::Vector(numeric_inputs.to_vec()));
            } else {
                arguments.extend(numeric_inputs.iter().copied().map(Value::F64));
            }
            arguments.extend(body.captures.iter().cloned());
            let value = match evaluate_with_budget(&body.body, &arguments, &[], budget) {
                Ok(value) => value,
                Err(fault @ EvalFault::BudgetExhausted { .. }) => return Err(fault),
                Err(fault) if matches!(op, EmirOp::TryCallRealProgram { .. }) => {
                    return Ok(Value::Result {
                        ok: false,
                        payload: Box::new(Value::Text(fault.to_string())),
                    });
                }
                Err(fault) => return Err(fault),
            };
            match (op, value) {
                (EmirOp::CallProgram { .. }, Value::F64(value)) => Ok(Value::Vector(vec![value])),
                (EmirOp::CallProgram { .. }, value @ Value::Vector(_)) => Ok(value),
                (EmirOp::CallScalarProgram { .. }, value @ Value::F64(_)) => Ok(value),
                (EmirOp::CallRealProgram { .. }, value) => value
                    .as_real_f64()
                    .map(Value::F64)
                    .ok_or_else(|| EvalFault::CarrierRefused {
                        op: op.name(),
                        detail: "E-TYPE-012: program result must be a real scalar".into(),
                    }),
                (EmirOp::TryCallRealProgram { .. }, value) => Ok(match value.as_real_f64() {
                    Some(value) => Value::Result {
                        ok: true,
                        payload: Box::new(Value::F64(value)),
                    },
                    None => Value::Result {
                        ok: false,
                        payload: Box::new(Value::Text(
                            "E-TYPE-012: program result must be a real scalar".into(),
                        )),
                    },
                }),
                _ => Err(EvalFault::CarrierRefused {
                    op: op.name(),
                    detail: "E-TYPE-012: program result has the wrong numeric carrier".into(),
                }),
            }
        }

        _ => unreachable!("op routed to the wrong eval family"),
    }
}
