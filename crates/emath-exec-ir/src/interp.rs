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

use crate::{EdgePolicy, EmirOp, EmirProgram, FoldCombine};

mod dual;
mod helpers;
mod value;

use dual::evaluate_dual;
use helpers::*;
pub use value::{EvalFault, Value, format_f64};

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
        EmirOp::ConstI64(value) => Ok(Value::I64(value)),
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
        EmirOp::Round(value) => Ok(Value::F64(f64_of(registers, value, name)?.round())),
        EmirOp::Sign(value) => Ok(Value::F64(f64_of(registers, value, name)?.signum())),
        EmirOp::Log2(value) => Ok(Value::F64(f64_of(registers, value, name)?.log2())),
        EmirOp::Log10(value) => Ok(Value::F64(f64_of(registers, value, name)?.log10())),
        EmirOp::Sinh(value) => Ok(Value::F64(f64_of(registers, value, name)?.sinh())),
        EmirOp::Cosh(value) => Ok(Value::F64(f64_of(registers, value, name)?.cosh())),
        EmirOp::Atan(value) => Ok(Value::F64(f64_of(registers, value, name)?.atan())),
        EmirOp::Cbrt(value) => Ok(Value::F64(f64_of(registers, value, name)?.cbrt())),
        EmirOp::Recip(value) => Ok(Value::F64(f64_of(registers, value, name)?.recip())),
        EmirOp::Fract(value) => Ok(Value::F64(f64_of(registers, value, name)?.fract())),
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
            let expected = rows.checked_mul(cols).ok_or(EvalFault::Arithmetic {
                op: name,
                detail: "matrix size overflow",
            })?;
            if elements.len() != expected {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "matrix element count does not match rows*cols",
                });
            }
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
            let offset = r
                .checked_mul(c_count)
                .and_then(|base| base.checked_add(c))
                .ok_or(EvalFault::Arithmetic {
                    op: name,
                    detail: "matrix index offset overflow",
                })?;
            data.get(offset)
                .copied()
                .map(Value::F64)
                .ok_or(EvalFault::IndexOutOfBounds {
                    op: name,
                    index: i64::try_from(offset).unwrap_or(i64::MAX),
                    len: data.len(),
                })
        }
        EmirOp::VectorAdd(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            require_equal_len(v1.len(), v2.len(), name, "vector length mismatch")?;
            let out = v1.iter().zip(v2.iter()).map(|(a, b)| a + b).collect();
            Ok(Value::Vector(out))
        }
        EmirOp::VectorSub(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            require_equal_len(v1.len(), v2.len(), name, "vector length mismatch")?;
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
            require_equal_len(v1.len(), v2.len(), name, "vector length mismatch")?;
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
        EmirOp::Stencil1d {
            input,
            ref weights,
            center,
            edge,
        } => {
            let v = vector_of(registers, input, name)?;
            let n = v.len();
            let last = n.saturating_sub(1) as isize;
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                let mut acc = 0.0f64;
                for (k, &w) in weights.iter().enumerate() {
                    let raw = i as isize + k as isize - center as isize;
                    acc += match edge {
                        // Replicate the nearest in-range cell.
                        EdgePolicy::Clamp => {
                            let idx = raw.clamp(0, last) as usize;
                            w * v[idx]
                        }
                        // Mirror across the boundary: u[-1]=u[1], u[n]=u[n-2].
                        // The trailing clamp guards tiny vectors (n < 3)
                        // where the mirror target is itself out of range.
                        EdgePolicy::Neumann => {
                            let idx = if raw < 0 {
                                (-raw) as usize
                            } else if raw > last {
                                (2 * last - raw) as usize
                            } else {
                                raw as usize
                            };
                            w * v[idx.clamp(0, last as usize)]
                        }
                        // Fixed boundary values; OOB taps read the constant.
                        EdgePolicy::Dirichlet { left, right } => {
                            if raw < 0 {
                                w * left
                            } else if raw > last {
                                w * right
                            } else {
                                w * v[raw as usize]
                            }
                        }
                    };
                }
                out.push(acc);
            }
            Ok(Value::Vector(out))
        }
        EmirOp::Stencil2d {
            input,
            ref weights,
            center,
            edge,
        } => {
            let (rows, cols, data) = matrix_of(registers, input, name)?;
            let last_r = rows.saturating_sub(1) as isize;
            let last_c = cols.saturating_sub(1) as isize;
            let (cr, cc) = center;
            let mut out = Vec::with_capacity(data.len());
            for r in 0..rows {
                for c in 0..cols {
                    let mut acc = 0.0f64;
                    for kr in 0..3usize {
                        for kc in 0..3usize {
                            let w = weights[kr * 3 + kc];
                            if w == 0.0 {
                                continue;
                            }
                            let raw_r = r as isize + kr as isize - cr as isize;
                            let raw_c = c as isize + kc as isize - cc as isize;
                            acc += match edge {
                                EdgePolicy::Clamp => {
                                    let rr = raw_r.clamp(0, last_r) as usize;
                                    let cc2 = raw_c.clamp(0, last_c) as usize;
                                    w * data[rr * cols + cc2]
                                }
                                EdgePolicy::Neumann => {
                                    let rr = (if raw_r < 0 {
                                        -raw_r
                                    } else if raw_r > last_r {
                                        2 * last_r - raw_r
                                    } else {
                                        raw_r
                                    })
                                    .clamp(0, last_r) as usize;
                                    let cc2 = (if raw_c < 0 {
                                        -raw_c
                                    } else if raw_c > last_c {
                                        2 * last_c - raw_c
                                    } else {
                                        raw_c
                                    })
                                    .clamp(0, last_c) as usize;
                                    w * data[rr * cols + cc2]
                                }
                                EdgePolicy::Dirichlet { .. } => {
                                    return Err(EvalFault::Arithmetic {
                                        op: name,
                                        detail: "2D Dirichlet boundary is not yet supported; \
                                                 use Clamp or Neumann",
                                    });
                                }
                            };
                        }
                    }
                    out.push(acc);
                }
            }
            Ok(Value::Matrix {
                rows,
                cols,
                data: out,
            })
        }
        EmirOp::MatrixAdd(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (r2, c2, d2) = matrix_of(registers, right, name)?;
            require_same_matrix_shape(r1, c1, r2, c2, name)?;
            let data = d1.iter().zip(d2.iter()).map(|(a, b)| a + b).collect();
            Ok(Value::Matrix {
                rows: r1,
                cols: c1,
                data,
            })
        }
        EmirOp::MatrixSub(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (r2, c2, d2) = matrix_of(registers, right, name)?;
            require_same_matrix_shape(r1, c1, r2, c2, name)?;
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
            require_equal_len(v.len(), cols, name, "matrix×vector width mismatch")?;
            let mut out = Vec::with_capacity(rows);
            for r in 0..rows {
                let sum: f64 = (0..cols)
                    .map(|c| m_data[r * cols + c] * v[c])
                    .sum();
                out.push(sum);
            }
            Ok(Value::Vector(out))
        }
        EmirOp::MatrixMulMatrix(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (r2, c2, d2) = matrix_of(registers, right, name)?;
            if c1 != r2 {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "matrix product inner dimensions mismatch",
                });
            }
            let out_len = r1.checked_mul(c2).ok_or(EvalFault::Arithmetic {
                op: name,
                detail: "matrix product size overflow",
            })?;
            let mut out = Vec::with_capacity(out_len);
            for i in 0..r1 {
                for j in 0..c2 {
                    let sum: f64 = (0..c1)
                        .map(|k| d1[i * c1 + k] * d2[k * c2 + j])
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
            let len = rows.checked_mul(cols).ok_or(EvalFault::Arithmetic {
                op: name,
                detail: "matrix size overflow",
            })?;
            let mut out = vec![0.0; len];
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
            let expected = shape_product(shape).ok_or(EvalFault::Arithmetic {
                op: name,
                detail: "tensor size overflow",
            })?;
            if elements.len() != expected {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "tensor element count does not match shape product",
                });
            }
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
                offset = offset
                    .checked_mul(shape[axis])
                    .and_then(|base| base.checked_add(i))
                    .ok_or(EvalFault::Arithmetic {
                        op: name,
                        detail: "tensor index offset overflow",
                    })?;
            }
            data.get(offset)
                .copied()
                .map(Value::F64)
                .ok_or(EvalFault::IndexOutOfBounds {
                    op: name,
                    index: i64::try_from(offset).unwrap_or(i64::MAX),
                    len: data.len(),
                })
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
            // Bounds must be finite whole numbers; bare `as i64` maps NaN→0 and
            // Inf→saturating extremes, which silently runs the wrong loop.
            let start_i = finite_whole_i64(start_val, start.0, name)?;
            let end_i = finite_whole_i64(end_val, end.0, name)?;
            match combine {
                FoldCombine::Add | FoldCombine::Mul => {
                    let mut acc_i: Option<i64> = match register(registers, init)? {
                        Value::I64(n) => Some(*n),
                        Value::F64(_) => None,
                        _ => return Err(EvalFault::TypeConfusion { register: init.0, op: name }),
                    };
                    let mut acc_f: f64 = if acc_i.is_none() {
                        f64_of(registers, init, name)?
                    } else { 0.0 };
                    for i in start_i..end_i {
                        let mut body_inputs = inputs.to_vec();
                        let idx = usize::from(loop_var_index);
                        while body_inputs.len() <= idx {
                            body_inputs.push(Value::F64(0.0));
                        }
                        body_inputs[idx] = if acc_i.is_some() {
                            Value::I64(i)
                        } else {
                            Value::F64(i as f64)
                        };
                        match evaluate(body, &body_inputs, state)? {
                            Value::I64(term) => {
                                if let Some(ref mut acc) = acc_i {
                                    *acc = match combine {
                                        FoldCombine::Add => *acc + term,
                                        FoldCombine::Mul => *acc * term,
                                        FoldCombine::And | FoldCombine::Or => {
                                            return Err(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "numeric fold got bool combine",
                                            });
                                        }
                                    };
                                } else {
                                    let term_f = term as f64;
                                    acc_f = match combine {
                                        FoldCombine::Add => acc_f + term_f,
                                        FoldCombine::Mul => acc_f * term_f,
                                        FoldCombine::And | FoldCombine::Or => {
                                            return Err(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "numeric fold got bool combine",
                                            });
                                        }
                                    };
                                }
                            }
                            Value::F64(term) => {
                                if let Some(acc) = acc_i.take() {
                                    acc_f = acc as f64;
                                }
                                acc_f = match combine {
                                    FoldCombine::Add => acc_f + term,
                                    FoldCombine::Mul => acc_f * term,
                                    FoldCombine::And | FoldCombine::Or => {
                                        return Err(EvalFault::Arithmetic {
                                            op: name,
                                            detail: "numeric fold got bool combine",
                                        });
                                    }
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
                    Ok(match acc_i {
                        Some(n) => Value::I64(n),
                        None => Value::F64(acc_f),
                    })
                }
                FoldCombine::And | FoldCombine::Or => {
                    // `bool_of` admits Bool and numeric 0/≠0; bare `f64_of`
                    // wrongly refused a Bool vacuous init for forall/exists.
                    let mut acc = bool_of(registers, init, name)?;
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
                            FoldCombine::Add | FoldCombine::Mul => {
                                return Err(EvalFault::Arithmetic {
                                    op: name,
                                    detail: "bool fold got numeric combine",
                                });
                            }
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
            // Composite Simpson requires a positive even panel count; steps==0
            // is `/ 0.0` → Inf, and odd n is a silently wrong quadrature.
            if steps == 0 || steps % 2 != 0 {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "integral steps must be positive and even",
                });
            }
            let a = f64_of(registers, start, name)?;
            let b = f64_of(registers, end, name)?;
            let n = i64::from(steps);
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
                // A vanished derivative is not convergence: Newton cannot
                // step, so returning `x` would silently invent a root.
                if df.abs() < 1e-30 {
                    return Err(EvalFault::Arithmetic {
                        op: name,
                        detail: "solve derivative vanished before convergence",
                    });
                }
                x -= f / df;
            }
            // Accept a root landed by the final Newton update; otherwise
            // refuse rather than invent one (same rule as causal_newton).
            work_inputs[var_index as usize] = Value::F64(x);
            let dual = evaluate_dual(body, &work_inputs, state, var_index, name)?;
            if dual.primal.abs() < tolerance {
                return Ok(Value::F64(x));
            }
            Err(EvalFault::Arithmetic {
                op: name,
                detail: "solve did not converge within max_iter",
            })
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
            if var_indices.is_empty() {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "optimize requires at least one variable",
                });
            }
            let mut work_inputs = inputs.to_vec();
            let sign = if maximize { 1.0 } else { -1.0 };
            // Extract initial guesses from inputs; refuse silent 0.0 defaults.
            let mut x: Vec<f64> = Vec::with_capacity(var_indices.len());
            for &vi in var_indices {
                match inputs.get(vi as usize) {
                    Some(Value::F64(v)) => x.push(*v),
                    Some(_) => {
                        return Err(EvalFault::TypeConfusion {
                            register: u32::from(vi),
                            op: name,
                        });
                    }
                    None => return Err(EvalFault::MissingInput(vi)),
                }
            }
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
            // Accept stationarity reached by the final gradient step.
            let mut max_grad = 0.0f64;
            for (i, &vi) in var_indices.iter().enumerate() {
                work_inputs[vi as usize] = Value::F64(x[i]);
                let dual = evaluate_dual(body, &work_inputs, state, vi, name)?;
                max_grad = max_grad.max(dual.tangent.abs());
            }
            if max_grad < tolerance {
                return Ok(Value::F64(x[0]));
            }
            Err(EvalFault::Arithmetic {
                op: name,
                detail: "optimize did not converge within max_iter",
            })
        }
    }
}

// --- Helper functions extracted to interp/helpers.rs ---
// --- Dual-number autodiff subsystem extracted to interp/dual.rs ---

