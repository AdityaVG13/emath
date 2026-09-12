//! Deterministic reference VM for the universal executable instruction set.

use std::cell::Cell;
use std::collections::BTreeMap;

use crate::term_compile::{CompiledCell, ResultGuard, run_guards};
use crate::{
    BuiltinId, CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget, FoldCombine, ReduceId,
    VectorScalarOp,
};

mod helpers;
mod value;

use helpers::*;
pub use value::{EvalFault, ProgramValue, Value, format_f64};

pub fn evaluate(
    program: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
) -> Result<Value, EvalFault> {
    evaluate_with_budget(program, inputs, state, EvalBudget::default())
}

pub fn evaluate_with_budget(
    program: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    evaluate_frames(
        program,
        ValueFrame::direct(inputs),
        ValueFrame::direct(state),
        budget,
    )
}

#[derive(Clone, Copy)]
struct ValueFrame<'a> {
    values: &'a [Value],
    slots: Option<&'a [EmirValue]>,
}

impl<'a> ValueFrame<'a> {
    fn direct(values: &'a [Value]) -> Self {
        Self {
            values,
            slots: None,
        }
    }
    fn mapped(values: &'a [Value], slots: &'a [EmirValue]) -> Result<Self, EvalFault> {
        for slot in slots {
            register(values, *slot)?;
        }
        Ok(Self {
            values,
            slots: Some(slots),
        })
    }
    fn get(self, index: usize) -> Option<&'a Value> {
        let index = match self.slots {
            Some(slots) => slots.get(index)?.0 as usize,
            None => index,
        };
        self.values.get(index)
    }
    fn to_vec(self) -> Vec<Value> {
        match self.slots {
            // mapped() validates every immutable slot before this frame is used.
            Some(slots) => slots
                .iter()
                .map(|slot| self.values[slot.0 as usize].clone())
                .collect(),
            None => self.values.to_vec(),
        }
    }
}

fn evaluate_frames(
    program: &EmirProgram,
    inputs: ValueFrame<'_>,
    state: ValueFrame<'_>,
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    evaluate_frames_in(program, program, inputs, state, budget)
}

fn evaluate_frames_in(
    self_program: &EmirProgram,
    program: &EmirProgram,
    inputs: ValueFrame<'_>,
    state: ValueFrame<'_>,
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    let _fuel = EvaluationFuel::enter(budget);
    let mut registers = Vec::with_capacity(program.ops.len());
    let mut applications = 0_u32;
    for (step, (op, span)) in program.ops.iter().enumerate() {
        let _site = crate::progress::site(*span, step);
        let executed = u32::try_from(step).unwrap_or(u32::MAX);
        if executed >= budget.max_steps {
            return Err(EvalFault::BudgetExhausted { executed });
        }
        if matches!(op, EmirOp::ApplyCapability { .. }) {
            applications = applications.saturating_add(1);
            if applications > budget.max_capability_applications {
                return Err(EvalFault::BudgetExhausted { executed });
            }
        }
        EvaluationFuel::charge(matches!(op, EmirOp::ApplyCapability { .. }))?;
        registers.push(eval_op(
            self_program,
            op,
            &registers,
            inputs,
            state,
            budget,
        )?);
    }
    register(&registers, program.result).cloned()
}

// Nested authored calls and loops share the outer evaluation budget.
#[derive(Clone, Copy)]
struct Fuel {
    steps: u32,
    applications: u32,
    executed: u32,
}
thread_local! { static FUEL: Cell<Option<Fuel>> = const { Cell::new(None) }; }
struct EvaluationFuel(bool);
impl EvaluationFuel {
    fn enter(budget: EvalBudget) -> Self {
        Self(FUEL.with(|slot| {
            if slot.get().is_some() {
                false
            } else {
                slot.set(Some(Fuel {
                    steps: budget.max_steps,
                    applications: budget.max_capability_applications,
                    executed: 0,
                }));
                true
            }
        }))
    }
    fn charge(application: bool) -> Result<(), EvalFault> {
        FUEL.with(|slot| {
            let Some(mut fuel) = slot.get() else {
                return Ok(());
            };
            if fuel.steps == 0 || (application && fuel.applications == 0) {
                return Err(EvalFault::BudgetExhausted {
                    executed: fuel.executed,
                });
            }
            fuel.steps -= 1;
            fuel.applications -= u32::from(application);
            fuel.executed += 1;
            slot.set(Some(fuel));
            Ok(())
        })
    }
}
impl Drop for EvaluationFuel {
    fn drop(&mut self) {
        if self.0 {
            FUEL.with(|slot| slot.set(None));
        }
    }
}

pub fn evaluate_f64(
    program: &EmirProgram,
    inputs: &[f64],
    state: &[f64],
) -> Result<Value, EvalFault> {
    let inputs = inputs.iter().copied().map(Value::F64).collect::<Vec<_>>();
    let state = state.iter().copied().map(Value::F64).collect::<Vec<_>>();
    evaluate(program, &inputs, &state)
}

fn eval_op(
    self_program: &EmirProgram,
    op: &EmirOp,
    registers: &[Value],
    inputs: ValueFrame<'_>,
    state: ValueFrame<'_>,
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
        EmirOp::ListCreate(elements) => elements
            .iter()
            .map(|value| register(registers, *value).cloned())
            .collect::<Result<Vec<_>, _>>()
            .map(Value::List),
        EmirOp::RecordField { record, field } => {
            let Value::Record { fields, .. } = register(registers, *record)? else {
                return Err(EvalFault::TypeConfusion {
                    register: record.0,
                    op: "record-field",
                });
            };
            fields
                .get(field)
                .cloned()
                .ok_or_else(|| EvalFault::CarrierRefused {
                    op: "record-field",
                    detail: format!("record has no field `{field}`"),
                })
        }
        EmirOp::Refuse(detail) => Err(EvalFault::CarrierRefused {
            op: "refuse",
            detail: detail.clone(),
        }),
        EmirOp::Branch {
            condition,
            args,
            then_body,
            else_body,
        } => {
            let selected = bool_of(registers, *condition, "branch")?;
            let _site = crate::progress::site(emath_core::Span::default(), usize::from(selected));
            let body = if selected { then_body } else { else_body };
            let arguments = args
                .iter()
                .map(|arg| register(registers, *arg).cloned())
                .collect::<Result<Vec<_>, _>>()?;
            evaluate_frames_in(
                self_program,
                body,
                ValueFrame::direct(&arguments),
                ValueFrame::direct(&[]),
                budget,
            )
        }
        EmirOp::Collect { count, args, body } => {
            let count = i64_of(registers, *count, "collect")?;
            let count = usize::try_from(count).map_err(|_| EvalFault::CarrierRefused {
                op: "collect",
                detail: "collection count must be nonnegative and fit usize".into(),
            })?;
            if count > budget.max_steps as usize {
                return Err(EvalFault::BudgetExhausted { executed: 0 });
            }
            let mut values = Vec::new();
            values
                .try_reserve_exact(count)
                .map_err(|_| EvalFault::CarrierRefused {
                    op: "collect",
                    detail: "collection allocation exceeds available capacity".into(),
                })?;
            let mut arguments = Vec::with_capacity(args.len() + 1);
            arguments.push(Value::I64(0));
            for value in args {
                arguments.push(register(registers, *value)?.clone());
            }
            for index in 0..count {
                let _site = crate::progress::site(emath_core::Span::default(), index);
                EvaluationFuel::charge(false)?;
                arguments[0] = Value::I64(index as i64);
                values.push(evaluate_with_budget(body, &arguments, &[], budget)?);
            }
            Ok(Value::List(values))
        }
        EmirOp::Iterate {
            count,
            init,
            args,
            stop,
            body,
        } => {
            let Value::I64(count_value) = register(registers, *count)? else {
                return Err(EvalFault::TypeConfusion {
                    register: count.0,
                    op: "iterate",
                });
            };
            if *count_value < 0 {
                return Err(EvalFault::Arithmetic {
                    op: "iterate",
                    detail: "iteration count must be nonnegative",
                });
            }
            let mut arguments = Vec::with_capacity(args.len() + 2);
            arguments.push(Value::I64(0));
            arguments.push(register(registers, *init)?.clone());
            for arg in args {
                arguments.push(register(registers, *arg)?.clone());
            }
            for index in 0..*count_value {
                let _site = crate::progress::site(emath_core::Span::default(), index as usize);
                arguments[0] = Value::I64(index);
                if let Some(stop) = stop {
                    let done = evaluate_with_budget(stop, &arguments, &[], budget)?;
                    if value_as_bool(&done, "iterate-stop")? {
                        break;
                    }
                }
                // Charge even an empty body so iteration cannot bypass fuel.
                EvaluationFuel::charge(false)?;
                arguments[1] = evaluate_with_budget(body, &arguments, &[], budget)?;
            }
            Ok(arguments.swap_remove(1))
        }
        EmirOp::ConstF64(bits) => Ok(Value::F64(f64::from_bits(*bits))),
        EmirOp::ConstI64(value) => Ok(Value::I64(*value)),
        EmirOp::ConstBigInt(value) => Value::parse_bigint(value).ok_or(EvalFault::Arithmetic {
            op: "const-bigint",
            detail: "invalid bounded integer constant",
        }),
        EmirOp::ConstText(value) => Ok(Value::Text(value.clone())),
        EmirOp::ConstComplex(re, im) => Ok(Value::Complex { re: *re, im: *im }),
        EmirOp::ConstBool(value) => Ok(Value::Bool(*value)),
        EmirOp::LoadInput(index) => inputs
            .get(usize::from(*index))
            .cloned()
            .ok_or(EvalFault::MissingInput(*index)),
        EmirOp::LoadState(index) => state
            .get(usize::from(*index))
            .cloned()
            .ok_or(EvalFault::MissingState(*index)),
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
        EmirOp::IntegerQuotient(left, right) => i64_of(registers, *left, "integer-quotient")?
            .checked_div(i64_of(registers, *right, "integer-quotient")?)
            .map(Value::I64)
            .ok_or(EvalFault::Arithmetic {
                op: "integer-quotient",
                detail: "E-INTEGER-QUOTIENT: zero divisor or i64 overflow",
            }),
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
        EmirOp::F64Exp2(value) => Ok(Value::F64(f64_of(registers, *value, "exp2")?.exp2())),
        EmirOp::F64PowI(base, exponent) => {
            let Value::I64(exponent) = register(registers, *exponent)? else {
                return Err(EvalFault::TypeConfusion {
                    register: exponent.0,
                    op: "powi",
                });
            };
            let exponent = i32::try_from(*exponent).map_err(|_| EvalFault::Arithmetic {
                op: "powi",
                detail: "E-SCALAR-CONVERT: exponent exceeds i32",
            })?;
            Ok(Value::F64(f64_of(registers, *base, "powi")?.powi(exponent)))
        }
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
        EmirOp::TensorShape(value) => {
            let (shape, _) = tensor_of(registers, *value, "tensor-shape")?;
            shape
                .iter()
                .map(|axis| {
                    i64::try_from(*axis)
                        .map(Value::I64)
                        .map_err(|_| EvalFault::Arithmetic {
                            op: "tensor-shape",
                            detail: "tensor dimension exceeds Int",
                        })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List)
        }
        EmirOp::TensorPack { shape, data } => {
            let fault = || EvalFault::Arithmetic {
                op: "tensor-pack",
                detail: "E-TENSOR-SHAPE: invalid dimensions or data length",
            };
            let Value::List(axes) = register(registers, *shape)? else {
                return Err(fault());
            };
            let shape = axes
                .iter()
                .map(|axis| match axis {
                    Value::I64(axis) => usize::try_from(*axis).map_err(|_| fault()),
                    _ => Err(fault()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let data = vector_of(registers, *data, "tensor-pack")?;
            if shape
                .iter()
                .try_fold(1usize, |size, axis| size.checked_mul(*axis))
                != Some(data.len())
            {
                return Err(fault());
            }
            Ok(Value::Tensor {
                shape,
                data: data.to_vec(),
            })
        }
        EmirOp::DenseIndex { dense, index } => {
            let data = match register(registers, *dense)? {
                Value::Vector(data) => data.as_slice(),
                Value::Matrix { .. } => matrix_of(registers, *dense, "dense-index")?.2,
                Value::Tensor { .. } => tensor_of(registers, *dense, "dense-index")?.1,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: dense.0,
                        op: "dense-index",
                    });
                }
            };
            let index = checked_index(registers, *index, data.len(), "dense-index")?;
            Ok(Value::F64(data[index]))
        }
        EmirOp::TensorCreate { shape, elements } => Ok(Value::Tensor {
            shape: shape.clone(),
            data: elements
                .iter()
                .map(|value| f64_of(registers, *value, "tensor-create"))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        EmirOp::VectorIndex { vector, index } => {
            if let Value::List(values) = register(registers, *vector)? {
                let index = checked_index(registers, *index, values.len(), "vector-index")?;
                return Ok(values[index].clone());
            }
            let values = vector_of(registers, *vector, "vector-index")?;
            let index = checked_index(registers, *index, values.len(), "vector-index")?;
            Ok(Value::F64(values[index]))
        }
        EmirOp::MatrixIndex { matrix, row, col } => {
            let (rows, cols, values) = matrix_of(registers, *matrix, "matrix-index")?;
            let row = checked_index(registers, *row, rows, "matrix-index")?;
            let col = checked_index(registers, *col, cols, "matrix-index")?;
            Ok(Value::F64(values[row * cols + col]))
        }
        EmirOp::TensorIndex { tensor, indices } => {
            let (shape, data) = tensor_of(registers, *tensor, "tensor-index")?;
            let mut offset = 0_usize;
            for (axis, index) in indices.iter().enumerate() {
                let len = shape
                    .get(axis)
                    .copied()
                    .ok_or(EvalFault::IndexOutOfBounds {
                        op: "tensor-index",
                        index: axis as i64,
                        len: shape.len(),
                    })?;
                offset = offset * len + checked_index(registers, *index, len, "tensor-index")?;
            }
            data.get(offset)
                .copied()
                .map(Value::F64)
                .ok_or(EvalFault::IndexOutOfBounds {
                    op: "tensor-index",
                    index: offset as i64,
                    len: data.len(),
                })
        }
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
        EmirOp::Fold {
            start,
            end,
            init,
            combine,
            loop_var_index,
            body,
        } => eval_fold(
            registers,
            inputs,
            state,
            *start,
            *end,
            *init,
            *combine,
            *loop_var_index,
            body,
        ),
        EmirOp::ApplyCapability {
            capability,
            class,
            args,
        } => apply_capability(capability, *class, args, registers, budget),
        EmirOp::VectorMap { builtin, source } => {
            let values = vector_of(registers, *source, "vector-map")?
                .iter()
                .map(|value| builtin.eval_unary(*value))
                .collect::<Option<Vec<_>>>()
                .ok_or(EvalFault::Arithmetic {
                    op: "vector-map",
                    detail: "binary scalar opcode used in map instruction",
                })?;
            Ok(Value::Vector(values))
        }
        EmirOp::VectorMapScalar { op, vector, scalar } => {
            let scalar = f64_of(registers, *scalar, "vector-map-scalar")?;
            let values = vector_of(registers, *vector, "vector-map-scalar")?;
            Ok(Value::Vector(
                values
                    .iter()
                    .map(|value| match op {
                        VectorScalarOp::Add => *value + scalar,
                        VectorScalarOp::Sub => *value - scalar,
                        VectorScalarOp::Mul => *value * scalar,
                        VectorScalarOp::Div => *value / scalar,
                    })
                    .collect(),
            ))
        }
        EmirOp::VectorReduce { reduce, source } => {
            let values = vector_of(registers, *source, "vector-reduce")?;
            let Some(first) = values.first().copied() else {
                return Err(EvalFault::Arithmetic {
                    op: "vector-reduce",
                    detail: "empty vector",
                });
            };
            let value = match reduce {
                ReduceId::Sum => values.iter().sum(),
                ReduceId::Max => values.iter().copied().fold(first, f64::max),
                ReduceId::Min => values.iter().copied().fold(first, f64::min),
            };
            Ok(Value::F64(value))
        }
        EmirOp::VectorAllFinite(source) => Ok(Value::Bool(
            vector_of(registers, *source, "vector-all-finite")?
                .iter()
                .all(|value| value.is_finite()),
        )),
        EmirOp::ToF64Vector(sequence) => match register(registers, *sequence)? {
            value @ Value::Vector(_) => Ok(value.clone()),
            Value::List(values) => values
                .iter()
                .map(|value| match value {
                    Value::F64(value) => Ok(*value),
                    _ => Err(EvalFault::TypeConfusion {
                        register: sequence.0,
                        op: "to-f64-vector",
                    }),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Vector),
            _ => Err(EvalFault::TypeConfusion {
                register: sequence.0,
                op: "to-f64-vector",
            }),
        },
        EmirOp::CallFrame {
            body,
            inputs,
            state,
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
        EmirOp::CallSelf { inputs } => {
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
    }
}

thread_local! {
    /// Depth of ongoing reference-cell evaluations. Bounds recursive
    /// capability bodies by the same budget that bounds flat applications:
    /// each nested reference dispatch counts one level.
    static REFERENCE_DEPTH: Cell<u32> = const { Cell::new(0) };
}

fn apply_capability(
    capability: &str,
    class: CellClass,
    args: &[EmirValue],
    registers: &[Value],
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    if class != CellClass::Pure {
        return Err(EvalFault::ProviderCallRequired {
            capability: capability.to_string(),
            args: args.len(),
        });
    }
    let values = args
        .iter()
        .map(|value| register(registers, *value).cloned())
        .collect::<Result<Vec<_>, _>>()?;
    crate::progress::apply(capability, &values, || {
        dispatch_capability(capability, &values, budget)
    })
}

fn dispatch_capability(
    capability: &str,
    values: &[Value],
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    // A verified installed native binding is the fast path; it is preferred
    // when present because the install validated the capsule contract.
    if let Some(kernel) = crate::native_kernel::native_kernel(capability) {
        if !kernel.admits_arity(values.len()) {
            return Err(EvalFault::Arithmetic {
                op: "apply-capability",
                detail: "capability argument count does not match capsule contract",
            });
        }
        return (kernel.handler)(values).map_err(|code| EvalFault::CapabilityRefused {
            capability: capability.to_string(),
            code,
        });
    }
    // Fallback: the installed authored reference cell (Language Image
    // reference data). Nothing else matches — no feature-name lookup, no
    // std-cell registry.
    if let Some(cell) = crate::native_kernel::installed_reference_cell(capability) {
        return apply_reference_cell(capability, &cell, values, budget);
    }
    Err(EvalFault::Arithmetic {
        op: "apply-capability",
        detail: "no installed reference bytecode or native kernel",
    })
}

/// Execute an installed authored reference cell: arity and defaults from the
/// declared params, contract guards at the seam, generic bytecode with
/// budget accounting, and the optional post-body certificate guard.
fn apply_reference_cell(
    capability: &str,
    cell: &CompiledCell,
    values: &[Value],
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    if !cell.admits_arity(values.len()) {
        return Err(EvalFault::Arithmetic {
            op: "apply-capability",
            detail: "capability argument count does not match capsule contract",
        });
    }
    let mut normalized;
    let values = if values.len() < cell.params.len() {
        normalized = Vec::with_capacity(cell.params.len());
        normalized.extend_from_slice(values);
        let first_default = cell.params.len() - cell.defaults.len();
        for default in &cell.defaults[values.len() - first_default..] {
            normalized.push(evaluate_with_budget(default, &normalized, &[], budget)?);
        }
        normalized.as_slice()
    } else {
        values
    };
    run_guards(capability, &cell.guards, values)?;
    let entered = REFERENCE_DEPTH.with(|depth| {
        let next = depth.get() + 1;
        if next > budget.max_capability_applications {
            None
        } else {
            depth.set(next);
            Some(())
        }
    });
    let Some(()) = entered else {
        return Err(EvalFault::BudgetExhausted {
            executed: budget.max_capability_applications,
        });
    };
    let evaluated = evaluate_with_budget(&cell.program, values, &[], budget);
    REFERENCE_DEPTH.with(|depth| depth.set(depth.get() - 1));
    let value = evaluated?;
    if let Some(guard) = cell.result_guard {
        enforce_result_guard(capability, guard, &value)?;
    }
    Ok(value)
}

fn enforce_result_guard(
    capability: &str,
    guard: ResultGuard,
    value: &Value,
) -> Result<(), EvalFault> {
    let ResultGuard::AllZero { code } = guard;
    let violated = match value {
        Value::Vector(elements) => elements.iter().any(|element| *element != 0.0),
        Value::Matrix { data, .. } => data.iter().any(|element| *element != 0.0),
        Value::F64(element) => *element != 0.0,
        Value::I64(element) => *element != 0,
        _ => {
            return Err(EvalFault::TypeConfusion {
                register: 0,
                op: "apply-capability",
            });
        }
    };
    if violated {
        Err(EvalFault::CapabilityRefused {
            capability: capability.to_string(),
            code: code.to_string(),
        })
    } else {
        Ok(())
    }
}

/// Type-preserving scalar addition (control mail 66): two `Value::I64`
/// operands add exactly via `i64::checked_add` and return `Value::I64`;
/// Float64 operands retain the existing strict-f64 behavior; any other
/// carrier mix is a typed confusion — never a silent coercion.
#[derive(Clone, Copy)]
enum ScalarOp {
    Add,
    Sub,
    Mul,
    Div,
}

fn apply_f64(left: f64, right: f64, kind: ScalarOp) -> f64 {
    match kind {
        ScalarOp::Add => left + right,
        ScalarOp::Sub => left - right,
        ScalarOp::Mul => left * right,
        ScalarOp::Div => left / right,
    }
}

fn scalar_arithmetic(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    kind: ScalarOp,
) -> Result<Value, EvalFault> {
    let left_value = register(registers, left)?;
    let right_value = register(registers, right)?;
    match (left_value, right_value) {
        (Value::Rat { num: an, den: ad }, Value::Rat { num: bn, den: bd }) => {
            let a = (*an, *ad);
            let b = (*bn, *bd);
            let result = match kind {
                ScalarOp::Add => emath_rt::ratio_add(a, b),
                ScalarOp::Sub => emath_rt::ratio_sub(a, b),
                ScalarOp::Mul => emath_rt::ratio_mul(a, b),
                ScalarOp::Div => emath_rt::ratio_div(a, b),
            };
            result
                .map(|(num, den)| Value::Rat { num, den })
                .map_err(|detail| EvalFault::CarrierRefused { op, detail })
        }
        (Value::I64(left), Value::I64(right)) => {
            let result = match kind {
                ScalarOp::Add => left.checked_add(*right).map(Value::I64),
                ScalarOp::Sub => left.checked_sub(*right).map(Value::I64),
                ScalarOp::Mul => left.checked_mul(*right).map(Value::I64),
                ScalarOp::Div => {
                    return emath_rt::ratio_div((i128::from(*left), 1), (i128::from(*right), 1))
                        .map(|(num, den)| Value::Rat { num, den })
                        .map_err(|detail| EvalFault::CarrierRefused { op, detail });
                }
            };
            result.ok_or(EvalFault::Arithmetic {
                op,
                detail: "i64 overflow",
            })
        }
        (Value::I64(left), Value::Rat { num, den }) => {
            let a = (i128::from(*left), 1);
            let b = (*num, *den);
            let result = match kind {
                ScalarOp::Add => emath_rt::ratio_add(a, b),
                ScalarOp::Sub => emath_rt::ratio_sub(a, b),
                ScalarOp::Mul => emath_rt::ratio_mul(a, b),
                ScalarOp::Div => emath_rt::ratio_div(a, b),
            };
            result
                .map(|(num, den)| Value::Rat { num, den })
                .map_err(|detail| EvalFault::CarrierRefused { op, detail })
        }
        (Value::Rat { num, den }, Value::I64(right)) => {
            let a = (*num, *den);
            let b = (i128::from(*right), 1);
            let result = match kind {
                ScalarOp::Add => emath_rt::ratio_add(a, b),
                ScalarOp::Sub => emath_rt::ratio_sub(a, b),
                ScalarOp::Mul => emath_rt::ratio_mul(a, b),
                ScalarOp::Div => emath_rt::ratio_div(a, b),
            };
            result
                .map(|(num, den)| Value::Rat { num, den })
                .map_err(|detail| EvalFault::CarrierRefused { op, detail })
        }
        (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
            let (a, b) = complex_parts(left_value).ok_or(EvalFault::TypeConfusion {
                register: left.0,
                op,
            })?;
            let (c, d) = complex_parts(right_value).ok_or(EvalFault::TypeConfusion {
                register: right.0,
                op,
            })?;
            let (re, im) = match kind {
                ScalarOp::Add => (a + c, b + d),
                ScalarOp::Sub => (a - c, b - d),
                ScalarOp::Mul => (a * c - b * d, a * d + b * c),
                ScalarOp::Div => {
                    let denominator = c * c + d * d;
                    ((a * c + b * d) / denominator, (b * c - a * d) / denominator)
                }
            };
            Ok(Value::Complex { re, im })
        }
        (Value::I64(left), Value::F64(right)) => {
            Ok(Value::F64(apply_f64(*left as f64, *right, kind)))
        }
        (Value::F64(left), Value::I64(right)) => {
            Ok(Value::F64(apply_f64(*left, *right as f64, kind)))
        }
        (Value::F64(left), Value::F64(right)) => Ok(Value::F64(apply_f64(*left, *right, kind))),
        (Value::Vector(left), Value::Vector(right)) if left.len() == right.len() => {
            Ok(Value::Vector(
                left.iter()
                    .zip(right.iter())
                    .map(|(a, b)| apply_f64(*a, *b, kind))
                    .collect(),
            ))
        }
        (Value::Vector(left), Value::F64(right)) => Ok(Value::Vector(
            left.iter().map(|a| apply_f64(*a, *right, kind)).collect(),
        )),
        (Value::F64(left), Value::Vector(right)) => Ok(Value::Vector(
            right.iter().map(|b| apply_f64(*left, *b, kind)).collect(),
        )),
        (
            Value::Matrix {
                rows,
                cols,
                data: left,
            },
            Value::Matrix {
                rows: right_rows,
                cols: right_cols,
                data: right,
            },
        ) if rows == right_rows && cols == right_cols => Ok(Value::Matrix {
            rows: *rows,
            cols: *cols,
            data: left
                .iter()
                .zip(right.iter())
                .map(|(a, b)| apply_f64(*a, *b, kind))
                .collect(),
        }),
        (Value::Matrix { rows, cols, data }, Value::F64(scalar)) => Ok(Value::Matrix {
            rows: *rows,
            cols: *cols,
            data: data.iter().map(|a| apply_f64(*a, *scalar, kind)).collect(),
        }),
        (Value::F64(scalar), Value::Matrix { rows, cols, data }) => Ok(Value::Matrix {
            rows: *rows,
            cols: *cols,
            data: data.iter().map(|a| apply_f64(*scalar, *a, kind)).collect(),
        }),
        (Value::Tensor { shape, data }, Value::F64(scalar)) => Ok(Value::Tensor {
            shape: shape.clone(),
            data: data.iter().map(|a| apply_f64(*a, *scalar, kind)).collect(),
        }),
        (Value::F64(scalar), Value::Tensor { shape, data }) => Ok(Value::Tensor {
            shape: shape.clone(),
            data: data.iter().map(|a| apply_f64(*scalar, *a, kind)).collect(),
        }),
        (
            Value::Tensor {
                shape: left_shape,
                data: left,
            },
            Value::Tensor {
                shape: right_shape,
                data: right,
            },
        ) if left_shape == right_shape => Ok(Value::Tensor {
            shape: left_shape.clone(),
            data: left
                .iter()
                .zip(right.iter())
                .map(|(a, b)| apply_f64(*a, *b, kind))
                .collect(),
        }),
        _ => Err(EvalFault::TypeConfusion {
            register: left.0,
            op,
        }),
    }
}

fn complex_parts(value: &Value) -> Option<(f64, f64)> {
    match value {
        Value::Complex { re, im } => Some((*re, *im)),
        Value::F64(value) => Some((*value, 0.0)),
        Value::I64(value) => Some((*value as f64, 0.0)),
        _ => None,
    }
}

fn scalar_neg(registers: &[Value], value: EmirValue) -> Result<Value, EvalFault> {
    match register(registers, value)? {
        Value::I64(value) => value
            .checked_neg()
            .map(Value::I64)
            .ok_or(EvalFault::Arithmetic {
                op: "neg",
                detail: "i64 overflow",
            }),
        Value::Rat { num, den } => emath_rt::ratio_sub((0, 1), (*num, *den))
            .map(|(num, den)| Value::Rat { num, den })
            .map_err(|detail| EvalFault::CarrierRefused { op: "neg", detail }),
        Value::F64(value) => Ok(Value::F64(-value)),
        Value::Complex { re, im } => Ok(Value::Complex { re: -*re, im: -*im }),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op: "neg",
        }),
    }
}

fn scalar_unary(
    registers: &[Value],
    value: EmirValue,
    builtin: BuiltinId,
) -> Result<Value, EvalFault> {
    if let Value::Complex { re, im } = register(registers, value)? {
        let result = match builtin {
            BuiltinId::Abs => return Ok(Value::F64(re.hypot(*im))),
            BuiltinId::Sqrt => emath_rt::complex_sqrt(*re, *im),
            BuiltinId::Ln => emath_rt::complex_ln(*re, *im),
            BuiltinId::Exp => emath_rt::complex_exp(*re, *im),
            BuiltinId::Log2 => {
                let (re, im) = emath_rt::complex_ln(*re, *im);
                (re / std::f64::consts::LN_2, im / std::f64::consts::LN_2)
            }
            BuiltinId::Log10 => {
                let (re, im) = emath_rt::complex_ln(*re, *im);
                (re / std::f64::consts::LN_10, im / std::f64::consts::LN_10)
            }
            _ => {
                return Err(EvalFault::TypeConfusion {
                    register: value.0,
                    op: "unary-kernel",
                });
            }
        };
        return Ok(Value::Complex {
            re: result.0,
            im: result.1,
        });
    }
    builtin
        .eval_unary(f64_of(registers, value, "unary-kernel")?)
        .map(Value::F64)
        .ok_or(EvalFault::Arithmetic {
            op: "unary-kernel",
            detail: "binary scalar opcode used in unary instruction",
        })
}

fn scalar_binary(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    evaluate: impl FnOnce(f64, f64) -> f64,
) -> Result<Value, EvalFault> {
    Ok(Value::F64(evaluate(
        f64_of(registers, left, op)?,
        f64_of(registers, right, op)?,
    )))
}

fn comparison(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    compare: impl FnOnce(f64, f64) -> bool,
) -> Result<Value, EvalFault> {
    match (register(registers, left)?, register(registers, right)?) {
        (Value::Rat { num: an, den: ad }, Value::Rat { num: bn, den: bd }) => {
            let a = (*an, *ad);
            let b = (*bn, *bd);
            let (left, right, negate) = match op {
                "lt" => (a, b, false),
                "le" => (b, a, true),
                "gt" => (b, a, false),
                "ge" => (a, b, true),
                _ => unreachable!("ordered comparison instruction"),
            };
            return emath_rt::ratio_lt(left, right)
                .map(|less| Value::Bool(less != negate))
                .map_err(|detail| EvalFault::CarrierRefused { op, detail });
        }
        (Value::I64(left), Value::I64(right)) => {
            return Ok(Value::Bool(compare_ordering(left.cmp(right), op)));
        }
        (Value::I64(left), Value::F64(right)) => {
            return Ok(Value::Bool(
                emath_rt::cmp_i64_f64(*left, *right)
                    .is_some_and(|ordering| compare_ordering(ordering, op)),
            ));
        }
        (Value::F64(left), Value::I64(right)) => {
            return Ok(Value::Bool(
                emath_rt::cmp_i64_f64(*right, *left)
                    .map(std::cmp::Ordering::reverse)
                    .is_some_and(|ordering| compare_ordering(ordering, op)),
            ));
        }
        _ => {}
    }
    Ok(Value::Bool(compare(
        f64_of(registers, left, op)?,
        f64_of(registers, right, op)?,
    )))
}

fn compare_ordering(ordering: std::cmp::Ordering, op: &'static str) -> bool {
    match op {
        "lt" => ordering.is_lt(),
        "le" => ordering.is_le(),
        "gt" => ordering.is_gt(),
        "ge" => ordering.is_ge(),
        _ => false,
    }
}

fn boolean_binary(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    evaluate: impl FnOnce(bool, bool) -> bool,
) -> Result<Value, EvalFault> {
    Ok(Value::Bool(evaluate(
        bool_of(registers, left, op)?,
        bool_of(registers, right, op)?,
    )))
}

fn checked_index(
    registers: &[Value],
    value: EmirValue,
    len: usize,
    op: &'static str,
) -> Result<usize, EvalFault> {
    let raw = i64_of(registers, value, op)?;
    usize::try_from(raw)
        .ok()
        .filter(|index| *index < len)
        .ok_or(EvalFault::IndexOutOfBounds {
            op,
            index: raw,
            len,
        })
}

fn eval_fold(
    registers: &[Value],
    inputs: ValueFrame<'_>,
    state: ValueFrame<'_>,
    start: EmirValue,
    end: EmirValue,
    init: EmirValue,
    combine: FoldCombine,
    loop_var_index: u16,
    body: &EmirProgram,
) -> Result<Value, EvalFault> {
    let start = i64_of(registers, start, "fold")?;
    let end = i64_of(registers, end, "fold")?;
    let mut accumulator = register(registers, init)?.clone();
    let mut body_inputs = inputs.to_vec();
    let slot = usize::from(loop_var_index);
    if body_inputs.len() <= slot {
        body_inputs.resize(slot + 1, Value::I64(0));
    }
    for (iteration, value) in (start..end).enumerate() {
        let _site = crate::progress::site(emath_core::Span::default(), iteration);
        body_inputs[slot] = Value::I64(value);
        let next = evaluate_frames(
            body,
            ValueFrame::direct(&body_inputs),
            state,
            EvalBudget::default(),
        )?;
        accumulator = match combine {
            FoldCombine::Add => {
                Value::F64(value_as_f64(&accumulator, "fold")? + value_as_f64(&next, "fold")?)
            }
            FoldCombine::Mul => {
                Value::F64(value_as_f64(&accumulator, "fold")? * value_as_f64(&next, "fold")?)
            }
            FoldCombine::And => {
                Value::Bool(value_as_bool(&accumulator, "fold")? && value_as_bool(&next, "fold")?)
            }
            FoldCombine::Or => {
                Value::Bool(value_as_bool(&accumulator, "fold")? || value_as_bool(&next, "fold")?)
            }
        };
    }
    Ok(accumulator)
}

fn value_as_f64(value: &Value, op: &'static str) -> Result<f64, EvalFault> {
    match value {
        Value::F64(value) => Ok(*value),
        Value::I64(value) => Ok(*value as f64),
        _ => Err(EvalFault::Arithmetic {
            op,
            detail: "fold carrier mismatch",
        }),
    }
}

fn value_as_bool(value: &Value, op: &'static str) -> Result<bool, EvalFault> {
    match value {
        Value::Bool(value) => Ok(*value),
        _ => Err(EvalFault::Arithmetic {
            op,
            detail: "fold carrier mismatch",
        }),
    }
}

fn format_text(template: &str, arguments: &[Value]) -> String {
    use std::fmt::Write;

    let mut output = String::with_capacity(template.len());
    let mut remaining = template;
    for value in arguments {
        let Some((prefix, suffix)) = remaining.split_once("{}") else {
            break;
        };
        output.push_str(prefix);
        write!(output, "{value}").expect("writing to a String cannot fail");
        remaining = suffix;
    }
    output.push_str(remaining);
    output
}

fn sample_series(
    points: &[(f64, f64)],
    interpolation: &str,
    extrapolation: &str,
    time: f64,
) -> Result<f64, EvalFault> {
    let Some(&(start, start_value)) = points.first() else {
        return Err(EvalFault::Arithmetic {
            op: "series-sample",
            detail: "series has no support points",
        });
    };
    let &(end, end_value) = points.last().expect("nonempty checked");
    let after_end = time > end;
    if time < start || after_end {
        match extrapolation {
            "refuse" => {
                return Err(EvalFault::SeriesOutOfSupport {
                    time_bits: time.to_bits(),
                    start_bits: start.to_bits(),
                    end_bits: end.to_bits(),
                });
            }
            "clamp" => return Ok(if time < start { start_value } else { end_value }),
            "extend" => {
                if after_end {
                    match interpolation {
                        "previous" | "pwc" | "nearest" => return Ok(end_value),
                        _ => {}
                    }
                }
            }
            _ => {
                return Err(EvalFault::Arithmetic {
                    op: "series-sample",
                    detail: "unknown extrapolation policy",
                });
            }
        }
    }
    if points.len() == 1 || time == end {
        return Ok(end_value);
    }
    let index = if time <= start {
        0
    } else if time >= end {
        points.len() - 2
    } else {
        points
            .windows(2)
            .position(|window| time >= window[0].0 && time < window[1].0)
            .expect("strictly increasing support brackets interior time")
    };
    let (left_time, left_value) = points[index];
    let (right_time, right_value) = points[index + 1];
    let alpha = (time - left_time) / (right_time - left_time);
    match interpolation {
        "previous" | "pwc" => Ok(left_value),
        "nearest" => Ok(if alpha < 0.5 { left_value } else { right_value }),
        "linear" => Ok(left_value + alpha * (right_value - left_value)),
        "monotone_cubic" => {
            let secant = (right_value - left_value) / (right_time - left_time);
            let left_slope = if index == 0 {
                secant
            } else {
                let prior = (left_value - points[index - 1].1) / (left_time - points[index - 1].0);
                if prior.signum() == secant.signum() {
                    0.5 * (prior + secant)
                } else {
                    0.0
                }
            };
            let right_slope = if index + 2 == points.len() {
                secant
            } else {
                let next = (points[index + 2].1 - right_value) / (points[index + 2].0 - right_time);
                if next.signum() == secant.signum() {
                    0.5 * (secant + next)
                } else {
                    0.0
                }
            };
            let h = right_time - left_time;
            let a2 = alpha * alpha;
            let a3 = a2 * alpha;
            Ok((2.0 * a3 - 3.0 * a2 + 1.0) * left_value
                + (a3 - 2.0 * a2 + alpha) * h * left_slope
                + (-2.0 * a3 + 3.0 * a2) * right_value
                + (a3 - a2) * h * right_slope)
        }
        _ => Err(EvalFault::Arithmetic {
            op: "series-sample",
            detail: "unknown interpolation policy",
        }),
    }
}
