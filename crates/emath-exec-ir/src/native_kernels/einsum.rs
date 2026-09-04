//! Domain-neutral variadic contraction kernel for linear capability
//! capsules.
//!
//! This module deliberately does not register itself. `native_kernel.rs`
//! can integrate [`EINSUM_KERNELS`] into its immutable table without
//! matching on a mathematical feature name. The descriptor key and
//! signature are the entire ABI; aliases and FeatureIDs remain language
//! data.
//!
//! Carrier contract: the kernel takes ONE `Text` subscript and ONE
//! universal `Value::List` sequence whose elements may be any mix of
//! `Vector`, `Matrix`, and `Tensor` values (rank-polymorphic, variadic
//! operand count). All contraction math is delegated to the existing
//! validated `emath_rt::einsum_checked` evaluator — nothing is
//! duplicated here.

use crate::interp::Value;
use crate::native_kernel::NativeKernel;

/// Capsule-backed kernels in stable descriptor order.
pub static EINSUM_KERNELS: &[NativeKernel] = &[NativeKernel {
    kernel_id: "einsum-contract",
    signature: "(Text,Sequence)->Tensor",
    arity: 2,
    handler: einsum_contract,
}];

/// Flatten one carrier element into `(shape, row-major data)` exactly as
/// the validated evaluator expects. Rank-polymorphic: Vector is rank-1,
/// Matrix is rank-2, Tensor is rank-N.
fn einsum_operand_of(value: &Value) -> Result<(Vec<usize>, Vec<f64>), String> {
    match value {
        Value::Vector(data) => Ok((vec![data.len()], data.clone())),
        Value::Matrix { rows, cols, data } => {
            if data.len() != rows.saturating_mul(*cols) {
                return Err("E-TYPE-012: matrix data length does not match its shape".to_string());
            }
            Ok((vec![*rows, *cols], data.clone()))
        }
        Value::Tensor { shape, data } => {
            let expected: usize = shape.iter().product();
            if data.len() != expected {
                return Err("E-TYPE-012: tensor data length does not match its shape".to_string());
            }
            Ok((shape.clone(), data.clone()))
        }
        _ => Err(
            "E-TYPE-012: einsum-contract operands must be Vector, Matrix, or Tensor values"
                .to_string(),
        ),
    }
}

/// Stable refusal codes surfaced through the capability seam:
/// `E-EINSUM-001` (subscript/precondition refusal from the validated
/// evaluator) and `E-EINSUM-002` (contracted/output index outside an
/// operand axis). Carrier shape refusals reuse the existing
/// `E-TYPE-012` typed-refusal code.
fn einsum_contract(args: &[Value]) -> Result<Value, String> {
    let [subscripts, operands] = args else {
        return Err("E-TYPE-012: einsum-contract expects (Text, Sequence)".to_string());
    };
    let Value::Text(subscript) = subscripts else {
        return Err("E-TYPE-012: einsum-contract subscripts must be Text".to_string());
    };
    let operand_values: Vec<&Value> = match operands {
        Value::List(values) => values.iter().collect(),
        Value::Record { type_name, fields } if type_name == "Sequence" => fields.values().collect(),
        _ => {
            return Err("E-TYPE-012: einsum-contract operands must be a Sequence".to_string());
        }
    };
    let mut prepared = Vec::with_capacity(operand_values.len());
    for operand in operand_values {
        prepared.push(einsum_operand_of(operand)?);
    }
    let (shape, data) =
        emath_rt::einsum_checked(subscript, &prepared).map_err(|error| match error {
            emath_rt::EinsumError::Arithmetic(detail) => format!("E-EINSUM-001: {detail}"),
            emath_rt::EinsumError::IndexOutOfBounds { index, len } => {
                format!("E-EINSUM-002: einsum index {index} is outside 0..{len}")
            }
        })?;
    match shape.as_slice() {
        [] => Ok(Value::F64(data.first().copied().unwrap_or(0.0))),
        [len] => Ok(Value::Vector(data)),
        [rows, cols] => Ok(Value::Matrix {
            rows: *rows,
            cols: *cols,
            data,
        }),
        _ => Ok(Value::Tensor { shape, data }),
    }
}
