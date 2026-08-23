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

use crate::{EdgePolicy, EmirOp, EmirProgram, EmirSliceAxis, EmirValue, FoldCombine};
use std::fmt;

mod dual;

use dual::evaluate_dual;

/// A typed register value. Locals match generated Rust (`f64` / `bool` / `Vec<f64>`).
#[derive(Clone, Debug)]
pub enum Value {
    /// IEEE-754 binary64.
    F64(f64),
    /// Signed 64-bit integer (exact arithmetic in folds).
    I64(i64),
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
            (Self::I64(left), Self::I64(right)) => left == right,
            (Self::I64(left), Self::F64(right)) => (*left as f64) == *right,
            (Self::F64(left), Self::I64(right)) => *left == (*right as f64),
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
            Self::I64(value) => write!(f, "{value}"),
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
    /// Op violated an arithmetic precondition (zero/odd Simpson steps,
    /// index-offset overflow, etc.). Distinct from IEEE `/0` on f64 ops,
    /// which remains Inf/NaN to match generated Rust.
    Arithmetic {
        /// EMIR op name.
        op: &'static str,
        /// Short reason (`integral steps must be positive and even`).
        detail: &'static str,
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
            Self::Arithmetic { op, detail } => write!(f, "{op}: {detail}"),
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

// --- Dual-number autodiff subsystem extracted to interp/dual.rs ---

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
        (Value::I64(left), Value::I64(right)) => left == right,
        (Value::I64(left), Value::F64(right)) => (*left as f64) == *right,
        (Value::F64(left), Value::I64(right)) => *left == (*right as f64),
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
            index: i64::try_from(index).unwrap_or(i64::MAX),
            len,
        });
    }
    Ok(index)
}

/// Convert a fold bound to `i64` only when it is a finite whole number.
///
/// Lossy `as i64` is refused: NaN becomes 0 and ±Inf saturate, which would
/// otherwise make `sum`/`product`/`forall`/`exists` loops run the wrong range.
/// Values outside the `i64` range are also refused rather than saturating.
fn finite_whole_i64(raw: f64, register: u32, op: &'static str) -> Result<i64, EvalFault> {
    if !raw.is_finite() || raw.fract() != 0.0 {
        return Err(EvalFault::TypeConfusion { register, op });
    }
    if raw < i64::MIN as f64 || raw > i64::MAX as f64 {
        return Err(EvalFault::TypeConfusion { register, op });
    }
    Ok(raw as i64)
}

fn require_equal_len(
    left: usize,
    right: usize,
    op: &'static str,
    detail: &'static str,
) -> Result<(), EvalFault> {
    if left != right {
        return Err(EvalFault::Arithmetic { op, detail });
    }
    Ok(())
}

fn require_same_matrix_shape(
    r1: usize,
    c1: usize,
    r2: usize,
    c2: usize,
    op: &'static str,
) -> Result<(), EvalFault> {
    if r1 != r2 || c1 != c2 {
        return Err(EvalFault::Arithmetic {
            op,
            detail: "matrix shape mismatch",
        });
    }
    Ok(())
}

fn f64_of(registers: &[Value], value: EmirValue, op: &'static str) -> Result<f64, EvalFault> {
    match register(registers, value)? {
        Value::F64(number) => Ok(*number),
        Value::I64(number) => Ok(*number as f64),
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
        Value::Matrix { rows, cols, data } => {
            let expected = rows.checked_mul(*cols).ok_or(EvalFault::Arithmetic {
                op,
                detail: "matrix size overflow",
            })?;
            if data.len() != expected {
                return Err(EvalFault::Arithmetic {
                    op,
                    detail: "matrix data length does not match rows*cols",
                });
            }
            Ok((*rows, *cols, data.as_slice()))
        }
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
        Value::Tensor { shape, data } => {
            let expected = shape_product(shape).ok_or(EvalFault::Arithmetic {
                op,
                detail: "tensor size overflow",
            })?;
            if data.len() != expected {
                return Err(EvalFault::Arithmetic {
                    op,
                    detail: "tensor data length does not match shape product",
                });
            }
            Ok((shape.as_slice(), data.as_slice()))
        }
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
) -> Result<(), EvalFault> {
    if axis == shape.len() {
        let value = data.get(offset).copied().ok_or(EvalFault::IndexOutOfBounds {
            op: "tensor-slice",
            index: i64::try_from(offset).unwrap_or(i64::MAX),
            len: data.len(),
        })?;
        out.push(value);
        return Ok(());
    }
    let stride = shape[axis + 1..].iter().product::<usize>().max(1);
    for i in 0..out_shape[axis] {
        let next = offset
            .checked_add(
                starts[axis]
                    .checked_add(i)
                    .and_then(|idx| idx.checked_mul(stride))
                    .ok_or(EvalFault::Arithmetic {
                        op: "tensor-slice",
                        detail: "tensor slice offset overflow",
                    })?,
            )
            .ok_or(EvalFault::Arithmetic {
                op: "tensor-slice",
                detail: "tensor slice offset overflow",
            })?;
        collect_slice(data, shape, starts, out_shape, axis + 1, next, out)?;
    }
    Ok(())
}

fn shape_product(shape: &[usize]) -> Option<usize> {
    shape.iter().try_fold(1usize, |acc, &dim| acc.checked_mul(dim))
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
                        index: i64::try_from(end_i).unwrap_or(i64::MAX),
                        len: shape[axis],
                    });
                }
                starts.push(start_i);
                out_shape.push(end_i - start_i);
            }
        }
    }
    let expected = shape_product(&shape).ok_or(EvalFault::Arithmetic {
        op: name,
        detail: "tensor size overflow",
    })?;
    if data.len() != expected {
        return Err(EvalFault::Arithmetic {
            op: name,
            detail: "tensor/matrix data length does not match shape",
        });
    }
    let mut out = Vec::new();
    collect_slice(data, &shape, &starts, &out_shape, 0, 0, &mut out)?;
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

