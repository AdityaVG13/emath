//! Domain-neutral adapters for the finite-category certification kernels.
//! Mathematical names and authority remain in capsules; the carriers are the
//! dense composition-table laws documented in `emath-rt::category`.

use crate::interp::Value;
use crate::native_kernel::NativeKernel;

/// Descriptors to append to the immutable native-kernel registry.
///
/// Determinism is inherited from the underlying kernels: fixed-order law
/// passes, first-failure refusal, index-fold path evaluation, and
/// bit-identical results for identical inputs.
pub const KERNELS: &[NativeKernel] = &[
    NativeKernel {
        kernel_id: "finite-category-certification",
        signature: "(Vector<Float64>,Vector<Float64>,Matrix<Float64>)->Bool",
        arity: 3,
        handler: category_validity,
    },
    NativeKernel {
        kernel_id: "diagram-commutativity-mask",
        signature: "(Vector<Float64>,Vector<Float64>,Matrix<Float64>,Vector<Float64>)->Vector<Float64>",
        arity: 4,
        handler: diagram_commutativity,
    },
];

fn category_error(error: emath_rt::CategoryError) -> String {
    error.code().to_string()
}

fn category_validity(args: &[Value]) -> Result<Value, String> {
    let [dom, cod, comp] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let dom = vector(dom)?;
    let cod = vector(cod)?;
    let comp = composition(comp)?;
    emath_rt::category_check(dom, cod, &comp)
        .map(Value::Bool)
        .map_err(category_error)
}

fn diagram_commutativity(args: &[Value]) -> Result<Value, String> {
    let [dom, cod, comp, faces] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let dom = vector(dom)?;
    let cod = vector(cod)?;
    let comp = composition(comp)?;
    let faces = vector(faces)?;
    let mask = emath_rt::diagram_commutative(dom, cod, &comp, faces).map_err(category_error)?;
    Ok(Value::Vector(
        mask.iter()
            .map(|face| if *face { 1.0 } else { 0.0 })
            .collect(),
    ))
}

/// The dense `rows × cols` composition table as the row-major nested
/// carrier the kernel expects.
fn composition(value: &Value) -> Result<Vec<Vec<f64>>, String> {
    let Value::Matrix { rows, cols, data } = value else {
        return Err("E-TYPE-012: kernel argument must be Matrix<Float64>".to_string());
    };
    Ok((0..*rows)
        .map(|row| data[row * cols..(row + 1) * cols].to_vec())
        .collect())
}

fn vector(value: &Value) -> Result<&[f64], String> {
    match value {
        Value::Vector(entries) => Ok(entries),
        _ => Err("E-TYPE-012: kernel argument must be Vector<Float64>".to_string()),
    }
}
