use crate::{EmirSliceAxis, EmirValue};
use super::value::{EvalFault, Value};

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
        (Value::I64(left), Value::F64(right)) => (*left as f64) == *right,
        (Value::F64(left), Value::I64(right)) => *left == (*right as f64),
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Bool(left), Value::F64(right)) => *left == (*right != 0.0),
        (Value::F64(left), Value::Bool(right)) => (*left != 0.0) == *right,
        (Value::Complex { re: r1, im: i1 }, Value::Complex { re: r2, im: i2 }) => {
            r1 == r2 && i1 == i2
        }
        (Value::Complex { re, im }, Value::F64(right)) => *im == 0.0 && re == right,
        (Value::F64(left), Value::Complex { re, im }) => *im == 0.0 && left == re,
        (Value::Complex { re, im }, Value::I64(right)) => *im == 0.0 && re == &(*right as f64),
        (Value::I64(left), Value::Complex { re, im }) => *im == 0.0 && &(*left as f64) == re,
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

pub(super) fn register(registers: &[Value], value: EmirValue) -> Result<&Value, EvalFault> {
    registers
        .get(value.0 as usize)
        .ok_or(EvalFault::BadRegister(value.0))
}

pub(super) fn whole_index(
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

/// Fold-bound → `i64` only when finite and whole; lossy `as i64` (NaN→0,
/// Inf saturation, out-of-range) is refused so loops run the right range.
pub(super) fn finite_whole_i64(raw: f64, register: u32, op: &'static str) -> Result<i64, EvalFault> {
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

pub(super) fn f64_of(registers: &[Value], value: EmirValue, op: &'static str) -> Result<f64, EvalFault> {
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

pub(super) fn i64_of(registers: &[Value], value: EmirValue, op: &'static str) -> Result<i64, EvalFault> {
    match register(registers, value)? {
        Value::I64(number) => Ok(*number),
        Value::F64(number) => Ok(*number as i64),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

/// Extract a complex (re, im) pair from a register. F64 and I64 values
/// are promoted to complex with zero imaginary part.
pub(super) fn complex_of(registers: &[Value], value: EmirValue, op: &'static str) -> Result<(f64, f64), EvalFault> {
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

pub(super) fn bool_of(registers: &[Value], value: EmirValue, op: &'static str) -> Result<bool, EvalFault> {
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

pub(super) fn collect_slice(
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

pub(super) fn shape_product(shape: &[usize]) -> Option<usize> {
    shape.iter().try_fold(1usize, |acc, &dim| acc.checked_mul(dim))
}

/// Chunk a flat row-major matrix into rows, for the emath-rt nested
/// representation. Callers must have validated `data.len() == rows * cols`
/// (via `matrix_of`).
pub(super) fn rows_of(data: &[f64], cols: usize) -> Vec<Vec<f64>> {
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
