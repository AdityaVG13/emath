use super::value::{EvalFault, Value};
use crate::{EmirSliceAxis, EmirValue};

pub(super) fn eq_ne(
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
        (Value::I64(left), Value::F64(right)) => emath_rt::eq_i64_f64(*left, *right),
        (Value::F64(left), Value::I64(right)) => emath_rt::eq_i64_f64(*right, *left),
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Bool(left), Value::F64(right)) => *left == (*right != 0.0),
        (Value::F64(left), Value::Bool(right)) => (*left != 0.0) == *right,
        (Value::Complex { re: r1, im: i1 }, Value::Complex { re: r2, im: i2 }) => {
            r1 == r2 && i1 == i2
        }
        (Value::Complex { re, im }, Value::F64(right)) => *im == 0.0 && re == right,
        (Value::F64(left), Value::Complex { re, im }) => *im == 0.0 && left == re,
        (Value::Complex { re, im }, Value::I64(right)) => {
            *im == 0.0 && emath_rt::eq_i64_f64(*right, *re)
        }
        (Value::I64(left), Value::Complex { re, im }) => {
            *im == 0.0 && emath_rt::eq_i64_f64(*left, *re)
        }
        (Value::Vector(left), Value::Vector(right)) => left == right,
        (
            Value::Matrix {
                rows: r1,
                cols: c1,
                data: d1,
            },
            Value::Matrix {
                rows: r2,
                cols: c2,
                data: d2,
            },
        ) => r1 == r2 && c1 == c2 && d1 == d2,
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

pub(super) fn register(registers: &[Value], value: EmirValue) -> Result<&Value, EvalFault> {
    registers
        .get(value.0 as usize)
        .ok_or(EvalFault::BadRegister(value.0))
}

pub(super) fn map_index_error(op: &'static str, err: emath_rt::IndexError) -> EvalFault {
    match err {
        emath_rt::IndexError::OutOfBounds { index, len } => {
            EvalFault::IndexOutOfBounds { op, index, len }
        }
        emath_rt::IndexError::Arithmetic(detail) => EvalFault::Arithmetic { op, detail },
    }
}

/// Fold-bound → `i64` only when finite and whole; lossy `as i64` (NaN→0,
/// Inf saturation, out-of-range) is refused so loops run the right range.
pub(super) fn finite_whole_i64(
    raw: f64,
    register: u32,
    op: &'static str,
) -> Result<i64, EvalFault> {
    if !raw.is_finite() || raw.fract() != 0.0 {
        return Err(EvalFault::TypeConfusion { register, op });
    }
    if raw < i64::MIN as f64 || raw > i64::MAX as f64 {
        return Err(EvalFault::TypeConfusion { register, op });
    }
    Ok(raw as i64)
}

pub(super) fn require_equal_len(
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

pub(super) fn require_same_matrix_shape(
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

pub(super) fn f64_of(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
) -> Result<f64, EvalFault> {
    match register(registers, value)? {
        Value::F64(number) => Ok(*number),
        Value::I64(number) => Ok(*number as f64),
        Value::Complex { re, im } if *im == 0.0 => Ok(*re),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

/// Integer kernels (`factorial`, `mod_inv`, …) need a real i64. I64 is
/// as-is; F64 must be finite and whole — bare `as i64` maps NaN→0,
/// Inf→saturating extremes, and subnormals→0, which are silent finite lies.
pub(super) fn i64_of(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
) -> Result<i64, EvalFault> {
    match register(registers, value)? {
        Value::I64(number) => Ok(*number),
        Value::F64(number) => finite_whole_i64(*number, value.0, op),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

pub(super) fn i64_checked(
    a: i64,
    b: i64,
    op: &'static str,
    f: impl FnOnce(i64, i64) -> Option<i64>,
) -> Result<Value, EvalFault> {
    f(a, b).map(Value::I64).ok_or(EvalFault::Arithmetic {
        op,
        detail: "i64 overflow",
    })
}

/// Integer-first comparison: I64×I64 stays exact; mixed I64×F64 (and
/// real Complex) is exact, not a widening round past 2^53. Same-kind
/// F64 stays IEEE.
pub(super) fn ord_cmp(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    pred: impl Fn(core::cmp::Ordering) -> bool,
    on_f64: impl FnOnce(f64, f64) -> bool,
) -> Result<Value, EvalFault> {
    match (register(registers, left)?, register(registers, right)?) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::Bool(pred(a.cmp(b)))),
        (Value::I64(a), Value::F64(b)) => Ok(Value::Bool(
            emath_rt::cmp_i64_f64(*a, *b).is_some_and(&pred),
        )),
        (Value::F64(a), Value::I64(b)) => Ok(Value::Bool(
            emath_rt::cmp_i64_f64(*b, *a)
                .map(core::cmp::Ordering::reverse)
                .is_some_and(&pred),
        )),
        (Value::I64(a), Value::Complex { re, im }) if *im == 0.0 => Ok(Value::Bool(
            emath_rt::cmp_i64_f64(*a, *re).is_some_and(&pred),
        )),
        (Value::Complex { re, im }, Value::I64(b)) if *im == 0.0 => Ok(Value::Bool(
            emath_rt::cmp_i64_f64(*b, *re)
                .map(core::cmp::Ordering::reverse)
                .is_some_and(&pred),
        )),
        _ => Ok(Value::Bool(on_f64(
            f64_of(registers, left, op)?,
            f64_of(registers, right, op)?,
        ))),
    }
}

/// Fold start/end bound: I64 as-is so 2^53+1 is not rounded through f64.
pub(super) fn fold_bound(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
) -> Result<i64, EvalFault> {
    match register(registers, value)? {
        Value::I64(n) => Ok(*n),
        _ => finite_whole_i64(f64_of(registers, value, op)?, value.0, op),
    }
}

/// Extract a complex (re, im) pair from a register. F64 and I64 values
/// are promoted to complex with zero imaginary part.
pub(super) fn complex_of(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
) -> Result<(f64, f64), EvalFault> {
    match register(registers, value)? {
        Value::Complex { re, im } => Ok((*re, *im)),
        Value::F64(number) => Ok((*number, 0.0)),
        Value::I64(number) => Ok((*number as f64, 0.0)),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

pub(super) fn bool_of(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
) -> Result<bool, EvalFault> {
    match register(registers, value)? {
        Value::Bool(flag) => Ok(*flag),
        Value::F64(num) => Ok(*num != 0.0),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

pub(super) fn vector_of<'a>(
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

pub(super) fn matrix_of<'a>(
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

pub(super) fn tensor_of<'a>(
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

pub(super) fn shape_product(shape: &[usize]) -> Option<usize> {
    shape
        .iter()
        .try_fold(1usize, |acc, &dim| acc.checked_mul(dim))
}

/// Chunk a flat row-major matrix into rows, for the emath-rt nested
/// representation. Callers must have validated `data.len() == rows * cols`
/// (via `matrix_of`). `cols == 0` is a 0-width matrix (`data` empty);
/// `chunks_exact(0)` panics, and the nested form cannot recover row count.
pub(super) fn rows_of(data: &[f64], cols: usize) -> Vec<Vec<f64>> {
    if cols == 0 {
        return Vec::new();
    }
    data.chunks_exact(cols).map(|row| row.to_vec()).collect()
}

/// Flatten nested rows back into the flat row-major representation.
pub(super) fn flatten_rows(rows: &[Vec<f64>]) -> Vec<f64> {
    rows.iter().flat_map(|row| row.iter().copied()).collect()
}

pub(super) fn eval_tensor_slice(
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
    let mut rt_axes = Vec::with_capacity(axes.len());
    for axis in axes {
        match axis {
            EmirSliceAxis::Point(index) => {
                rt_axes.push(emath_rt::SliceAxis::Point(f64_of(registers, *index, name)?));
            }
            EmirSliceAxis::Range { start, end } => {
                rt_axes.push(emath_rt::SliceAxis::Range {
                    start: f64_of(registers, *start, name)?,
                    end: f64_of(registers, *end, name)?,
                });
            }
        }
    }
    match emath_rt::tensor_slice_checked(&shape, data, &rt_axes) {
        Ok((kept, out)) => Ok(match kept.as_slice() {
            [] => Value::F64(out.first().copied().unwrap_or(f64::NAN)),
            [_] => Value::Vector(out),
            [rows, cols] => Value::Matrix {
                rows: *rows,
                cols: *cols,
                data: out,
            },
            _ => Value::Tensor {
                shape: kept,
                data: out,
            },
        }),
        Err(err) => Err(map_index_error(name, err)),
    }
}

/// Einstein summation. Identities the language claims:
/// `einsum("ik,kj->ij", A, B)` is matrix product; `einsum("i,i->", u, v)`
/// is `dot`; implicit mode (no arrow) emits unique free indices in
/// alphabetical order. Repeated output labels (`"i->ii"`) write the
/// diagonal and leave off-diagonal zeros. Size-1 axes broadcast;
/// unequal non-broadcast extents are a typed fault. The contraction
/// lives in `emath-rt`; this wrapper maps values and typed errors.
pub(super) fn eval_einsum(
    registers: &[Value],
    subscripts: &str,
    inputs: &[EmirValue],
    name: &'static str,
) -> Result<Value, EvalFault> {
    let mut operands = Vec::with_capacity(inputs.len());
    for &v in inputs {
        let val = register(registers, v)?;
        let (shape, data) = match val {
            Value::Vector(d) => (vec![d.len()], d.clone()),
            Value::Matrix { rows, cols, data } => (vec![*rows, *cols], data.clone()),
            Value::Tensor { shape, data } => (shape.clone(), data.clone()),
            _ => {
                return Err(EvalFault::TypeConfusion {
                    register: v.0,
                    op: name,
                });
            }
        };
        operands.push((shape, data));
    }

    match emath_rt::einsum_checked(subscripts, &operands) {
        Ok((shape, data)) => Ok(match shape.len() {
            0 => Value::F64(data.first().copied().unwrap_or(0.0)),
            1 => Value::Vector(data),
            2 => Value::Matrix {
                rows: shape.first().copied().unwrap_or(0),
                cols: shape.get(1).copied().unwrap_or(0),
                data,
            },
            _ => Value::Tensor { shape, data },
        }),
        Err(emath_rt::EinsumError::Arithmetic(detail)) => {
            Err(EvalFault::Arithmetic { op: name, detail })
        }
        Err(emath_rt::EinsumError::IndexOutOfBounds { index, len }) => {
            Err(EvalFault::IndexOutOfBounds {
                op: name,
                index,
                len,
            })
        }
    }
}
