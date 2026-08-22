//! Strict-f64 interpreter for [`EmirProgram`](crate::EmirProgram).
//!
//! Registers are typed exactly like the locals the Rust backend emits
//! (`f64` / `bool`). Type confusion is a typed fault, never a coercion.
//!
//! # Determinism
//!
//! Arithmetic, comparisons, `min`/`max`, `abs`/`floor`/`ceil`, `is_finite`,
//! and boolean ops are bit-exact IEEE-754 binary64 across platforms.
//! Transcendentals (`sin`, `cos`, `tan`, `tanh`, `exp`, `ln`, `powf`,
//! `atan2`) follow the platform libm -- the same caveat as generated Rust
//! (Tier 1). Domain obligations recorded during lowering are assumptions,
//! not runtime checks: division by zero yields inf/NaN per IEEE, matching
//! the emitted Rust which also does not insert those checks.

use crate::{EmirOp, EmirProgram, EmirSliceAxis, EmirValue, FoldCombine};
use std::fmt;

/// A typed register value. Locals match generated Rust (`f64` / `bool` / `Vec<f64>`).
#[derive(Clone, Debug)]
pub enum Value {
    /// IEEE-754 binary64.
    F64(f64),
    /// Boolean, produced by comparisons, `is_finite`, `and`/`or`/`not`.
    Bool(bool),
    /// Vector of Float64.
    Vector(Vec<f64>),
    /// Matrix of Float64 (row-major).
    Matrix {
        rows: usize,
        cols: usize,
        data: Vec<f64>,
    },
    /// Rank-3+ tensor of Float64, row-major.
    Tensor {
        shape: Vec<usize>,
        data: Vec<f64>,
    },
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::F64(left), Self::F64(right)) => left.to_bits() == right.to_bits(),
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Vector(left), Self::Vector(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right.iter())
                        .all(|(l, r)| l.to_bits() == r.to_bits())
            }
            (
                Self::Matrix {
                    rows: r1,
                    cols: c1,
                    data: d1,
                },
                Self::Matrix {
                    rows: r2,
                    cols: c2,
                    data: d2,
                },
            ) => {
                r1 == r2
                    && c1 == c2
                    && d1.len() == d2.len()
                    && d1
                        .iter()
                        .zip(d2.iter())
                        .all(|(l, r)| l.to_bits() == r.to_bits())
            }
            (
                Self::Tensor {
                    shape: s1,
                    data: d1,
                },
                Self::Tensor {
                    shape: s2,
                    data: d2,
                },
            ) => {
                s1 == s2
                    && d1.len() == d2.len()
                    && d1
                        .iter()
                        .zip(d2.iter())
                        .all(|(l, r)| l.to_bits() == r.to_bits())
            }
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::F64(value) => f.write_str(&format_f64(*value)),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Vector(vec) => {
                f.write_str("[")?;
                for (i, elem) in vec.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str(&format_f64(*elem))?;
                }
                f.write_str("]")
            }
            Self::Matrix { rows, cols, data } => {
                f.write_str("[")?;
                for r in 0..*rows {
                    if r > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str("[")?;
                    for c in 0..*cols {
                        if c > 0 {
                            f.write_str(", ")?;
                        }
                        if let Some(elem) = data.get(r * cols + c) {
                            f.write_str(&format_f64(*elem))?;
                        }
                    }
                    f.write_str("]")?;
                }
                f.write_str("]")
            }
            Self::Tensor { shape, data } => {
                write!(f, "tensor{:?}[", shape)?;
                for (i, elem) in data.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str(&format_f64(*elem))?;
                }
                f.write_str("]")
            }
        }
    }
}

/// Format an f64 for display and JSON number tokens.
///
/// Finite values use `format!("{v}")`, with a trailing `.0` when that
/// spelling would otherwise look like an integer. Non-finite values are
/// the strings `NaN`, `Infinity`, and `-Infinity`.
#[must_use]
pub fn format_f64(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    let mut text = format!("{value}");
    if !text.contains('.') && !text.contains('e') && !text.contains('E') {
        text.push_str(".0");
    }
    text
}

/// Typed evaluation fault. The interpreter never panics on a well-formed
/// program; every failure is one of these variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalFault {
    /// An operand had the wrong type for `op` (no coercion).
    TypeConfusion {
        /// Register that failed the type check.
        register: u32,
        /// EMIR op name (`EmirOp::name`).
        op: &'static str,
    },
    /// `LoadInput` index was outside the provided input slice.
    MissingInput(u16),
    /// `LoadState` index was outside the provided state slice.
    MissingState(u16),
    /// An operand or the program result named an unwritten register.
    BadRegister(u32),
    /// Index was not a finite whole number, or was outside the value.
    IndexOutOfBounds {
        /// EMIR op name (`vec-index` / `mat-index`).
        op: &'static str,
        /// Requested index (row for matrices when `col` is set).
        index: i64,
        /// Exclusive upper bound of the indexed axis.
        len: usize,
    },
}

impl fmt::Display for EvalFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeConfusion { register, op } => {
                write!(f, "type confusion at %{register} in {op}")
            }
            Self::MissingInput(index) => write!(f, "missing input {index}"),
            Self::MissingState(index) => write!(f, "missing state {index}"),
            Self::BadRegister(register) => write!(f, "bad register %{register}"),
            Self::IndexOutOfBounds { op, index, len } => {
                write!(f, "{op} index {index} is outside 0..{len}")
            }
        }
    }
}

/// Evaluate `program` in a single forward pass over its linear ops.
///
/// `inputs` and `state` are indexed by [`EmirOp::LoadInput`] /
/// [`EmirOp::LoadState`]. Missing slots are faults. IEEE-754 exceptions
/// (division by zero, invalid) are not faults.
///
/// `And` / `Or` evaluate both operands (the linear IR already materialized
/// them as registers) then apply `&&` / `||`, matching the Rust backend
/// which emits `&&` / `||` against those registers.
pub fn evaluate(program: &EmirProgram, inputs: &[Value], state: &[Value]) -> Result<Value, EvalFault> {
    let mut registers: Vec<Value> = Vec::with_capacity(program.ops.len());
    for (op, _) in &program.ops {
        let value = eval_op(op, &registers, inputs, state)?;
        registers.push(value);
    }
    register(&registers, program.result).cloned()
}

/// Convenience for scalar-only programs (existing tests and given maps).
pub fn evaluate_f64(program: &EmirProgram, inputs: &[f64], state: &[f64]) -> Result<Value, EvalFault> {
    let inputs: Vec<Value> = inputs.iter().copied().map(Value::F64).collect();
    let state: Vec<Value> = state.iter().copied().map(Value::F64).collect();
    evaluate(program, &inputs, &state)
}

fn eval_op(
    op: &EmirOp,
    registers: &[Value],
    inputs: &[Value],
    state: &[Value],
) -> Result<Value, EvalFault> {
    let name = op.name();
    match *op {
        EmirOp::ConstF64(bits) => Ok(Value::F64(f64::from_bits(bits))),
        EmirOp::LoadInput(index) => inputs
            .get(usize::from(index))
            .cloned()
            .ok_or(EvalFault::MissingInput(index)),
        EmirOp::LoadState(index) => state
            .get(usize::from(index))
            .cloned()
            .ok_or(EvalFault::MissingState(index)),
        EmirOp::F64Add(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)? + f64_of(registers, right, name)?,
        )),
        EmirOp::F64Sub(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)? - f64_of(registers, right, name)?,
        )),
        EmirOp::F64Mul(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)? * f64_of(registers, right, name)?,
        )),
        EmirOp::F64Div(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)? / f64_of(registers, right, name)?,
        )),
        EmirOp::F64Pow(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.powf(f64_of(registers, right, name)?),
        )),
        EmirOp::Neg(value) => Ok(Value::F64(-f64_of(registers, value, name)?)),
        EmirOp::Exp(value) => Ok(Value::F64(f64_of(registers, value, name)?.exp())),
        EmirOp::Ln(value) => Ok(Value::F64(f64_of(registers, value, name)?.ln())),
        EmirOp::Sqrt(value) => Ok(Value::F64(f64_of(registers, value, name)?.sqrt())),
        EmirOp::Sin(value) => Ok(Value::F64(f64_of(registers, value, name)?.sin())),
        EmirOp::Cos(value) => Ok(Value::F64(f64_of(registers, value, name)?.cos())),
        EmirOp::Tan(value) => Ok(Value::F64(f64_of(registers, value, name)?.tan())),
        EmirOp::Tanh(value) => Ok(Value::F64(f64_of(registers, value, name)?.tanh())),
        EmirOp::Abs(value) => Ok(Value::F64(f64_of(registers, value, name)?.abs())),
        EmirOp::Floor(value) => Ok(Value::F64(f64_of(registers, value, name)?.floor())),
        EmirOp::Ceil(value) => Ok(Value::F64(f64_of(registers, value, name)?.ceil())),
        EmirOp::Sign(value) => Ok(Value::F64(f64_of(registers, value, name)?.signum())),
        EmirOp::Log2(value) => Ok(Value::F64(f64_of(registers, value, name)?.log2())),
        EmirOp::Log10(value) => Ok(Value::F64(f64_of(registers, value, name)?.log10())),
        EmirOp::Sinh(value) => Ok(Value::F64(f64_of(registers, value, name)?.sinh())),
        EmirOp::Cosh(value) => Ok(Value::F64(f64_of(registers, value, name)?.cosh())),
        EmirOp::Atan(value) => Ok(Value::F64(f64_of(registers, value, name)?.atan())),
        EmirOp::Cbrt(value) => Ok(Value::F64(f64_of(registers, value, name)?.cbrt())),
        EmirOp::Hypot(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.hypot(f64_of(registers, right, name)?),
        )),
        EmirOp::Min(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.min(f64_of(registers, right, name)?),
        )),
        EmirOp::Max(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.max(f64_of(registers, right, name)?),
        )),
        EmirOp::Atan2(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.atan2(f64_of(registers, right, name)?),
        )),
        EmirOp::Mod(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)? % f64_of(registers, right, name)?,
        )),
        EmirOp::Lt(left, right) => Ok(Value::Bool(
            f64_of(registers, left, name)? < f64_of(registers, right, name)?,
        )),
        EmirOp::Le(left, right) => Ok(Value::Bool(
            f64_of(registers, left, name)? <= f64_of(registers, right, name)?,
        )),
        EmirOp::Gt(left, right) => Ok(Value::Bool(
            f64_of(registers, left, name)? > f64_of(registers, right, name)?,
        )),
        EmirOp::Ge(left, right) => Ok(Value::Bool(
            f64_of(registers, left, name)? >= f64_of(registers, right, name)?,
        )),
        EmirOp::Eq(left, right) => eq_ne(registers, left, right, name, true),
        EmirOp::Ne(left, right) => eq_ne(registers, left, right, name, false),
        EmirOp::And(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? && bool_of(registers, right, name)?,
        )),
        EmirOp::Or(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? || bool_of(registers, right, name)?,
        )),
        EmirOp::Not(value) => Ok(Value::Bool(!bool_of(registers, value, name)?)),
        EmirOp::IsFinite(value) => Ok(Value::Bool(f64_of(registers, value, name)?.is_finite())),
        EmirOp::Select {
            condition,
            then_value,
            else_value,
        } => {
            if bool_of(registers, condition, name)? {
                register(registers, then_value).cloned()
            } else {
                register(registers, else_value).cloned()
            }
        }
        EmirOp::VectorCreate(ref elements) => {
            let mut vec = Vec::with_capacity(elements.len());
            for &elem in elements {
                vec.push(f64_of(registers, elem, name)?);
            }
            Ok(Value::Vector(vec))
        }
        EmirOp::MatrixCreate {
            rows,
            cols,
            ref elements,
        } => {
            let mut data = Vec::with_capacity(elements.len());
            for &elem in elements {
                data.push(f64_of(registers, elem, name)?);
            }
            Ok(Value::Matrix {
                rows,
                cols,
                data,
            })
        }
        EmirOp::VectorIndex { vector, index } => {
            let vec = vector_of(registers, vector, name)?;
            let i = whole_index(registers, index, name, vec.len())?;
            Ok(Value::F64(vec[i]))
        }
        EmirOp::MatrixIndex { matrix, row, col } => {
            let (r_count, c_count, data) = matrix_of(registers, matrix, name)?;
            let r = whole_index(registers, row, name, r_count)?;
            let c = whole_index(registers, col, name, c_count)?;
            Ok(Value::F64(data[r * c_count + c]))
        }
        EmirOp::VectorAdd(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            let out = v1.iter().zip(v2.iter()).map(|(a, b)| a + b).collect();
            Ok(Value::Vector(out))
        }
        EmirOp::VectorSub(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            let out = v1.iter().zip(v2.iter()).map(|(a, b)| a - b).collect();
            Ok(Value::Vector(out))
        }
        EmirOp::VectorScale(left, right) => {
            // Canonical operand order from admission: (vector, scalar).
            // Still accept (scalar, vector) so older EMIR stays evaluable.
            match (register(registers, left)?, register(registers, right)?) {
                (Value::Vector(v), Value::F64(s)) | (Value::F64(s), Value::Vector(v)) => {
                    Ok(Value::Vector(v.iter().map(|x| x * s).collect()))
                }
                _ => Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                }),
            }
        }
        EmirOp::VectorDot(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            let dot: f64 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
            Ok(Value::F64(dot))
        }
        EmirOp::VectorNorm(value) => {
            let v = vector_of(registers, value, name)?;
            let sum_sq: f64 = v.iter().map(|x| x * x).sum();
            Ok(Value::F64(sum_sq.sqrt()))
        }
        EmirOp::VectorLength(value) => {
            let v = vector_of(registers, value, name)?;
            Ok(Value::F64(v.len() as f64))
        }
        EmirOp::MatrixAdd(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (_, _, d2) = matrix_of(registers, right, name)?;
            let data = d1.iter().zip(d2.iter()).map(|(a, b)| a + b).collect();
            Ok(Value::Matrix {
                rows: r1,
                cols: c1,
                data,
            })
        }
        EmirOp::MatrixSub(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (_, _, d2) = matrix_of(registers, right, name)?;
            let data = d1.iter().zip(d2.iter()).map(|(a, b)| a - b).collect();
            Ok(Value::Matrix {
                rows: r1,
                cols: c1,
                data,
            })
        }
        EmirOp::MatrixScale(left, right) => {
            match (register(registers, left)?, register(registers, right)?) {
                (Value::Matrix { rows, cols, data }, Value::F64(s))
                | (Value::F64(s), Value::Matrix { rows, cols, data }) => Ok(Value::Matrix {
                    rows: *rows,
                    cols: *cols,
                    data: data.iter().map(|x| x * s).collect(),
                }),
                _ => Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                }),
            }
        }
        EmirOp::MatrixMulVector(matrix, vector) => {
            let (rows, cols, m_data) = matrix_of(registers, matrix, name)?;
            let v = vector_of(registers, vector, name)?;
            let mut out = Vec::with_capacity(rows);
            for r in 0..rows {
                let sum: f64 = (0..cols)
                    .map(|c| m_data[r * cols + c] * v.get(c).copied().unwrap_or(0.0))
                    .sum();
                out.push(sum);
            }
            Ok(Value::Vector(out))
        }
        EmirOp::MatrixMulMatrix(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (r2, c2, d2) = matrix_of(registers, right, name)?;
            let mut out = Vec::with_capacity(r1 * c2);
            for i in 0..r1 {
                for j in 0..c2 {
                    let sum: f64 = (0..c1)
                        .map(|k| {
                            let a = d1[i * c1 + k];
                            let b = if k < r2 { d2[k * c2 + j] } else { 0.0 };
                            a * b
                        })
                        .sum();
                    out.push(sum);
                }
            }
            Ok(Value::Matrix {
                rows: r1,
                cols: c2,
                data: out,
            })
        }
        EmirOp::MatrixTranspose(value) => {
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let mut out = vec![0.0; rows * cols];
            for r in 0..rows {
                for c in 0..cols {
                    out[c * rows + r] = data[r * cols + c];
                }
            }
            Ok(Value::Matrix {
                rows: cols,
                cols: rows,
                data: out,
            })
        }
        EmirOp::TensorCreate { ref shape, ref elements } => {
            let mut data = Vec::with_capacity(elements.len());
            for &elem in elements {
                data.push(f64_of(registers, elem, name)?);
            }
            Ok(Value::Tensor {
                shape: shape.clone(),
                data,
            })
        }
        EmirOp::TensorIndex {
            tensor,
            ref indices,
        } => {
            let (shape, data) = tensor_of(registers, tensor, name)?;
            if indices.len() != shape.len() {
                return Err(EvalFault::TypeConfusion {
                    register: tensor.0,
                    op: name,
                });
            }
            let mut offset = 0usize;
            for (axis, &index) in indices.iter().enumerate() {
                let i = whole_index(registers, index, name, shape[axis])?;
                offset = offset * shape[axis] + i;
            }
            Ok(Value::F64(data[offset]))
        }
        EmirOp::TensorSlice {
            tensor,
            ref axes,
        } => eval_tensor_slice(registers, tensor, axes, name),
        EmirOp::TensorAdd(left, right) => {
            let (s1, d1) = tensor_of(registers, left, name)?;
            let (s2, d2) = tensor_of(registers, right, name)?;
            if s1 != s2 {
                return Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                });
            }
            Ok(Value::Tensor {
                shape: s1.to_vec(),
                data: d1.iter().zip(d2.iter()).map(|(a, b)| a + b).collect(),
            })
        }
        EmirOp::TensorSub(left, right) => {
            let (s1, d1) = tensor_of(registers, left, name)?;
            let (s2, d2) = tensor_of(registers, right, name)?;
            if s1 != s2 {
                return Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                });
            }
            Ok(Value::Tensor {
                shape: s1.to_vec(),
                data: d1.iter().zip(d2.iter()).map(|(a, b)| a - b).collect(),
            })
        }
        EmirOp::Fold {
            start,
            end,
            init,
            combine,
            loop_var_index,
            ref body,
        } => {
            let start_val = f64_of(registers, start, name)?;
            let end_val = f64_of(registers, end, name)?;
            let start_i = start_val as i64;
            let end_i = end_val as i64;
            match combine {
                FoldCombine::Add | FoldCombine::Mul => {
                    let mut acc = f64_of(registers, init, name)?;
                    for i in start_i..end_i {
                        let mut body_inputs = inputs.to_vec();
                        let idx = usize::from(loop_var_index);
                        while body_inputs.len() <= idx {
                            body_inputs.push(Value::F64(0.0));
                        }
                        body_inputs[idx] = Value::F64(i as f64);
                        match evaluate(body, &body_inputs, state)? {
                            Value::F64(term) => {
                                acc = match combine {
                                    FoldCombine::Add => acc + term,
                                    FoldCombine::Mul => acc * term,
                                    _ => unreachable!(),
                                };
                            }
                            _ => {
                                return Err(EvalFault::TypeConfusion {
                                    register: body.result.0,
                                    op: name,
                                })
                            }
                        }
                    }
                    Ok(Value::F64(acc))
                }
                FoldCombine::And | FoldCombine::Or => {
                    let mut acc = f64_of(registers, init, name)? != 0.0;
                    for i in start_i..end_i {
                        let mut body_inputs = inputs.to_vec();
                        let idx = usize::from(loop_var_index);
                        while body_inputs.len() <= idx {
                            body_inputs.push(Value::F64(0.0));
                        }
                        body_inputs[idx] = Value::F64(i as f64);
                        let term = match evaluate(body, &body_inputs, state)? {
                            Value::Bool(b) => b,
                            Value::F64(f) => f != 0.0,
                            _ => {
                                return Err(EvalFault::TypeConfusion {
                                    register: body.result.0,
                                    op: name,
                                })
                            }
                        };
                        acc = match combine {
                            FoldCombine::And => acc && term,
                            FoldCombine::Or => acc || term,
                            _ => unreachable!(),
                        };
                    }
                    Ok(Value::Bool(acc))
                }
            }
        }
        EmirOp::Integral {
            start,
            end,
            steps,
            loop_var_index,
            ref integrand,
        } => {
            let a = f64_of(registers, start, name)?;
            let b = f64_of(registers, end, name)?;
            let n = steps as i64;
            let h = (b - a) / n as f64;
            let mut acc = 0.0;
            for i in 0..=n {
                let x = a + i as f64 * h;
                let weight = if i == 0 || i == n {
                    1.0
                } else if i % 2 == 0 {
                    2.0
                } else {
                    4.0
                };
                let mut body_inputs = inputs.to_vec();
                let idx = usize::from(loop_var_index);
                while body_inputs.len() <= idx {
                    body_inputs.push(Value::F64(0.0));
                }
                body_inputs[idx] = Value::F64(x);
                match evaluate(integrand, &body_inputs, state)? {
                    Value::F64(fx) => acc += weight * fx,
                    _ => {
                        return Err(EvalFault::TypeConfusion {
                            register: integrand.result.0,
                            op: name,
                        })
                    }
                }
            }
            Ok(Value::F64(acc * h / 3.0))
        }
        EmirOp::Differentiate { ref body, var_index } => {
            let dual = evaluate_dual(body, inputs, state, var_index, name)?;
            Ok(Value::F64(dual.tangent))
        }
        EmirOp::Solve {
            ref body,
            var_index,
            tolerance,
            max_iter,
        } => {
            // Newton's method: x_new = x_old - f(x) / f'(x)
            // Uses dual-number evaluation for both f and f' in one pass.
            let mut x = match inputs.get(var_index as usize) {
                Some(Value::F64(v)) => *v,
                _ => return Err(EvalFault::TypeConfusion {
                    register: var_index as u32,
                    op: name,
                }),
            };
            let mut work_inputs = inputs.to_vec();
            for _ in 0..max_iter {
                work_inputs[var_index as usize] = Value::F64(x);
                let dual = evaluate_dual(body, &work_inputs, state, var_index, name)?;
                let f = dual.primal;
                let df = dual.tangent;
                if f.abs() < tolerance {
                    return Ok(Value::F64(x));
                }
                if df.abs() < 1e-30 {
                    return Ok(Value::F64(x));
                }
                x -= f / df;
            }
            Ok(Value::F64(x))
        }
        EmirOp::Optimize {
            ref body,
            ref var_indices,
            maximize,
            learning_rate,
            tolerance,
            max_iter,
        } => {
            // Multi-variable gradient descent (or ascent):
            //   x_i_new = x_i_old -/+ lr * df/dx_i
            // One dual-number pass per variable gives each partial.
            let mut work_inputs = inputs.to_vec();
            let sign = if maximize { 1.0 } else { -1.0 };
            // Extract initial guesses from inputs.
            let mut x: Vec<f64> = var_indices
                .iter()
                .map(|&vi| match inputs.get(vi as usize) {
                    Some(Value::F64(v)) => *v,
                    _ => 0.0,
                })
                .collect();
            for _ in 0..max_iter {
                // Compute partial derivative w.r.t. each variable.
                let mut grads = Vec::with_capacity(var_indices.len());
                let mut max_grad = 0.0f64;
                for (i, &vi) in var_indices.iter().enumerate() {
                    work_inputs[vi as usize] = Value::F64(x[i]);
                    let dual = evaluate_dual(body, &work_inputs, state, vi, name)?;
                    grads.push(dual.tangent);
                    max_grad = max_grad.max(dual.tangent.abs());
                }
                if max_grad < tolerance {
                    // Return the first variable's converged value.
                    return Ok(Value::F64(x[0]));
                }
                for (i, &vi) in var_indices.iter().enumerate() {
                    x[i] += sign * learning_rate * grads[i];
                    work_inputs[vi as usize] = Value::F64(x[i]);
                }
            }
            Ok(Value::F64(x[0]))
        }
    }
}

/// Dual number for forward-mode autodiff: (primal, tangent).
#[derive(Clone)]
struct Dual {
    primal: f64,
    tangent: f64,
}

/// Evaluate an EMIR sub-program with dual numbers, seeding the input at
/// `var_index` with tangent 1.0.  Returns the full dual (primal +
/// tangent) of the result — i.e. both the function value and its
/// derivative with respect to that input.
fn evaluate_dual(
    program: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
    var_index: u16,
    name: &'static str,
) -> Result<Dual, EvalFault> {
    let mut registers: Vec<Dual> = Vec::with_capacity(program.ops.len());
    for (op, _) in &program.ops {
        let dual = match op {
            EmirOp::ConstF64(bits) => Dual {
                primal: f64::from_bits(*bits),
                tangent: 0.0,
            },
            EmirOp::LoadInput(idx) => {
                let primal = match inputs.get(*idx as usize) {
                    Some(Value::F64(v)) => *v,
                    _ => return Err(EvalFault::TypeConfusion { register: *idx as u32, op: name }),
                };
                let tangent = if *idx == var_index { 1.0 } else { 0.0 };
                Dual { primal, tangent }
            }
            EmirOp::LoadState(idx) => {
                let primal = match state.get(*idx as usize) {
                    Some(Value::F64(v)) => *v,
                    _ => return Err(EvalFault::TypeConfusion { register: *idx as u32, op: name }),
                };
                Dual { primal, tangent: 0.0 }
            }
            EmirOp::F64Add(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: a.primal + b.primal, tangent: a.tangent + b.tangent }
            }
            EmirOp::F64Sub(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: a.primal - b.primal, tangent: a.tangent - b.tangent }
            }
            EmirOp::F64Mul(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual {
                    primal: a.primal * b.primal,
                    tangent: a.tangent * b.primal + a.primal * b.tangent,
                }
            }
            EmirOp::F64Div(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                let bp2 = b.primal * b.primal;
                Dual {
                    primal: a.primal / b.primal,
                    tangent: (a.tangent * b.primal - a.primal * b.tangent) / bp2,
                }
            }
            EmirOp::Neg(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: -a.primal, tangent: -a.tangent }
            }
            EmirOp::Exp(a) => {
                let a = dual_of(&registers, a, name)?;
                let p = a.primal.exp();
                Dual { primal: p, tangent: p * a.tangent }
            }
            EmirOp::Ln(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.ln(), tangent: a.tangent / a.primal }
            }
            EmirOp::Sqrt(a) => {
                let a = dual_of(&registers, a, name)?;
                let p = a.primal.sqrt();
                Dual { primal: p, tangent: a.tangent / (2.0 * p) }
            }
            EmirOp::Sin(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.sin(), tangent: a.primal.cos() * a.tangent }
            }
            EmirOp::Cos(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.cos(), tangent: -a.primal.sin() * a.tangent }
            }
            EmirOp::Tan(a) => {
                let a = dual_of(&registers, a, name)?;
                let c = a.primal.cos();
                Dual { primal: a.primal.tan(), tangent: a.tangent / (c * c) }
            }
            EmirOp::Tanh(a) => {
                let a = dual_of(&registers, a, name)?;
                let t = a.primal.tanh();
                Dual { primal: t, tangent: (1.0 - t * t) * a.tangent }
            }
            EmirOp::Abs(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.abs(), tangent: a.primal.signum() * a.tangent }
            }
            EmirOp::Floor(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.floor(), tangent: 0.0 }
            }
            EmirOp::Ceil(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.ceil(), tangent: 0.0 }
            }
            EmirOp::Sign(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.signum(), tangent: 0.0 }
            }
            EmirOp::Log2(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.log2(), tangent: a.tangent / (a.primal * std::f64::consts::LN_2) }
            }
            EmirOp::Log10(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.log10(), tangent: a.tangent / (a.primal * std::f64::consts::LN_10) }
            }
            EmirOp::Sinh(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.sinh(), tangent: a.primal.cosh() * a.tangent }
            }
            EmirOp::Cosh(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: a.primal.cosh(), tangent: a.primal.sinh() * a.tangent }
            }
            EmirOp::Atan(a) => {
                let a = dual_of(&registers, a, name)?;
                let d = 1.0 / (1.0 + a.primal * a.primal);
                Dual { primal: a.primal.atan(), tangent: d * a.tangent }
            }
            EmirOp::Cbrt(a) => {
                let a = dual_of(&registers, a, name)?;
                let p = a.primal.cbrt();
                Dual { primal: p, tangent: a.tangent / (3.0 * p * p) }
            }
            EmirOp::Hypot(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                let h = a.primal.hypot(b.primal);
                Dual { primal: h, tangent: (a.primal * a.tangent + b.primal * b.tangent) / h }
            }
            EmirOp::F64Pow(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                let p = a.primal.powf(b.primal);
                if b.tangent == 0.0 {
                    // Constant exponent: d/dx [a^b] = b * a^(b-1) * a'
                    Dual { primal: p, tangent: b.primal * a.primal.powf(b.primal - 1.0) * a.tangent }
                } else {
                    // General: a^b * (b * a'/a + b' * ln(a))
                    Dual { primal: p, tangent: p * (b.primal * a.tangent / a.primal + b.tangent * a.primal.ln()) }
                }
            }
            EmirOp::Min(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                if a.primal < b.primal {
                    Dual { primal: a.primal, tangent: a.tangent }
                } else {
                    Dual { primal: b.primal, tangent: b.tangent }
                }
            }
            EmirOp::Max(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                if a.primal > b.primal {
                    Dual { primal: a.primal, tangent: a.tangent }
                } else {
                    Dual { primal: b.primal, tangent: b.tangent }
                }
            }
            EmirOp::Atan2(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                let denom = a.primal * a.primal + b.primal * b.primal;
                Dual {
                    primal: a.primal.atan2(b.primal),
                    tangent: (b.primal * a.tangent - a.primal * b.tangent) / denom,
                }
            }
            EmirOp::Mod(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                // mod(a, b) = a - b * floor(a/b).  At non-boundary points,
                // d/da = 1, d/db ≈ -floor(a/b).  Use the simple approximation.
                Dual {
                    primal: a.primal % b.primal,
                    tangent: a.tangent,
                }
            }
            EmirOp::Select { condition: c, then_value: t, else_value: e } => {
                let c = dual_of(&registers, c, name)?;
                let t = dual_of(&registers, t, name)?;
                let e = dual_of(&registers, e, name)?;
                if c.primal != 0.0 {
                    Dual { primal: t.primal, tangent: t.tangent }
                } else {
                    Dual { primal: e.primal, tangent: e.tangent }
                }
            }
            EmirOp::IsFinite(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: if a.primal.is_finite() { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            // Comparisons and boolean ops: tangent is always 0.0.
            EmirOp::Eq(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal == b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Ne(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal != b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Lt(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal < b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Le(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal <= b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Gt(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal > b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Ge(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal >= b.primal { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::And(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal != 0.0 && b.primal != 0.0 { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Or(a, b) => {
                let a = dual_of(&registers, a, name)?;
                let b = dual_of(&registers, b, name)?;
                Dual { primal: if a.primal != 0.0 || b.primal != 0.0 { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            EmirOp::Not(a) => {
                let a = dual_of(&registers, a, name)?;
                Dual { primal: if a.primal == 0.0 { 1.0 } else { 0.0 }, tangent: 0.0 }
            }
            _ => {
                return Err(EvalFault::TypeConfusion {
                    register: program.result.0,
                    op: "differentiate (unsupported op in dual evaluation)",
                });
            }
        };
        registers.push(dual);
    }
    let result = registers
        .get(program.result.0 as usize)
        .ok_or(EvalFault::BadRegister(program.result.0))?;
    Ok(result.clone())
}

fn dual_of(registers: &[Dual], value: &EmirValue, op: &'static str) -> Result<Dual, EvalFault> {
    registers
        .get(value.0 as usize)
        .cloned()
        .ok_or(EvalFault::TypeConfusion { register: value.0, op })
}

fn eq_ne(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    equal: bool,
) -> Result<Value, EvalFault> {
    let left_value = register(registers, left)?;
    let right_value = register(registers, right)?;
    let result = match (&left_value, &right_value) {
        (Value::F64(left), Value::F64(right)) => left == right,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Bool(left), Value::F64(right)) => *left == (*right != 0.0),
        (Value::F64(left), Value::Bool(right)) => (*left != 0.0) == *right,
        (Value::Vector(left), Value::Vector(right)) => left == right,
        (Value::Matrix { rows: r1, cols: c1, data: d1 }, Value::Matrix { rows: r2, cols: c2, data: d2 }) => {
            r1 == r2 && c1 == c2 && d1 == d2
        }
        (
            Value::Tensor {
                shape: s1,
                data: d1,
            },
            Value::Tensor {
                shape: s2,
                data: d2,
            },
        ) => s1 == s2 && d1 == d2,
        _ => {
            return Err(EvalFault::TypeConfusion {
                register: left.0,
                op,
            });
        }
    };
    Ok(Value::Bool(if equal { result } else { !result }))
}

fn register(registers: &[Value], value: EmirValue) -> Result<&Value, EvalFault> {
    registers
        .get(value.0 as usize)
        .ok_or(EvalFault::BadRegister(value.0))
}

fn whole_index(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
    len: usize,
) -> Result<usize, EvalFault> {
    let raw = f64_of(registers, value, op)?;
    if !raw.is_finite() || raw < 0.0 || raw.fract() != 0.0 {
        return Err(EvalFault::IndexOutOfBounds {
            op,
            index: raw as i64,
            len,
        });
    }
    let index = raw as usize;
    if index >= len {
        return Err(EvalFault::IndexOutOfBounds {
            op,
            index: index as i64,
            len,
        });
    }
    Ok(index)
}

fn f64_of(registers: &[Value], value: EmirValue, op: &'static str) -> Result<f64, EvalFault> {
    match register(registers, value)? {
        Value::F64(number) => Ok(*number),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

fn bool_of(registers: &[Value], value: EmirValue, op: &'static str) -> Result<bool, EvalFault> {
    match register(registers, value)? {
        Value::Bool(flag) => Ok(*flag),
        Value::F64(num) => Ok(*num != 0.0),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

fn vector_of<'a>(
    registers: &'a [Value],
    value: EmirValue,
    op: &'static str,
) -> Result<&'a [f64], EvalFault> {
    match register(registers, value)? {
        Value::Vector(v) => Ok(v.as_slice()),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

fn matrix_of<'a>(
    registers: &'a [Value],
    value: EmirValue,
    op: &'static str,
) -> Result<(usize, usize, &'a [f64]), EvalFault> {
    match register(registers, value)? {
        Value::Matrix { rows, cols, data } => Ok((*rows, *cols, data.as_slice())),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

fn tensor_of<'a>(
    registers: &'a [Value],
    value: EmirValue,
    op: &'static str,
) -> Result<(&'a [usize], &'a [f64]), EvalFault> {
    match register(registers, value)? {
        Value::Tensor { shape, data } => Ok((shape.as_slice(), data.as_slice())),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

fn collect_slice(
    data: &[f64],
    shape: &[usize],
    starts: &[usize],
    out_shape: &[usize],
    axis: usize,
    offset: usize,
    out: &mut Vec<f64>,
) {
    if axis == shape.len() {
        out.push(data[offset]);
        return;
    }
    let stride = shape[axis + 1..].iter().product::<usize>().max(1);
    for i in 0..out_shape[axis] {
        collect_slice(
            data,
            shape,
            starts,
            out_shape,
            axis + 1,
            offset + (starts[axis] + i) * stride,
            out,
        );
    }
}

fn eval_tensor_slice(
    registers: &[Value],
    tensor: EmirValue,
    axes: &[EmirSliceAxis],
    name: &'static str,
) -> Result<Value, EvalFault> {
    let (shape, data) = match register(registers, tensor)? {
        Value::Vector(items) => (vec![items.len()], items.as_slice()),
        Value::Matrix { rows, cols, data } => (vec![*rows, *cols], data.as_slice()),
        Value::Tensor { shape, data } => (shape.clone(), data.as_slice()),
        _ => {
            return Err(EvalFault::TypeConfusion {
                register: tensor.0,
                op: name,
            });
        }
    };
    if axes.len() != shape.len() {
        return Err(EvalFault::TypeConfusion {
            register: tensor.0,
            op: name,
        });
    }
    let mut starts = Vec::with_capacity(axes.len());
    let mut out_shape = Vec::with_capacity(axes.len());
    for (axis, slice) in axes.iter().enumerate() {
        match slice {
            EmirSliceAxis::Point(index) => {
                let i = whole_index(registers, *index, name, shape[axis])?;
                starts.push(i);
                out_shape.push(1);
            }
            EmirSliceAxis::Range { start, end } => {
                let start_i = whole_index(registers, *start, name, shape[axis] + 1)?;
                let raw_end = f64_of(registers, *end, name)?;
                if !raw_end.is_finite() || raw_end < 0.0 || raw_end.fract() != 0.0 {
                    return Err(EvalFault::IndexOutOfBounds {
                        op: name,
                        index: raw_end as i64,
                        len: shape[axis],
                    });
                }
                let end_i = raw_end as usize;
                if end_i > shape[axis] || start_i > end_i {
                    return Err(EvalFault::IndexOutOfBounds {
                        op: name,
                        index: end_i as i64,
                        len: shape[axis],
                    });
                }
                starts.push(start_i);
                out_shape.push(end_i - start_i);
            }
        }
    }
    let mut out = Vec::new();
    collect_slice(data, &shape, &starts, &out_shape, 0, 0, &mut out);
    let kept: Vec<usize> = axes
        .iter()
        .zip(out_shape)
        .filter_map(|(axis, extent)| matches!(axis, EmirSliceAxis::Range { .. }).then_some(extent))
        .collect();
    match kept.as_slice() {
        [] => Ok(Value::F64(out.first().copied().unwrap_or(f64::NAN))),
        [_] => Ok(Value::Vector(out)),
        [rows, cols] => Ok(Value::Matrix {
            rows: *rows,
            cols: *cols,
            data: out,
        }),
        _ => Ok(Value::Tensor {
            shape: kept,
            data: out,
        }),
    }
}

