//! Rank-polymorphic dense-carrier indexing primitive for linear capability capsules.
//!
//! Mathematical methods (norms, products, decompositions, solvers,
//! polynomial arithmetic, folds) execute from authored capsule reference
//! bodies; this module retains only the genuine indexing primitive that the
//! reference vocabulary cannot express. It deliberately does not register
//! itself. `native_kernel.rs` can integrate [`LINEAR_KERNELS`] into its
//! immutable table without matching on a mathematical feature name. The
//! descriptor key and signature are the entire ABI; aliases and FeatureIDs
//! remain language data.

use crate::interp::Value;
use crate::native_kernel::NativeKernel;

/// Capsule-backed kernels in stable descriptor order.
pub static LINEAR_KERNELS: &[NativeKernel] = &[NativeKernel {
    kernel_id: "checked-dense-index",
    signature: "(Dense<Float64>,Sequence)->Float64",
    arity: 2,
    handler: checked_dense_index,
}];

/// Rank-polymorphic point index: the coordinate sequence length must
/// equal the carrier rank, and the result is the scalar at that offset.
pub fn checked_dense_index(args: &[Value]) -> Result<Value, String> {
    let [carrier, indices] = args else {
        return Err("E-TYPE-012: checked-dense-index expects Dense<Float64>, Sequence".to_string());
    };
    let (shape, data) = dense_shape_data(carrier)?;
    let coords = sequence_values(indices)?;
    if coords.len() != shape.len() {
        return Err(format!(
            "E-SHAPE-006: index requires {} subscript(s), found {}",
            shape.len(),
            coords.len()
        ));
    }
    let mut offset = 0_usize;
    for (axis, (extent, coord)) in shape.iter().zip(coords.iter()).enumerate() {
        let index = whole_index(coord, *extent, axis)?;
        offset = offset
            .checked_mul(*extent)
            .and_then(|value| value.checked_add(index))
            .ok_or_else(|| "E-SHAPE-006: dense index offset overflow".to_string())?;
    }
    data.get(offset)
        .copied()
        .map(Value::F64)
        .ok_or_else(|| {
            format!(
                "E-SHAPE-006: index {offset} is outside the dense carrier of length {}",
                data.len()
            )
        })
}

fn dense_shape_data(value: &Value) -> Result<(Vec<usize>, &[f64]), String> {
    match value {
        Value::Vector(values) => Ok((vec![values.len()], values.as_slice())),
        Value::Matrix { rows, cols, data } => {
            let expected = rows
                .checked_mul(*cols)
                .ok_or_else(|| "E-LINALG-004: dense carrier extent overflow".to_string())?;
            if data.len() != expected {
                return Err(
                    "E-LINALG-004: dense carrier data length does not match its shape".to_string(),
                );
            }
            Ok((vec![*rows, *cols], data.as_slice()))
        }
        Value::Tensor { shape, data } => {
            let expected = shape
                .iter()
                .try_fold(1_usize, |product, extent| product.checked_mul(*extent))
                .ok_or_else(|| "E-LINALG-004: dense carrier extent overflow".to_string())?;
            if data.len() != expected {
                return Err(
                    "E-LINALG-004: dense carrier data length does not match its shape".to_string(),
                );
            }
            Ok((shape.clone(), data.as_slice()))
        }
        _ => Err("E-TYPE-012: checked-dense-index expects Dense<Float64>".to_string()),
    }
}

fn sequence_values(value: &Value) -> Result<Vec<&Value>, String> {
    match value {
        Value::Set(values) | Value::List(values) => Ok(values.iter().collect()),
        Value::Vector(values) => Err(format!(
            "E-TYPE-012: checked-dense-index coordinates must be a Sequence, found Vector of length {}",
            values.len()
        )),
        Value::Record { type_name, fields } if type_name == "Sequence" => {
            Ok(fields.values().collect())
        }
        _ => Err("E-TYPE-012: checked-dense-index coordinates must be a Sequence".to_string()),
    }
}

fn whole_index(value: &Value, extent: usize, axis: usize) -> Result<usize, String> {
    let index = match value {
        Value::I64(value) => *value,
        Value::F64(value)
            if value.is_finite() && value.fract() == 0.0 && *value >= 0.0 && *value <= i64::MAX as f64 =>
        {
            *value as i64
        }
        _ => {
            return Err(format!(
                "E-SHAPE-006: index axis {axis} must be a whole non-negative integer"
            ));
        }
    };
    if index < 0 || index as usize >= extent {
        return Err(format!(
            "E-SHAPE-006: index {index} is outside axis {axis} of length {extent}"
        ));
    }
    Ok(index as usize)
}
