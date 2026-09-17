use super::*;

pub(super) fn eval_math(
    _self_program: &EmirProgram,
    op: &EmirOp,
    registers: &[Value],
    _inputs: ValueFrame<'_>,
    _state: ValueFrame<'_>,
    _budget: EvalBudget,
) -> Result<Value, EvalFault> {
    match op {
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
        _ => unreachable!("op routed to the wrong eval family"),
    }
}
