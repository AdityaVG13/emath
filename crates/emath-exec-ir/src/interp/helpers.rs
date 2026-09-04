use super::value::{EvalFault, Value};
use crate::{EmirSliceAxis, EmirValue};

pub(super) fn register(registers: &[Value], value: EmirValue) -> Result<&Value, EvalFault> {
    registers
        .get(value.0 as usize)
        .ok_or(EvalFault::BadRegister(value.0))
}

pub(super) fn f64_of(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
) -> Result<f64, EvalFault> {
    match register(registers, value)? {
        Value::F64(value) => Ok(*value),
        Value::I64(value) => Ok(*value as f64),
        Value::Complex { re, im } if *im == 0.0 => Ok(*re),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

pub(super) fn i64_of(
    registers: &[Value],
    value: EmirValue,
    op: &'static str,
) -> Result<i64, EvalFault> {
    match register(registers, value)? {
        Value::I64(value) => Ok(*value),
        Value::F64(value)
            if value.is_finite()
                && value.fract() == 0.0
                && *value >= i64::MIN as f64
                && *value <= i64::MAX as f64 =>
        {
            Ok(*value as i64)
        }
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
        Value::Bool(value) => Ok(*value),
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
        Value::Vector(values) => Ok(values),
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
        Value::Matrix { rows, cols, data } if rows.checked_mul(*cols) == Some(data.len()) => {
            Ok((*rows, *cols, data))
        }
        Value::Matrix { .. } => Err(EvalFault::Arithmetic {
            op,
            detail: "matrix storage does not match shape",
        }),
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
        Value::Tensor { shape, data }
            if shape
                .iter()
                .try_fold(1usize, |size, axis| size.checked_mul(*axis))
                == Some(data.len()) =>
        {
            Ok((shape, data))
        }
        Value::Tensor { .. } => Err(EvalFault::Arithmetic {
            op,
            detail: "tensor storage does not match shape",
        }),
        _ => Err(EvalFault::TypeConfusion {
            register: value.0,
            op,
        }),
    }
}

pub(super) fn eq_ne(
    registers: &[Value],
    left: EmirValue,
    right: EmirValue,
    op: &'static str,
    equal: bool,
) -> Result<Value, EvalFault> {
    let left_value = register(registers, left)?;
    let right_value = register(registers, right)?;
    let result = match (left_value, right_value) {
        (Value::F64(left), Value::F64(right)) => left == right,
        (
            Value::Complex { re: left_re, im: left_im },
            Value::Complex { re: right_re, im: right_im },
        ) => left_re == right_re && left_im == right_im,
        _ => left_value == right_value,
    };
    Ok(Value::Bool(if equal { result } else { !result }))
}

pub(super) fn eval_tensor_slice(
    registers: &[Value],
    tensor: EmirValue,
    axes: &[EmirSliceAxis],
    op: &'static str,
) -> Result<Value, EvalFault> {
    let (shape, data) = match register(registers, tensor)? {
        Value::Vector(values) => (vec![values.len()], values.as_slice()),
        Value::Matrix { rows, cols, data } => (vec![*rows, *cols], data.as_slice()),
        Value::Tensor { shape, data } => (shape.clone(), data.as_slice()),
        _ => {
            return Err(EvalFault::TypeConfusion {
                register: tensor.0,
                op,
            });
        }
    };
    if axes.len() != shape.len() {
        return Err(EvalFault::TypeConfusion {
            register: tensor.0,
            op,
        });
    }
    let mut runtime_axes = Vec::with_capacity(axes.len());
    for axis in axes {
        runtime_axes.push(match axis {
            EmirSliceAxis::Point(index) => {
                emath_rt::SliceAxis::Point(f64_of(registers, *index, op)?)
            }
            EmirSliceAxis::Range { start, end } => emath_rt::SliceAxis::Range {
                start: f64_of(registers, *start, op)?,
                end: f64_of(registers, *end, op)?,
            },
        });
    }
    match emath_rt::tensor_slice_checked(&shape, data, &runtime_axes) {
        Ok((kept, data)) => Ok(match kept.as_slice() {
            [] => Value::F64(data.first().copied().unwrap_or(f64::NAN)),
            [_] => Value::Vector(data),
            [rows, cols] => Value::Matrix {
                rows: *rows,
                cols: *cols,
                data,
            },
            _ => Value::Tensor { shape: kept, data },
        }),
        Err(emath_rt::IndexError::OutOfBounds { index, len }) => {
            Err(EvalFault::IndexOutOfBounds { op, index, len })
        }
        Err(emath_rt::IndexError::Arithmetic(detail)) => Err(EvalFault::Arithmetic { op, detail }),
    }
}
