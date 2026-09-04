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
pub use value::{EvalFault, Value, format_f64};

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
    let mut registers = Vec::with_capacity(program.ops.len());
    let mut applications = 0_u32;
    for (step, (op, _)) in program.ops.iter().enumerate() {
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
        registers.push(eval_op(op, &registers, inputs, state, budget)?);
    }
    register(&registers, program.result).cloned()
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
    op: &EmirOp,
    registers: &[Value],
    inputs: &[Value],
    state: &[Value],
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
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
        EmirOp::F64Add(left, right) => scalar_arithmetic(registers, *left, *right, "f64-add", ScalarOp::Add),
        EmirOp::F64Sub(left, right) => scalar_arithmetic(registers, *left, *right, "f64-sub", ScalarOp::Sub),
        EmirOp::F64Mul(left, right) => scalar_arithmetic(registers, *left, *right, "f64-mul", ScalarOp::Mul),
        EmirOp::F64Div(left, right) => scalar_arithmetic(registers, *left, *right, "f64-div", ScalarOp::Div),
        EmirOp::F64Pow(left, right) => scalar_binary(registers, *left, *right, "pow", f64::powf),
        EmirOp::Neg(value) => scalar_neg(registers, *value),
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
        EmirOp::VectorCreate(elements) => Ok(Value::Vector(
            elements
                .iter()
                .map(|value| f64_of(registers, *value, "vector-create"))
                .collect::<Result<Vec<_>, _>>()?,
        )),
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
        EmirOp::TensorCreate { shape, elements } => Ok(Value::Tensor {
            shape: shape.clone(),
            data: elements
                .iter()
                .map(|value| f64_of(registers, *value, "tensor-create"))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        EmirOp::VectorIndex { vector, index } => {
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
        EmirOp::ProgramLiteral(program) => Ok(Value::Program(program.clone())),
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
    // A verified installed native binding is the fast path; it is preferred
    // when present because the install validated the capsule contract.
    if let Some(kernel) = crate::native_kernel::native_kernel(capability) {
        if !kernel.admits_arity(values.len()) {
            return Err(EvalFault::Arithmetic {
                op: "apply-capability",
                detail: "capability argument count does not match capsule contract",
            });
        }
        return (kernel.handler)(&values).map_err(|code| EvalFault::CapabilityRefused {
            capability: capability.to_string(),
            code,
        });
    }
    // Fallback: the installed authored reference cell (Language Image
    // reference data). Nothing else matches — no feature-name lookup, no
    // std-cell registry.
    if let Some(cell) = crate::native_kernel::installed_reference_cell(capability) {
        return apply_reference_cell(capability, &cell, &values, budget);
    }
    Err(EvalFault::Arithmetic {
        op: "apply-capability",
        detail: "no installed reference bytecode or native kernel",
    })
}

/// Execute an installed authored reference cell: exact arity from the
/// declared params, contract guards at the seam, generic bytecode with
/// budget accounting, and the optional post-body certificate guard.
fn apply_reference_cell(
    capability: &str,
    cell: &CompiledCell,
    values: &[Value],
    budget: EvalBudget,
) -> Result<Value, EvalFault> {
    if values.len() != cell.params.len() {
        return Err(EvalFault::Arithmetic {
            op: "apply-capability",
            detail: "capability argument count does not match capsule contract",
        });
    }
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
        (Value::I64(left), Value::I64(right)) => {
            let result = match kind {
                ScalarOp::Add => left.checked_add(*right),
                ScalarOp::Sub => left.checked_sub(*right),
                ScalarOp::Mul => left.checked_mul(*right),
                ScalarOp::Div if *right != 0 && left % right == 0 => left.checked_div(*right),
                ScalarOp::Div => None,
            };
            result
                .map(Value::I64)
                .ok_or(EvalFault::Arithmetic {
                    op,
                    detail: "i64 overflow",
                })
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
        (Value::F64(left), Value::F64(right)) => Ok(Value::F64(match kind {
            ScalarOp::Add => left + right,
            ScalarOp::Sub => left - right,
            ScalarOp::Mul => left * right,
            ScalarOp::Div => left / right,
        })),
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
        Value::I64(value) => value.checked_neg().map(Value::I64).ok_or(EvalFault::Arithmetic {
            op: "neg",
            detail: "i64 overflow",
        }),
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
    inputs: &[Value],
    state: &[Value],
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
    for value in start..end {
        body_inputs[slot] = Value::I64(value);
        let next = evaluate(body, &body_inputs, state)?;
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
    let mut output = template.to_string();
    for value in arguments {
        output = output.replacen("{}", &value.to_string(), 1);
    }
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
            detail: "series is empty",
        });
    };
    let &(end, end_value) = points.last().expect("nonempty checked");
    if time < start || time > end {
        return match extrapolation {
            "clamp" => Ok(if time < start { start_value } else { end_value }),
            _ => Err(EvalFault::SeriesOutOfSupport {
                time_bits: time.to_bits(),
                start_bits: start.to_bits(),
                end_bits: end.to_bits(),
            }),
        };
    }
    if points.len() == 1 || time == end {
        return Ok(end_value);
    }
    let index = points
        .windows(2)
        .position(|window| time >= window[0].0 && time < window[1].0)
        .ok_or(EvalFault::Arithmetic {
            op: "series-sample",
            detail: "invalid support ordering",
        })?;
    let (left_time, left_value) = points[index];
    let (right_time, right_value) = points[index + 1];
    match interpolation {
        "previous" | "pwc" => Ok(left_value),
        "nearest" => Ok(if time - left_time < right_time - time {
            left_value
        } else {
            right_value
        }),
        _ => {
            Ok(left_value
                + (time - left_time) / (right_time - left_time) * (right_value - left_value))
        }
    }
}
