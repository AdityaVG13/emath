//! Domain-neutral numeric kernels used by domain-science capability capsules.
//!
//! This module does not name geometry, units, chemistry, species, or unit
//! catalogs. Capsules own those readings and bind them to these operations by
//! `(kernel_id, signature)`. Every handler validates carrier shape and finiteness
//! before producing a value; malformed inputs never truncate through `zip`.

use crate::interp::Value;
use crate::native_kernel::NativeKernel;

/// Descriptors to chain into `native_kernel::NATIVE_KERNELS`.
///
/// Integration intentionally consumes this slice as data; it must not add a
/// feature-name match arm.
pub(crate) static DOMAIN_SCIENCE_KERNELS: &[NativeKernel] = &[
    NativeKernel {
        kernel_id: "pairwise-sum-products",
        signature: "(Vector,Vector)->Float64",
        arity: 2,
        handler: pairwise_sum_products,
    },
    NativeKernel {
        kernel_id: "alternating-product-3",
        signature: "(Vector,Vector)->Vector",
        arity: 2,
        handler: alternating_product_3,
    },
    NativeKernel {
        kernel_id: "bilinear-product-4",
        signature: "(Vector,Vector)->Vector",
        arity: 2,
        handler: bilinear_product_4,
    },
    NativeKernel {
        kernel_id: "componentwise-integer-add",
        signature: "(Vector,Vector)->Vector",
        arity: 2,
        handler: componentwise_integer_add,
    },
    NativeKernel {
        kernel_id: "affine-map",
        signature: "(Float64,Float64,Float64)->Float64",
        arity: 3,
        handler: affine_map,
    },
    NativeKernel {
        kernel_id: "rectangular-linear-residual",
        signature: "(Matrix,Vector)->Vector",
        arity: 2,
        handler: rectangular_linear_residual,
    },
    NativeKernel {
        kernel_id: "componentwise-integer-negate",
        signature: "(Vector)->Vector",
        arity: 1,
        handler: componentwise_integer_negate,
    },
    NativeKernel {
        kernel_id: "componentwise-integer-scale",
        signature: "(Vector,Int)->Vector",
        arity: 2,
        handler: componentwise_integer_scale,
    },
    NativeKernel {
        kernel_id: "integer-vector-witness",
        signature: "(Vector,Vector)->Vector",
        arity: 2,
        handler: integer_vector_witness,
    },
    NativeKernel {
        kernel_id: "integer-row-rank",
        signature: "(Matrix)->Int",
        arity: 1,
        handler: integer_row_rank,
    },
    NativeKernel {
        kernel_id: "integer-nullspace-basis",
        signature: "(Matrix)->Matrix",
        arity: 1,
        handler: integer_nullspace_basis,
    },
    NativeKernel {
        kernel_id: "decimal-significance-round",
        signature: "(Float64,Int)->Float64",
        arity: 2,
        handler: decimal_significance_round,
    },
    NativeKernel {
        kernel_id: "decimal-significance-count",
        signature: "(Text)->Int",
        arity: 1,
        handler: decimal_significance_count,
    },
];

/// Extract exactly integral exponents from an f64 carrier vector. The
/// carrier is f64 by ABI; the group law is integral, so a fractional entry
/// refuses with the capsule-declared diagnostic instead of truncating.
fn integer_exponents(exponents: &[f64], operation: &str) -> Result<Vec<i64>, String> {
    exponents
        .iter()
        .map(|exponent| {
            if exponent.is_finite()
                && exponent.fract() == 0.0
                && *exponent >= i64::MIN as f64
                && *exponent <= i64::MAX as f64
            {
                Ok(*exponent as i64)
            } else {
                Err(format!(
                    "E-UNIT-001: {operation} requires exactly integral exponents"
                ))
            }
        })
        .collect()
}

fn integer_matrix_rows(
    rows: usize,
    cols: usize,
    data: &[f64],
    operation: &str,
) -> Result<Vec<Vec<i64>>, String> {
    let expected_len = rows.checked_mul(cols).ok_or_else(|| {
        format!("E-SHAPE-001: {operation} matrix shape overflows usize")
    })?;
    if data.len() != expected_len {
        return Err(format!(
            "E-SHAPE-001: {operation} matrix storage has length {}, expected {rows}x{cols}={expected_len}",
            data.len()
        ));
    }
    data.chunks_exact(cols.max(1))
        .take(rows)
        .map(|row| integer_exponents(row, operation))
        .collect()
}

fn gcd_i64(a: i64, b: i64) -> i64 {
    let (a, b) = (a.unsigned_abs(), b.unsigned_abs());
    let (mut a, mut b) = (a, b);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a as i64
}

/// Witness minimization: divide out the exponent gcd and fix the sign so
/// the first nonzero entry is positive.
fn primitive_vector(vector: &mut Vec<i64>) {
    let divisor = vector.iter().fold(0_i64, |acc, e| gcd_i64(acc, *e));
    if divisor > 1 {
        vector.iter_mut().for_each(|e| *e /= divisor);
    }
    if let Some(first) = vector.iter().copied().find(|e| *e != 0) {
        if first < 0 {
            vector.iter_mut().for_each(|e| *e = -*e);
        }
    }
}

fn componentwise_integer_negate(args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(exponents)] = args else {
        return Err("E-TYPE-012: componentwise-integer-negate expects one Vector argument"
            .to_string());
    };
    let exponents = integer_exponents(exponents, "componentwise-integer-negate")?;
    let result = exponents
        .iter()
        .map(|exponent| {
            exponent
                .checked_neg()
                .map(|negated| negated as f64)
                .ok_or_else(|| {
                    "E-UNIT-001: componentwise-integer-negate exponent overflow".to_string()
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Vector(result))
}

fn componentwise_integer_scale(args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(exponents), Value::I64(factor)] = args else {
        return Err(
            "E-TYPE-012: componentwise-integer-scale expects Vector, Int arguments".to_string(),
        );
    };
    let exponents = integer_exponents(exponents, "componentwise-integer-scale")?;
    let result = exponents
        .iter()
        .map(|exponent| {
            exponent
                .checked_mul(*factor)
                .map(|scaled| scaled as f64)
                .ok_or_else(|| {
                    "E-UNIT-001: componentwise-integer-scale exponent overflow".to_string()
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Vector(result))
}

fn integer_vector_witness(args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(left), Value::Vector(right)] = args else {
        return Err(
            "E-TYPE-012: integer-vector-witness expects two Vector arguments".to_string()
        );
    };
    let left = integer_exponents(left, "integer-vector-witness")?;
    let right = integer_exponents(right, "integer-vector-witness")?;
    if left != right {
        return Err(format!(
            "E-UNIT-001: integer-vector-witness operands differ ({left:?} vs {right:?})"
        ));
    }
    Ok(Value::Vector(left.iter().map(|e| *e as f64).collect()))
}

/// Fraction-free row rank of an integer matrix (rows = vectors), computed
/// by cross-multiplication with gcd reduction. Pure integer arithmetic,
/// deterministic pivot order.
fn integer_rank(mut rows: Vec<Vec<i64>>) -> usize {
    rows.iter_mut().for_each(primitive_vector);
    let mut basis: Vec<Vec<i64>> = Vec::new();
    'rows: for row in rows {
        let mut w = row;
        for b in &basis {
            let pivot = b
                .iter()
                .position(|e| *e != 0)
                .expect("echelon row has a pivot");
            if w[pivot] != 0 {
                let bp = b[pivot];
                let wp = w[pivot];
                w = b
                    .iter()
                    .zip(&w)
                    .map(|(b, w)| wp * b - bp * w)
                    .collect();
                primitive_vector(&mut w);
            }
        }
        if w.iter().all(|e| *e == 0) {
            continue 'rows;
        }
        let pivot = w
            .iter()
            .position(|e| *e != 0)
            .expect("non-zero row has a pivot");
        let pos = basis
            .iter()
            .position(|b| b.iter().position(|e| *e != 0).expect("pivot") > pivot)
            .unwrap_or(basis.len());
        basis.insert(pos, w);
    }
    basis.len()
}

fn integer_row_rank(args: &[Value]) -> Result<Value, String> {
    let [Value::Matrix { rows, cols, data }] = args else {
        return Err("E-TYPE-012: integer-row-rank expects one Matrix argument".to_string());
    };
    let rows = integer_matrix_rows(*rows, *cols, data, "integer-row-rank")?;
    Ok(Value::I64(integer_rank(rows) as i64))
}

/// Integer null-space basis of a matrix (rows = vectors): coefficient
/// vectors `c` with `Σ_i c_i · row_i = 0`, one per `n − rank`, each
/// witness-minimized and sign-canonical. Incremental kernel intersection
/// over the column equations; pure integer arithmetic.
fn integer_nullspace_basis(args: &[Value]) -> Result<Value, String> {
    let [Value::Matrix { rows, cols, data }] = args else {
        return Err(
            "E-TYPE-012: integer-nullspace-basis expects one Matrix argument".to_string()
        );
    };
    let variables = integer_matrix_rows(*rows, *cols, data, "integer-nullspace-basis")?;
    let n = variables.len();
    let mut kernel: Vec<Vec<i64>> = (0..n)
        .map(|i| {
            let mut e = vec![0_i64; n];
            e[i] = 1;
            e
        })
        .collect();
    for d in 0..*cols {
        let scores: Vec<i64> = kernel
            .iter()
            .map(|k| {
                k.iter()
                    .zip(&variables)
                    .fold(0_i64, |acc, (c, var)| acc + c * var[d])
            })
            .collect();
        let Some(p) = scores.iter().position(|s| *s != 0) else {
            continue;
        };
        let sp = scores[p];
        let mut next: Vec<Vec<i64>> = Vec::new();
        for (j, k) in kernel.iter().enumerate() {
            if j == p {
                continue;
            }
            if scores[j] == 0 {
                next.push(k.clone());
            } else {
                let mut combo: Vec<i64> = k
                    .iter()
                    .zip(&kernel[p])
                    .map(|(kj, kp)| scores[j] * kp - sp * kj)
                    .collect();
                primitive_vector(&mut combo);
                next.push(combo);
            }
        }
        kernel = next;
    }
    for coefficients in &mut kernel {
        if let Some(first) = coefficients.iter().copied().find(|e| *e != 0) {
            if first < 0 {
                coefficients.iter_mut().for_each(|e| *e = -*e);
            }
        }
    }
    let data = kernel
        .iter()
        .flatten()
        .map(|coefficient| *coefficient as f64)
        .collect();
    Ok(Value::Matrix {
        rows: kernel.len(),
        cols: n,
        data,
    })
}

fn decimal_significance_round(args: &[Value]) -> Result<Value, String> {
    let [Value::F64(value), Value::I64(significant)] = args else {
        return Err(
            "E-TYPE-012: decimal-significance-round expects Float64, Int arguments".to_string(),
        );
    };
    if *significant < 0 {
        return Err(format!(
            "E-PRECISION-001: decimal-significance-round requires a non-negative count (found {significant})"
        ));
    }
    Ok(Value::F64(emath_core::round_to_sig_figs(
        *value,
        *significant as u32,
    )))
}

fn decimal_significance_count(args: &[Value]) -> Result<Value, String> {
    let [Value::Text(literal)] = args else {
        return Err(
            "E-TYPE-012: decimal-significance-count expects one Text argument".to_string()
        );
    };
    emath_core::count_sig_figs(literal)
        .map(|count| Value::I64(i64::from(count)))
        .ok_or_else(|| {
            format!(
                "E-PRECISION-001: `{literal}` carries no precision information (not a numeric literal with a nonzero digit)"
            )
        })
}

fn vectors<'a>(args: &'a [Value], operation: &str) -> Result<(&'a [f64], &'a [f64]), String> {
    let [Value::Vector(left), Value::Vector(right)] = args else {
        return Err(format!(
            "E-TYPE-012: {operation} expects two Vector arguments"
        ));
    };
    if left.len() != right.len() {
        return Err(format!(
            "E-SHAPE-001: {operation} requires equal vector lengths (left={}, right={})",
            left.len(),
            right.len()
        ));
    }
    require_finite(left.iter().chain(right), operation)?;
    Ok((left, right))
}

fn require_finite<'a>(
    values: impl IntoIterator<Item = &'a f64>,
    operation: &str,
) -> Result<(), String> {
    if values.into_iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(format!(
            "E-CELL-006: {operation} requires finite Float64 operands"
        ))
    }
}

fn finite_result(value: f64, operation: &str) -> Result<f64, String> {
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| format!("E-CELL-006: {operation} produced a non-finite Float64 result"))
}

fn pairwise_sum_products(args: &[Value]) -> Result<Value, String> {
    let (left, right) = vectors(args, "pairwise-sum-products")?;
    let sum = left.iter().zip(right).try_fold(0.0, |sum, (left, right)| {
        finite_result(sum + left * right, "pairwise-sum-products")
    })?;
    Ok(Value::F64(sum))
}

fn alternating_product_3(args: &[Value]) -> Result<Value, String> {
    let (left, right) = vectors(args, "alternating-product-3")?;
    if left.len() != 3 {
        return Err(format!(
            "E-SHAPE-001: alternating-product-3 requires length 3 (found {})",
            left.len()
        ));
    }
    let result = vec![
        finite_result(
            left[1] * right[2] - left[2] * right[1],
            "alternating-product-3",
        )?,
        finite_result(
            left[2] * right[0] - left[0] * right[2],
            "alternating-product-3",
        )?,
        finite_result(
            left[0] * right[1] - left[1] * right[0],
            "alternating-product-3",
        )?,
    ];
    Ok(Value::Vector(result))
}

fn bilinear_product_4(args: &[Value]) -> Result<Value, String> {
    let (left, right) = vectors(args, "bilinear-product-4")?;
    if left.len() != 4 {
        return Err(format!(
            "E-SHAPE-001: bilinear-product-4 requires length 4 (found {})",
            left.len()
        ));
    }
    let [a, b, c, d] = [left[0], left[1], left[2], left[3]];
    let [e, f, g, h] = [right[0], right[1], right[2], right[3]];
    let result = vec![
        finite_result(a * e - b * f - c * g - d * h, "bilinear-product-4")?,
        finite_result(a * f + b * e + c * h - d * g, "bilinear-product-4")?,
        finite_result(a * g - b * h + c * e + d * f, "bilinear-product-4")?,
        finite_result(a * h + b * g - c * f + d * e, "bilinear-product-4")?,
    ];
    Ok(Value::Vector(result))
}

fn componentwise_integer_add(args: &[Value]) -> Result<Value, String> {
    let (left, right) = vectors(args, "componentwise-integer-add")?;
    if left
        .iter()
        .chain(right)
        .any(|exponent| exponent.fract() != 0.0)
    {
        return Err(
            "E-UNIT-001: componentwise-integer-add requires exactly integral exponents".to_string(),
        );
    }
    let result = left
        .iter()
        .zip(right)
        .map(|(left, right)| finite_result(left + right, "componentwise-integer-add"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Vector(result))
}

fn affine_map(args: &[Value]) -> Result<Value, String> {
    let [Value::F64(value), Value::F64(scale), Value::F64(offset)] = args else {
        return Err(
            "E-TYPE-012: affine-map expects three Float64 arguments (value, scale, offset)"
                .to_string(),
        );
    };
    require_finite([value, scale, offset], "affine-map")?;
    Ok(Value::F64(finite_result(
        value * scale + offset,
        "affine-map",
    )?))
}

fn rectangular_linear_residual(args: &[Value]) -> Result<Value, String> {
    let [Value::Matrix { rows, cols, data }, Value::Vector(vector)] = args else {
        return Err("E-TYPE-012: rectangular-linear-residual expects Matrix, Vector".to_string());
    };
    let expected_len = rows.checked_mul(*cols).ok_or_else(|| {
        "E-SHAPE-001: rectangular-linear-residual matrix shape overflows usize".to_string()
    })?;
    if data.len() != expected_len {
        return Err(format!(
            "E-SHAPE-001: rectangular-linear-residual matrix storage has length {}, expected {}x{}={expected_len}",
            data.len(),
            rows,
            cols
        ));
    }
    if *cols != vector.len() {
        return Err(format!(
            "E-SHAPE-001: rectangular-linear-residual requires matrix.cols == vector.length ({} != {})",
            cols,
            vector.len()
        ));
    }
    require_finite(data.iter().chain(vector), "rectangular-linear-residual")?;
    if *cols == 0 {
        return Ok(Value::Vector(vec![0.0; *rows]));
    }
    let mut result = Vec::with_capacity(*rows);
    for row in data.chunks_exact(*cols) {
        let residual = row
            .iter()
            .zip(vector)
            .try_fold(0.0, |sum, (coefficient, value)| {
                finite_result(sum + coefficient * value, "rectangular-linear-residual")
            })?;
        result.push(residual);
    }
    Ok(Value::Vector(result))
}
