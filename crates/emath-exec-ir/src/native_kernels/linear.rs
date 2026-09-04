//! Domain-neutral dense-carrier kernels for linear capability capsules.
//!
//! This module deliberately does not register itself. `native_kernel.rs` can
//! integrate [`LINEAR_KERNELS`] into its immutable table without matching on a
//! mathematical feature name. The descriptor key and signature are the entire
//! ABI; aliases and FeatureIDs remain language data.

use crate::interp::Value;
use crate::native_kernel::NativeKernel;

/// Capsule-backed kernels in stable descriptor order.
pub static LINEAR_KERNELS: &[NativeKernel] = &[
    NativeKernel {
        kernel_id: "vector-l2",
        signature: "(Vector<Float64>)->Float64",
        arity: 1,
        handler: vector_l2,
    },
    NativeKernel {
        kernel_id: "symmetric-spectrum",
        signature: "(Matrix<Float64>)->Vector<Float64>",
        arity: 1,
        handler: symmetric_spectrum,
    },
    NativeKernel {
        kernel_id: "symmetric-basis",
        signature: "(Matrix<Float64>)->Matrix<Float64>",
        arity: 1,
        handler: symmetric_basis,
    },
    NativeKernel {
        kernel_id: "rectangular-spectrum",
        signature: "(Matrix<Float64>)->Vector<Float64>",
        arity: 1,
        handler: rectangular_spectrum,
    },
    NativeKernel {
        kernel_id: "rectangular-factors",
        signature: "(Matrix<Float64>)->Matrix<Float64>",
        arity: 1,
        handler: rectangular_factors,
    },
    NativeKernel {
        kernel_id: "convergent-system-solve",
        signature: "(Matrix<Float64>,Vector<Float64>)->Vector<Float64>",
        arity: 2,
        handler: convergent_system_solve,
    },
    NativeKernel {
        kernel_id: "dense-vector-add",
        signature: "(Vector<Float64>,Vector<Float64>)->Vector<Float64>",
        arity: 2,
        handler: dense_vector_add,
    },
    NativeKernel {
        kernel_id: "dense-matrix-add",
        signature: "(Matrix<Float64>,Matrix<Float64>)->Matrix<Float64>",
        arity: 2,
        handler: dense_matrix_add,
    },
    NativeKernel {
        kernel_id: "dense-matrix-product",
        signature: "(Matrix<Float64>,Matrix<Float64>)->Matrix<Float64>",
        arity: 2,
        handler: dense_matrix_product,
    },
    NativeKernel {
        kernel_id: "dense-matrix-vector-product",
        signature: "(Matrix<Float64>,Vector<Float64>)->Vector<Float64>",
        arity: 2,
        handler: dense_matrix_vector_product,
    },
    NativeKernel {
        kernel_id: "dense-transpose",
        signature: "(Matrix<Float64>)->Matrix<Float64>",
        arity: 1,
        handler: dense_transpose,
    },
    NativeKernel {
        kernel_id: "dense-tensor-add",
        signature: "(Tensor<Float64>,Tensor<Float64>)->Tensor<Float64>",
        arity: 2,
        handler: dense_tensor_add,
    },
    NativeKernel {
        kernel_id: "polynomial-multiply",
        signature: "(Vector<Float64>,Vector<Float64>)->Vector<Float64>",
        arity: 2,
        handler: polynomial_multiply,
    },
    NativeKernel {
        kernel_id: "polynomial-evaluate",
        signature: "(Vector<Float64>,Float64)->Float64",
        arity: 2,
        handler: polynomial_evaluate,
    },
];

/// Deterministic Euclidean norm over the dense vector carrier.
pub fn vector_l2(args: &[Value]) -> Result<Value, String> {
    match args {
        [Value::Vector(values)] => Ok(Value::F64(emath_rt::vec_norm(values))),
        _ => Err("E-TYPE-012: vector-l2 expects Vector<Float64>".to_string()),
    }
}

/// Ascending symmetric spectrum using the existing deterministic Jacobi kernel.
pub fn symmetric_spectrum(args: &[Value]) -> Result<Value, String> {
    let (rows, cols, data) = unary_matrix(args, "symmetric-spectrum")?;
    let (values, _) =
        emath_rt::symmetric_decomposition(data, rows, cols).map_err(|error| error.to_string())?;
    Ok(Value::Vector(values))
}

/// Eigenvector columns aligned with [`symmetric_spectrum`].
pub fn symmetric_basis(args: &[Value]) -> Result<Value, String> {
    let (rows, cols, data) = unary_matrix(args, "symmetric-basis")?;
    let (_, columns) =
        emath_rt::symmetric_decomposition(data, rows, cols).map_err(|error| error.to_string())?;
    let order = columns.len();
    let mut packed = vec![0.0; order * order];
    for (column, values) in columns.iter().enumerate() {
        for (row, value) in values.iter().enumerate() {
            packed[row * order + column] = *value;
        }
    }
    Ok(Value::Matrix {
        rows: order,
        cols: order,
        data: packed,
    })
}

/// Descending thin singular values of a rectangular dense carrier.
pub fn rectangular_spectrum(args: &[Value]) -> Result<Value, String> {
    let (rows, cols, data) = unary_matrix(args, "rectangular-spectrum")?;
    emath_rt::rectangular_spectrum(data, rows, cols)
        .map(Value::Vector)
        .map_err(|error| error.to_string())
}

/// Existing packed `[U; s; Vᵀ]` thin-factor carrier.
pub fn rectangular_factors(args: &[Value]) -> Result<Value, String> {
    let (rows, cols, data) = unary_matrix(args, "rectangular-factors")?;
    let packed =
        emath_rt::rectangular_factors(data, rows, cols).map_err(|error| error.to_string())?;
    let rank = rows.min(cols);
    Ok(Value::Matrix {
        rows: rows + 1 + rank,
        cols,
        data: packed,
    })
}

/// Conjugate-gradient solve with the existing shape and convergence refusals.
pub fn convergent_system_solve(args: &[Value]) -> Result<Value, String> {
    let [Value::Matrix { rows, cols, data }, Value::Vector(rhs)] = args else {
        return Err(
            "E-TYPE-012: convergent-system-solve expects Matrix<Float64>,Vector<Float64>"
                .to_string(),
        );
    };
    validate_layout(*rows, *cols, data)?;
    emath_rt::convergent_dense_solve(data, *rows, *cols, rhs)
        .map(Value::Vector)
        .map_err(|error| error.to_string())
}

/// Coefficientwise dense vector addition (the retired `VectorAdd`
/// carrier law): identical lengths, ascending index order, no finite
/// gate (the Float64 world keeps IEEE non-finite propagation).
pub fn dense_vector_add(args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(left), Value::Vector(right)] = args else {
        return Err(
            "E-TYPE-012: dense-vector-add expects Vector<Float64>,Vector<Float64>".to_string(),
        );
    };
    if left.len() != right.len() {
        return Err("E-SHAPE-001: dense carrier length mismatch".to_string());
    }
    Ok(Value::Vector(
        left.iter().zip(right.iter()).map(|(l, r)| l + r).collect(),
    ))
}

/// Shape-matched dense matrix addition (the retired `MatrixAdd` law):
/// identical extents, ascending row-major index order.
pub fn dense_matrix_add(args: &[Value]) -> Result<Value, String> {
    let [
        Value::Matrix { rows, cols, data },
        Value::Matrix {
            rows: r2,
            cols: c2,
            data: data2,
        },
    ] = args
    else {
        return Err(
            "E-TYPE-012: dense-matrix-add expects Matrix<Float64>,Matrix<Float64>".to_string(),
        );
    };
    validate_layout(*rows, *cols, data)?;
    validate_layout(*r2, *c2, data2)?;
    if rows != r2 || cols != c2 {
        return Err("E-SHAPE-001: dense carrier shape mismatch".to_string());
    }
    Ok(Value::Matrix {
        rows: *rows,
        cols: *cols,
        data: data.iter().zip(data2.iter()).map(|(l, r)| l + r).collect(),
    })
}

/// Row-major matrix product (the retired `MatrixMulMatrix` law):
/// `c[i][j] = Σ_k a[i][k]·b[k][j]`, inner dimensions must agree,
/// ascending `k` accumulation is the deterministic order.
pub fn dense_matrix_product(args: &[Value]) -> Result<Value, String> {
    let [
        Value::Matrix { rows, cols, data },
        Value::Matrix {
            rows: r2,
            cols: c2,
            data: data2,
        },
    ] = args
    else {
        return Err(
            "E-TYPE-012: dense-matrix-product expects Matrix<Float64>,Matrix<Float64>".to_string(),
        );
    };
    validate_layout(*rows, *cols, data)?;
    validate_layout(*r2, *c2, data2)?;
    if cols != r2 {
        return Err("E-SHAPE-001: matrix product inner dimensions mismatch".to_string());
    }
    let mut packed = vec![0.0; rows * c2];
    for (row, product_row) in packed.chunks_mut(*c2).enumerate() {
        for (inner, left) in data[row * *cols..(row + 1) * *cols].iter().enumerate() {
            for (column, target) in product_row.iter_mut().enumerate() {
                *target += left * data2[inner * c2 + column];
            }
        }
    }
    Ok(Value::Matrix {
        rows: *rows,
        cols: *c2,
        data: packed,
    })
}

/// Dense matrix×vector product (the retired `MatrixMulVector` law):
/// the matrix width must equal the vector length; ascending column
/// order accumulates each output entry.
pub fn dense_matrix_vector_product(args: &[Value]) -> Result<Value, String> {
    let [Value::Matrix { rows, cols, data }, Value::Vector(vector)] = args else {
        return Err(
            "E-TYPE-012: dense-matrix-vector-product expects Matrix<Float64>,Vector<Float64>"
                .to_string(),
        );
    };
    validate_layout(*rows, *cols, data)?;
    if *cols != vector.len() {
        return Err("E-SHAPE-001: matrix×vector width mismatch".to_string());
    }
    let mut product = vec![0.0; *rows];
    for (row, target) in product.iter_mut().enumerate() {
        let mut accumulator = 0.0;
        for (column, left) in data[row * *cols..(row + 1) * *cols].iter().enumerate() {
            accumulator += left * vector[column];
        }
        *target = accumulator;
    }
    Ok(Value::Vector(product))
}

/// Row-major transpose involution (the retired `MatrixTranspose`
/// law): extents swap, index order is the unique layout-preserving
/// permutation, including 0-width and 0-height carriers.
pub fn dense_transpose(args: &[Value]) -> Result<Value, String> {
    let (rows, cols, data) = unary_matrix(args, "dense-transpose")?;
    let mut packed = vec![0.0; data.len()];
    for (index, value) in data.iter().enumerate() {
        let (row, column) = (index / cols, index % cols);
        packed[column * rows + row] = *value;
    }
    Ok(Value::Matrix {
        rows: cols,
        cols: rows,
        data: packed,
    })
}

/// Shape-matched dense tensor addition (the retired `TensorAdd` law):
/// identical shapes, ascending row-major index order.
pub fn dense_tensor_add(args: &[Value]) -> Result<Value, String> {
    let [
        Value::Tensor { shape, data },
        Value::Tensor {
            shape: shape2,
            data: data2,
        },
    ] = args
    else {
        return Err(
            "E-TYPE-012: dense-tensor-add expects Tensor<Float64>,Tensor<Float64>".to_string(),
        );
    };
    if shape != shape2 {
        return Err("E-SHAPE-001: tensor shape mismatch".to_string());
    }
    Ok(Value::Tensor {
        shape: shape.clone(),
        data: data.iter().zip(data2.iter()).map(|(l, r)| l + r).collect(),
    })
}

/// Cauchy convolution of ascending coefficient vectors through the
/// shared `emath-rt` polynomial kernel (`E-POLY-001` on non-finite
/// coefficients; the empty carrier is the zero polynomial).
pub fn polynomial_multiply(args: &[Value]) -> Result<Value, String> {
    match args {
        [Value::Vector(a), Value::Vector(b)] => emath_rt::checked_poly_mul(a, b)
            .map(Value::Vector)
            .map_err(|error| error.to_string()),
        _ => Err(
            "E-TYPE-012: polynomial-multiply expects Vector<Float64>,Vector<Float64>".to_string(),
        ),
    }
}

/// One-pass Horner evaluation of an ascending coefficient vector
/// through the shared `emath-rt` kernel (`E-POLY-001` coefficients,
/// `E-POLY-002` point; the empty carrier evaluates to 0.0).
pub fn polynomial_evaluate(args: &[Value]) -> Result<Value, String> {
    match args {
        [Value::Vector(coefficients), Value::F64(point)] => {
            emath_rt::checked_poly_eval(coefficients, *point)
                .map(Value::F64)
                .map_err(|error| error.to_string())
        }
        _ => Err("E-TYPE-012: polynomial-evaluate expects Vector<Float64>,Float64".to_string()),
    }
}

fn unary_matrix<'a>(
    args: &'a [Value],
    kernel_id: &str,
) -> Result<(usize, usize, &'a [f64]), String> {
    let [Value::Matrix { rows, cols, data }] = args else {
        return Err(format!("E-TYPE-012: {kernel_id} expects Matrix<Float64>"));
    };
    validate_layout(*rows, *cols, data)?;
    Ok((*rows, *cols, data))
}

fn validate_layout(rows: usize, cols: usize, data: &[f64]) -> Result<(), String> {
    let expected = rows
        .checked_mul(cols)
        .ok_or_else(|| "E-LINALG-004: dense carrier extent overflow".to_string())?;
    if data.len() != expected {
        return Err("E-LINALG-004: dense carrier data length does not match its shape".to_string());
    }
    Ok(())
}
