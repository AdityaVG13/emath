//! Interpreter for [`EmirProgram`]. Typed registers (`f64` / `i64` /
//! `bool` / vectors / ...); type confusion is a typed fault, never a
//! silent coercion. I64 add/sub/mul/neg stay exact (overflow is a fault).
//! Mixed I64×F64 arithmetic widens to f64; mixed comparisons are exact
//! (not a 2^53 widening round). Same-kind F64 comparisons are IEEE-754;
//! transcendentals follow platform libm (same caveat as generated Rust),
//! and domain obligations are assumptions, not runtime checks.

use crate::{BuiltinId, EdgePolicy, EmirOp, EmirProgram, FoldCombine};

mod dual;
mod helpers;
mod reverse;
mod value;

use dual::evaluate_dual;
use helpers::*;
use reverse::evaluate_reverse;
pub use value::{format_f64, EvalFault, Value};

/// Evaluate `program` in one forward pass; slots are indexed by
/// [`EmirOp::LoadInput`] / [`EmirOp::LoadState`], missing slots are
/// faults, IEEE-754 exceptions are not. `And`/`Or` evaluate both operands
/// (registers already materialized), matching the Rust backend.
pub fn evaluate(
    program: &EmirProgram,
    inputs: &[Value],
    state: &[Value],
) -> Result<Value, EvalFault> {
    let mut registers: Vec<Value> = Vec::with_capacity(program.ops.len());
    for (op, _) in &program.ops {
        let value = eval_op(op, &registers, inputs, state)?;
        registers.push(value);
    }
    register(&registers, program.result).cloned()
}

/// Convenience for scalar-only programs (existing tests and given maps).
pub fn evaluate_f64(
    program: &EmirProgram,
    inputs: &[f64],
    state: &[f64],
) -> Result<Value, EvalFault> {
    let inputs: Vec<Value> = inputs.iter().copied().map(Value::F64).collect();
    let state: Vec<Value> = state.iter().copied().map(Value::F64).collect();
    evaluate(program, &inputs, &state)
}

pub(super) fn eval_op(
    op: &EmirOp,
    registers: &[Value],
    inputs: &[Value],
    state: &[Value],
) -> Result<Value, EvalFault> {
    let name = op.name();
    match *op {
        EmirOp::ConstF64(bits) => Ok(Value::F64(f64::from_bits(bits))),
        EmirOp::ConstI64(value) => Ok(Value::I64(value)),
        EmirOp::ConstComplex(re, im) => Ok(Value::Complex { re, im }),
        EmirOp::ConstBool(value) => Ok(Value::Bool(value)),
        EmirOp::LoadInput(index) => inputs
            .get(usize::from(index))
            .cloned()
            .ok_or(EvalFault::MissingInput(index)),
        EmirOp::LoadState(index) => state
            .get(usize::from(index))
            .cloned()
            .ok_or(EvalFault::MissingState(index)),
        EmirOp::F64Add(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    Ok(Value::Complex {
                        re: lr + rr,
                        im: li + ri,
                    })
                }
                (Value::I64(a), Value::I64(b)) => i64_checked(*a, *b, name, i64::checked_add),
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? + f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Sub(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    Ok(Value::Complex {
                        re: lr - rr,
                        im: li - ri,
                    })
                }
                (Value::I64(a), Value::I64(b)) => i64_checked(*a, *b, name, i64::checked_sub),
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? - f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Mul(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    // (a + bi)(c + di) = (ac - bd) + (ad + bc)i
                    Ok(Value::Complex {
                        re: lr * rr - li * ri,
                        im: lr * ri + li * rr,
                    })
                }
                (Value::I64(a), Value::I64(b)) => i64_checked(*a, *b, name, i64::checked_mul),
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? * f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Div(left, right) => {
            let l = register(registers, left)?;
            let r = register(registers, right)?;
            match (l, r) {
                (Value::Complex { .. }, _) | (_, Value::Complex { .. }) => {
                    let (lr, li) = complex_of(registers, left, name)?;
                    let (rr, ri) = complex_of(registers, right, name)?;
                    let denom = rr * rr + ri * ri;
                    // (a + bi) / (c + di) = ((ac + bd) + (bc - ad)i) / (c² + d²)
                    Ok(Value::Complex {
                        re: (lr * rr + li * ri) / denom,
                        im: (li * rr - lr * ri) / denom,
                    })
                }
                _ => Ok(Value::F64(
                    f64_of(registers, left, name)? / f64_of(registers, right, name)?,
                )),
            }
        }
        EmirOp::F64Pow(left, right) => Ok(Value::F64(
            f64_of(registers, left, name)?.powf(f64_of(registers, right, name)?),
        )),
        EmirOp::Neg(value) => match register(registers, value)? {
            Value::Complex { re, im } => Ok(Value::Complex { re: -*re, im: -*im }),
            Value::I64(n) => n
                .checked_neg()
                .map(Value::I64)
                .ok_or(EvalFault::Arithmetic {
                    op: name,
                    detail: "i64 overflow",
                }),
            _ => Ok(Value::F64(-f64_of(registers, value, name)?)),
        },
        EmirOp::UnaryBuiltin(id, value) => match register(registers, value)? {
            Value::Complex { re, im } => eval_complex_unary(id, *re, *im, value.0, name),
            _ => Ok(Value::F64(id.eval_unary(f64_of(registers, value, name)?))),
        }
        EmirOp::BinaryBuiltin(id, left, right) => {
            let l = f64_of(registers, left, name)?;
            let r = f64_of(registers, right, name)?;
            Ok(Value::F64(id.eval_binary(l, r)))
        }
        EmirOp::Lt(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_lt(), |a, b| a < b)
        }
        EmirOp::Le(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_le(), |a, b| a <= b)
        }
        EmirOp::Gt(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_gt(), |a, b| a > b)
        }
        EmirOp::Ge(left, right) => {
            ord_cmp(registers, left, right, name, |o| o.is_ge(), |a, b| a >= b)
        }
        EmirOp::Eq(left, right) => eq_ne(registers, left, right, name, true),
        EmirOp::Ne(left, right) => eq_ne(registers, left, right, name, false),
        EmirOp::And(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? && bool_of(registers, right, name)?,
        )),
        EmirOp::Or(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? || bool_of(registers, right, name)?,
        )),
        EmirOp::Imply(left, right) => Ok(Value::Bool(
            !bool_of(registers, left, name)? || bool_of(registers, right, name)?,
        )),
        EmirOp::Iff(left, right) => Ok(Value::Bool(
            bool_of(registers, left, name)? == bool_of(registers, right, name)?,
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
            Ok(Value::Matrix { rows, cols, data })
        }
        EmirOp::VectorIndex { vector, index } => {
            let vec = vector_of(registers, vector, name)?;
            let raw = f64_of(registers, index, name)?;
            emath_rt::vec_index_checked(vec, raw)
                .map(Value::F64)
                .map_err(|err| map_index_error(name, err))
        }
        EmirOp::MatrixIndex { matrix, row, col } => {
            let (r_count, c_count, data) = matrix_of(registers, matrix, name)?;
            let raw_r = f64_of(registers, row, name)?;
            let raw_c = f64_of(registers, col, name)?;
            emath_rt::tensor_index_checked(&[r_count, c_count], data, &[raw_r, raw_c])
                .map(Value::F64)
                .map_err(|err| map_index_error(name, err))
        }
        EmirOp::VectorAdd(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            require_equal_len(v1.len(), v2.len(), name, "vector length mismatch")?;
            Ok(Value::Vector(emath_rt::vec_add(v1, v2)))
        }
        EmirOp::VectorSub(left, right) => {
            let v1 = vector_of(registers, left, name)?;
            let v2 = vector_of(registers, right, name)?;
            require_equal_len(v1.len(), v2.len(), name, "vector length mismatch")?;
            Ok(Value::Vector(emath_rt::vec_sub(v1, v2)))
        }
        EmirOp::VectorScale(left, right) => {
            // Canonical operand order from admission: (vector, scalar).
            // Still accept (scalar, vector) so older EMIR stays evaluable.
            match (register(registers, left)?, register(registers, right)?) {
                (Value::Vector(v), Value::F64(s)) | (Value::F64(s), Value::Vector(v)) => {
                    Ok(Value::Vector(emath_rt::vec_scale(v, *s)))
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
            Ok(Value::F64(emath_rt::vec_dot(v1, v2)))
        }
        EmirOp::VectorNorm(value) => {
            let v = vector_of(registers, value, name)?;
            Ok(Value::F64(emath_rt::vec_norm(v)))
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
            let edge = match edge {
                EdgePolicy::Clamp => emath_rt::EdgePolicy::Clamp,
                EdgePolicy::Neumann => emath_rt::EdgePolicy::Neumann,
                EdgePolicy::OneSided => emath_rt::EdgePolicy::OneSided,
                EdgePolicy::Dirichlet { left, right } => {
                    emath_rt::EdgePolicy::Dirichlet { left, right }
                }
            };
            Ok(Value::Vector(emath_rt::stencil_1d(
                v,
                weights,
                center as i64,
                edge,
            )))
        }
        EmirOp::Stencil2d {
            input,
            ref weights,
            center,
            edge,
        } => {
            let (rows, cols, data) = matrix_of(registers, input, name)?;
            if matches!(edge, EdgePolicy::Dirichlet { .. }) {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "2D Dirichlet boundary is not yet supported; use Clamp, Neumann, or OneSided",
                });
            }
            let edge = match edge {
                EdgePolicy::Clamp => emath_rt::EdgePolicy::Clamp,
                EdgePolicy::Neumann => emath_rt::EdgePolicy::Neumann,
                EdgePolicy::OneSided => emath_rt::EdgePolicy::OneSided,
                EdgePolicy::Dirichlet { .. } => unreachable!("checked above"),
            };
            let nested = rows_of(data, cols);
            let w9: &[f64; 9] =
                weights
                    .as_slice()
                    .try_into()
                    .map_err(|_| EvalFault::Arithmetic {
                        op: name,
                        detail: "2D stencil weights must have length 9",
                    })?;
            let out = emath_rt::stencil_2d(&nested, w9, (center.0 as i64, center.1 as i64), edge);
            Ok(Value::Matrix {
                rows,
                cols,
                data: flatten_rows(&out),
            })
        }
        EmirOp::MatrixAdd(left, right) => {
            let (r1, c1, d1) = matrix_of(registers, left, name)?;
            let (r2, c2, d2) = matrix_of(registers, right, name)?;
            require_same_matrix_shape(r1, c1, r2, c2, name)?;
            let a = rows_of(d1, c1);
            let b = rows_of(d2, c2);
            let data = flatten_rows(&emath_rt::mat_add(&a, &b));
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
            let a = rows_of(d1, c1);
            let b = rows_of(d2, c2);
            let data = flatten_rows(&emath_rt::mat_sub(&a, &b));
            Ok(Value::Matrix {
                rows: r1,
                cols: c1,
                data,
            })
        }
        EmirOp::MatrixScale(left, right) => {
            match (register(registers, left)?, register(registers, right)?) {
                (Value::Matrix { rows, cols, data }, Value::F64(s))
                | (Value::F64(s), Value::Matrix { rows, cols, data }) => {
                    let nested = rows_of(data, *cols);
                    Ok(Value::Matrix {
                        rows: *rows,
                        cols: *cols,
                        data: flatten_rows(&emath_rt::mat_scale(&nested, *s)),
                    })
                }
                _ => Err(EvalFault::TypeConfusion {
                    register: left.0,
                    op: name,
                }),
            }
        }
        EmirOp::MatrixMulVector(matrix, vector) => {
            let (_, cols, m_data) = matrix_of(registers, matrix, name)?;
            let v = vector_of(registers, vector, name)?;
            require_equal_len(v.len(), cols, name, "matrix×vector width mismatch")?;
            let nested = rows_of(m_data, cols);
            Ok(Value::Vector(emath_rt::mat_mul_vec(&nested, v)))
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
            let a = rows_of(d1, c1);
            let b = rows_of(d2, c2);
            let data = flatten_rows(&emath_rt::mat_mul_mat(&a, &b));
            Ok(Value::Matrix {
                rows: r1,
                cols: c2,
                data,
            })
        }
        EmirOp::MatrixTranspose(value) => {
            // Flat row-major involution. Nested `Vec<Vec<f64>>` cannot
            // store a 0-column (or 0-row) extent, and `chunks_exact(0)`
            // panics, so `transpose(transpose(A))` must not go through it.
            let (rows, cols, data) = matrix_of(registers, value, name)?;
            let mut out = vec![0.0; data.len()];
            if rows > 0 && cols > 0 {
                for r in 0..rows {
                    let src = r * cols;
                    for c in 0..cols {
                        out[c * rows + r] = data[src + c];
                    }
                }
            }
            Ok(Value::Matrix {
                rows: cols,
                cols: rows,
                data: out,
            })
        }
        EmirOp::TensorCreate {
            ref shape,
            ref elements,
        } => {
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
            let mut raw = Vec::with_capacity(indices.len());
            for &index in indices {
                raw.push(f64_of(registers, index, name)?);
            }
            emath_rt::tensor_index_checked(shape, data, &raw)
                .map(Value::F64)
                .map_err(|err| map_index_error(name, err))
        }
        EmirOp::TensorSlice { tensor, ref axes } => {
            eval_tensor_slice(registers, tensor, axes, name)
        }
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
                data: emath_rt::tensor_add(d1, d2),
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
                data: emath_rt::tensor_sub(d1, d2),
            })
        }
        EmirOp::Einsum {
            ref subscripts,
            ref inputs,
        } => eval_einsum(registers, subscripts, inputs, name),
        EmirOp::Factorial(n) => {
            let n = i64_of(registers, n, name)?;
            let result = emath_rt::factorial_checked(n)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::I64(result))
        }
        EmirOp::ModInv(a, m) => {
            let a = i64_of(registers, a, name)?;
            let m = i64_of(registers, m, name)?;
            let result = emath_rt::mod_inv_checked(a, m)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::I64(result))
        }
        EmirOp::Congruence(a, b, m) => {
            let a = i64_of(registers, a, name)?;
            let b = i64_of(registers, b, name)?;
            let m = i64_of(registers, m, name)?;
            if m == 0 {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "cong: modulus must be non-zero",
                });
            }
            Ok(Value::Bool((a - b).rem_euclid(m) == 0))
        }
        EmirOp::PolyEvalMod(coeffs, x, p) => {
            let x = i64_of(registers, x, name)?;
            let p = i64_of(registers, p, name)?;
            let coeff_vec = match register(registers, coeffs)? {
                Value::Vector(data) => data,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: coeffs.0,
                        op: name,
                    });
                }
            };
            let result = emath_rt::poly_eval_mod_checked(coeff_vec, x, p)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::I64(result))
        }
        EmirOp::RSEncode(coeffs, n, p) => {
            let n = i64_of(registers, n, name)?;
            let p = i64_of(registers, p, name)?;
            let coeff_vec = match register(registers, coeffs)? {
                Value::Vector(data) => data.clone(),
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: coeffs.0,
                        op: name,
                    });
                }
            };
            let codeword = emath_rt::rs_encode_checked(&coeff_vec, n, p)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::Vector(codeword))
        }
        EmirOp::HammingDistance(a, b) => {
            let a_vec = match register(registers, a)? {
                Value::Vector(data) => data,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: a.0,
                        op: name,
                    });
                }
            };
            let b_vec = match register(registers, b)? {
                Value::Vector(data) => data,
                _ => {
                    return Err(EvalFault::TypeConfusion {
                        register: b.0,
                        op: name,
                    });
                }
            };
            let dist = emath_rt::hamming_distance_checked(a_vec, b_vec)
                .map_err(|detail| EvalFault::Arithmetic { op: name, detail })?;
            Ok(Value::I64(dist))
        }
        EmirOp::Fold {
            start,
            end,
            init,
            combine,
            loop_var_index,
            ref body,
        } => {
            // I64 bounds stay exact; F64 bounds must be finite whole numbers
            // (bare `as i64` maps NaN→0 and Inf→saturating extremes).
            let start_i = fold_bound(registers, start, name)?;
            let end_i = fold_bound(registers, end, name)?;
            match combine {
                FoldCombine::Add | FoldCombine::Mul => {
                    let mut acc_i: Option<i64> = match register(registers, init)? {
                        Value::I64(n) => Some(*n),
                        Value::F64(_) => None,
                        _ => {
                            return Err(EvalFault::TypeConfusion {
                                register: init.0,
                                op: name,
                            });
                        }
                    };
                    let mut acc_f: f64 = if acc_i.is_none() {
                        f64_of(registers, init, name)?
                    } else {
                        0.0
                    };
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
                                        FoldCombine::Add => {
                                            acc.checked_add(term).ok_or(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "i64 overflow",
                                            })?
                                        }
                                        FoldCombine::Mul => {
                                            acc.checked_mul(term).ok_or(EvalFault::Arithmetic {
                                                op: name,
                                                detail: "i64 overflow",
                                            })?
                                        }
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
                                });
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
                                });
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
                        });
                    }
                }
            }
            Ok(Value::F64(acc * h / 3.0))
        }
        EmirOp::Differentiate {
            ref body,
            var_index,
        } => {
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
            let mut x = match inputs.get(var_index as usize).and_then(Value::as_real_f64) {
                Some(v) => v,
                None => {
                    return Err(EvalFault::TypeConfusion {
                        register: var_index as u32,
                        op: name,
                    });
                }
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
            learning_rate: _,
            tolerance,
            max_iter,
        } => {
            // Newton's method on ∇f = 0. A claimed min/max must be a
            // stationary point: x -= H^{-1} ∇f, with H from a
            // forward-difference of the dual gradient. Fixed-step
            // gradient descent with a small penalty weight could stop
            // at a point that was neither stationary for f nor feasible.
            if var_indices.is_empty() {
                return Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "optimize requires at least one variable",
                });
            }
            let mut work_inputs = inputs.to_vec();
            let mut x: Vec<f64> = Vec::with_capacity(var_indices.len());
            for &vi in var_indices {
                match inputs.get(vi as usize).and_then(Value::as_real_f64) {
                    Some(v) => x.push(v),
                    None => {
                        return Err(if inputs.get(vi as usize).is_none() {
                            EvalFault::MissingInput(vi)
                        } else {
                            EvalFault::TypeConfusion {
                                register: u32::from(vi),
                                op: name,
                            }
                        });
                    }
                }
            }
            const FD_EPS: f64 = 1e-8;
            for _ in 0..max_iter {
                let grads = optimize_grads(body, &mut work_inputs, state, var_indices, &x, name)?;
                let max_grad = grads.iter().fold(0.0_f64, |acc, g| acc.max(g.abs()));
                if max_grad < tolerance {
                    return Ok(Value::F64(x[0]));
                }
                let n = x.len();
                let mut hess = vec![vec![0.0_f64; n]; n];
                for j in 0..n {
                    x[j] += FD_EPS;
                    let perturbed =
                        optimize_grads(body, &mut work_inputs, state, var_indices, &x, name)?;
                    x[j] -= FD_EPS;
                    for i in 0..n {
                        hess[i][j] = (perturbed[i] - grads[i]) / FD_EPS;
                    }
                }
                let delta = dense_solve(&hess, &grads).map_err(|_| EvalFault::Arithmetic {
                    op: name,
                    detail: "optimize hessian vanished before stationarity",
                })?;
                let dot: f64 = grads.iter().zip(delta.iter()).map(|(g, d)| g * d).sum();
                // Newton on ∇f = 0 finds any stationary point. Refuse a
                // min returned as a max (or vice versa): g·(H^{-1}g) is
                // positive iff H is positive definite along g.
                if maximize {
                    if dot >= 0.0 {
                        return Err(EvalFault::Arithmetic {
                            op: name,
                            detail: "optimize hessian has the wrong curvature for maximize",
                        });
                    }
                } else if dot <= 0.0 {
                    return Err(EvalFault::Arithmetic {
                        op: name,
                        detail: "optimize hessian has the wrong curvature for minimize",
                    });
                }
                for (xi, d) in x.iter_mut().zip(delta.iter()) {
                    *xi -= d;
                }
            }
            let grads = optimize_grads(body, &mut work_inputs, state, var_indices, &x, name)?;
            let max_grad = grads.iter().fold(0.0_f64, |acc, g| acc.max(g.abs()));
            if max_grad < tolerance {
                return Ok(Value::F64(x[0]));
            }
            Err(EvalFault::Arithmetic {
                op: name,
                detail: "optimize did not converge within max_iter",
            })
        }
        EmirOp::SampleLimit {
            ref body,
            var_index,
            target,
            direction,
        } => {
            // Numerical limit approximation: sample the body at points
            // approaching the target along a geometric sequence of step
            // sizes (0.1, 0.01, ..., 1e-12). Return the last finite value
            // whose predecessor was also finite and within 1% of it
            // (convergence check). If no pair converges, return the last
            // finite sample.
            let target_val = match registers
                .get(target.0 as usize)
                .and_then(Value::as_real_f64)
            {
                Some(v) => v,
                None => {
                    return Err(EvalFault::TypeConfusion {
                        register: target.0,
                        op: name,
                    });
                }
            };
            let dir_val = match registers
                .get(direction.0 as usize)
                .and_then(Value::as_real_f64)
            {
                Some(v) => v,
                None => {
                    return Err(EvalFault::TypeConfusion {
                        register: direction.0,
                        op: name,
                    });
                }
            };
            let mut work_inputs = inputs.to_vec();
            while work_inputs.len() <= var_index as usize {
                work_inputs.push(Value::F64(0.0));
            }
            let directions: &[f64] = match dir_val as i64 {
                0 => &[1.0, -1.0], // two-sided
                1 => &[1.0],       // from above
                -1 => &[-1.0],     // from below
                _ => &[1.0, -1.0], // fallback: two-sided
            };
            let mut best = f64::NAN;
            let mut prev = f64::NAN;
            for step_exp in 1..=12u32 {
                let h = 10f64.powi(-(step_exp as i32));
                for &d in directions {
                    let x = target_val + d * h;
                    work_inputs[var_index as usize] = Value::F64(x);
                    match evaluate(body, &work_inputs, state) {
                        Ok(val) => {
                            if let Some(fx) = val.as_real_f64() {
                                if fx.is_finite() {
                                    if prev.is_finite()
                                        && (fx - prev).abs() <= fx.abs() * 0.01 + 1e-14
                                    {
                                        // Converged: successive samples agree to 1%.
                                        return Ok(Value::F64(fx));
                                    }
                                    prev = fx;
                                    best = fx;
                                }
                            }
                        }
                        _ => {} // non-finite or wrong type: skip
                    }
                }
            }
            if best.is_finite() {
                Ok(Value::F64(best))
            } else {
                Err(EvalFault::Arithmetic {
                    op: name,
                    detail: "sample_limit produced no finite values",
                })
            }
        }
        EmirOp::ReverseMode {
            ref body,
            ref var_indices,
        } => evaluate_reverse(body, inputs, state, var_indices, name),
    }
}

fn optimize_grads(
    body: &EmirProgram,
    work_inputs: &mut [Value],
    state: &[Value],
    var_indices: &[u16],
    x: &[f64],
    name: &'static str,
) -> Result<Vec<f64>, EvalFault> {
    for (i, &vi) in var_indices.iter().enumerate() {
        work_inputs[vi as usize] = Value::F64(x[i]);
    }
    let mut grads = Vec::with_capacity(var_indices.len());
    for &vi in var_indices {
        let dual = evaluate_dual(body, work_inputs, state, vi, name)?;
        grads.push(dual.tangent);
    }
    Ok(grads)
}

/// Solve `matrix * x = rhs` by Gaussian elimination with partial pivoting.
fn dense_solve(matrix: &[Vec<f64>], rhs: &[f64]) -> Result<Vec<f64>, ()> {
    let n = rhs.len();
    if matrix.len() != n || matrix.iter().any(|row| row.len() != n) {
        return Err(());
    }
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut a = matrix.to_vec();
    let mut b = rhs.to_vec();
    for col in 0..n {
        let mut pivot = col;
        let mut best = a[col][col].abs();
        for row in (col + 1)..n {
            let candidate = a[row][col].abs();
            if candidate > best {
                best = candidate;
                pivot = row;
            }
        }
        if best < 1e-30 {
            return Err(());
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in (col + 1)..n {
            let factor = a[row][col] / a[col][col];
            if factor == 0.0 {
                continue;
            }
            for k in col..n {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut acc = b[row];
        for k in (row + 1)..n {
            acc -= a[row][k] * x[k];
        }
        x[row] = acc / a[row][row];
    }
    Ok(x)
}

fn eval_complex_unary(
    id: BuiltinId,
    re: f64,
    im: f64,
    register: u32,
    op: &'static str,
) -> Result<Value, EvalFault> {
    match id {
        BuiltinId::Sqrt => {
            let (out_re, out_im) = emath_rt::complex_sqrt(re, im);
            Ok(Value::Complex {
                re: out_re,
                im: out_im,
            })
        }
        BuiltinId::Ln => {
            let (out_re, out_im) = emath_rt::complex_ln(re, im);
            Ok(Value::Complex {
                re: out_re,
                im: out_im,
            })
        }
        BuiltinId::Exp => {
            let (out_re, out_im) = emath_rt::complex_exp(re, im);
            Ok(Value::Complex {
                re: out_re,
                im: out_im,
            })
        }
        BuiltinId::Log10 => {
            let (out_re, out_im) = emath_rt::complex_ln(re, im);
            let scale = std::f64::consts::LN_10;
            Ok(Value::Complex {
                re: out_re / scale,
                im: out_im / scale,
            })
        }
        BuiltinId::Log2 => {
            let (out_re, out_im) = emath_rt::complex_ln(re, im);
            let scale = std::f64::consts::LN_2;
            Ok(Value::Complex {
                re: out_re / scale,
                im: out_im / scale,
            })
        }
        BuiltinId::Abs => Ok(Value::F64(re.hypot(im))),
        BuiltinId::Recip => {
            let denom = re * re + im * im;
            Ok(Value::Complex {
                re: re / denom,
                im: -im / denom,
            })
        }
        _ => Err(EvalFault::TypeConfusion { register, op }),
    }
}

// --- Helper functions extracted to interp/helpers.rs ---
// --- Dual-number autodiff subsystem extracted to interp/dual.rs ---

// Extended GCD moved to crates/emath-rt/src/body.rs (mod_inv_checked).
