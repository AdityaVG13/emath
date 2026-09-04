//! Domain-neutral scalar-optimization and sampled-limit kernels over the
//! `Value::Program` carrier ABI (the value produced by
//! `EmirOp::ProgramLiteral`). Mathematical names and authority remain in
//! capsules; the carried laws are the ones documented in
//! `interp/ops/eval_flow.rs` — Newton on ∇f = 0 for stationarity, and
//! geometric-approach sampling for limits — lifted to the program ABI
//! without a domain-named `EmirOp`.
//!
//! Gradient note: the law's dual-number gradient lives in the private
//! interpreter helpers, unreachable from a kernel handler. The kernel
//! evaluates the same stationarity law with a central finite-difference
//! gradient (exact on polynomials up to f64 rounding);
//! the Hessian uses a central difference of that gradient, and
//! every refusal keeps the law's detail string verbatim.

use crate::EmirProgram;
use crate::interp::Value;
use crate::native_kernel::NativeKernel;

/// Separate steps avoid catastrophic cancellation in the nested differences:
/// the gradient needs a smaller sampling interval than its derivative.
const GRADIENT_EPS: f64 = 1e-6;
const HESSIAN_EPS: f64 = 1e-4;

/// Descriptors to append to the immutable native-kernel registry.
///
/// Determinism is inherited from the laws: fixed iteration order, the
/// geometric 1e-1..1e-12 sampling schedule, first-failure refusal, and
/// bit-identical results for identical inputs.
pub const KERNELS: &[NativeKernel] = &[
    NativeKernel {
        kernel_id: "program-optimize",
        signature: "(Program,Vector<Float64>,Vector<Float64>,Bool,Float64,Float64,I64)->Float64",
        arity: 7,
        handler: program_optimize,
    },
    NativeKernel {
        kernel_id: "program-sample-limit",
        signature: "(Program,Vector<Float64>,I64,Float64,Float64)->Float64",
        arity: 5,
        handler: program_sample_limit,
    },
];

/// Newton on ∇f = 0 over an embedded program. Arguments: the program,
/// the complete Float64 input vector, the variable-index vector (exact
/// non-negative integer entries), the maximize flag, a learning rate
/// (admitted by the ABI but unused — Newton takes no fixed step, exactly
/// like the law's ignored field), the stationarity tolerance, and the
/// iteration budget.
fn program_optimize(args: &[Value]) -> Result<Value, String> {
    let [
        program,
        inputs,
        indices,
        maximize,
        learning_rate,
        tolerance,
        max_iter,
    ] = args
    else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let _ = learning_rate;
    let body = program_body(program)?;
    let inputs = vector(inputs)?;
    let indices = integral_vector(
        indices,
        "optimize variable indices must be non-negative exact integers",
    )?;
    if indices.is_empty() {
        return Err("optimize requires at least one variable".to_string());
    }
    let maximize = match maximize {
        Value::Bool(flag) => *flag,
        _ => return Err("E-TYPE-012: kernel argument must be Bool".to_string()),
    };
    let tolerance = scalar(tolerance)?;
    let max_iter = match max_iter {
        Value::I64(budget) => (*budget).clamp(0, u32::MAX as i64) as u32,
        _ => return Err("E-TYPE-012: kernel argument must be I64".to_string()),
    };
    let mut x: Vec<f64> = Vec::with_capacity(indices.len());
    for &vi in &indices {
        match inputs.get(vi) {
            Some(value) => x.push(*value),
            None => {
                return Err(format!(
                    "optimize variable index {vi} is not a supplied input"
                ));
            }
        }
    }
    for _ in 0..max_iter {
        let grads = gradients(body, inputs, &indices, &x)?;
        let max_grad = grads.iter().fold(0.0_f64, |acc, g| acc.max(g.abs()));
        if max_grad < tolerance {
            return Ok(Value::F64(x[0]));
        }
        let n = x.len();
        let mut hess = vec![vec![0.0_f64; n]; n];
        for j in 0..n {
            x[j] += HESSIAN_EPS;
            let plus = gradients(body, inputs, &indices, &x)?;
            x[j] -= 2.0 * HESSIAN_EPS;
            let minus = gradients(body, inputs, &indices, &x)?;
            x[j] += HESSIAN_EPS;
            for i in 0..n {
                hess[i][j] = (plus[i] - minus[i]) / (2.0 * HESSIAN_EPS);
            }
        }
        let delta = dense_solve(&hess, &grads)
            .map_err(|_| "optimize hessian vanished before stationarity".to_string())?;
        let dot: f64 = grads.iter().zip(delta.iter()).map(|(g, d)| g * d).sum();
        // Newton on ∇f = 0 finds any stationary point. Refuse a min
        // returned as a max (or vice versa): g·(H⁻¹g) is positive iff H
        // is positive definite along g.
        if maximize {
            if dot >= 0.0 {
                return Err("optimize hessian has the wrong curvature for maximize".to_string());
            }
        } else if dot <= 0.0 {
            return Err("optimize hessian has the wrong curvature for minimize".to_string());
        }
        for (xi, d) in x.iter_mut().zip(delta.iter()) {
            *xi -= d;
        }
    }
    let grads = gradients(body, inputs, &indices, &x)?;
    let max_grad = grads.iter().fold(0.0_f64, |acc, g| acc.max(g.abs()));
    if max_grad < tolerance {
        return Ok(Value::F64(x[0]));
    }
    Err("optimize did not converge within max_iter".to_string())
}

/// Numerical limit approximation over an embedded program: sample the
/// body at points approaching the target along a geometric sequence of
/// step sizes (0.1, 0.01, ..., 1e-12). Return the first finite value
/// whose predecessor was also finite and within 1% of it (convergence
/// check); if no pair converges, return the last finite sample.
fn program_sample_limit(args: &[Value]) -> Result<Value, String> {
    let [program, inputs, var_index, target, direction] = args else {
        return Err("capability argument count does not match the cell contract".to_string());
    };
    let body = program_body(program)?;
    let input_values = vector(inputs)?;
    let var_index = match var_index {
        Value::I64(slot) if *slot >= 0 => *slot as usize,
        _ => {
            return Err(
                "E-TYPE-012: sample_limit variable index must be a non-negative I64".to_string(),
            );
        }
    };
    let target_val = scalar(target)?;
    let dir_val = scalar(direction)?;
    let mut work_inputs: Vec<Value> = input_values.iter().map(|v| Value::F64(*v)).collect();
    while work_inputs.len() <= var_index {
        work_inputs.push(Value::F64(0.0));
    }
    let directions: &[f64] = match dir_val as i64 {
        0 => &[1.0, -1.0], // two-sided
        1 => &[1.0],       // from above
        -1 => &[-1.0],     // from below
        _ => &[1.0, -1.0], // fallback: two-sided
    };
    let mut best = f64::NAN;
    let mut prev = f64::NAN;
    for step_exp in 1..=12_u32 {
        let h = 10_f64.powi(-(step_exp as i32));
        for &d in directions {
            let x = target_val + d * h;
            work_inputs[var_index] = Value::F64(x);
            match crate::interp::evaluate(body, &work_inputs, &[]) {
                Ok(value) => {
                    if let Some(fx) = value.as_real_f64() {
                        if fx.is_finite() {
                            if prev.is_finite() && (fx - prev).abs() <= fx.abs() * 0.01 + 1e-14 {
                                // Converged: successive samples agree to 1%.
                                return Ok(Value::F64(fx));
                            }
                            prev = fx;
                            best = fx;
                        }
                    }
                }
                _ => {} // non-finite or wrong type: skip
            }
        }
    }
    if best.is_finite() {
        Ok(Value::F64(best))
    } else {
        Err("sample_limit produced no finite values".to_string())
    }
}

/// Central-difference gradient of the body at `x` over the variable
/// indices (the law's dual gradient, evaluated numerically at the
/// kernel ABI).
fn gradients(
    body: &EmirProgram,
    inputs: &[f64],
    indices: &[usize],
    x: &[f64],
) -> Result<Vec<f64>, String> {
    let mut work: Vec<Value> = inputs.iter().map(|v| Value::F64(*v)).collect();
    for (&vi, &xi) in indices.iter().zip(x.iter()) {
        work[vi] = Value::F64(xi);
    }
    let mut grads = Vec::with_capacity(x.len());
    for (k, &vi) in indices.iter().enumerate() {
        let mut plus = work.clone();
        plus[vi] = Value::F64(x[k] + GRADIENT_EPS);
        let mut minus = work.clone();
        minus[vi] = Value::F64(x[k] - GRADIENT_EPS);
        let f_plus = evaluate_scalar(body, &plus)?;
        let f_minus = evaluate_scalar(body, &minus)?;
        grads.push((f_plus - f_minus) / (2.0 * GRADIENT_EPS));
    }
    Ok(grads)
}

fn evaluate_scalar(body: &EmirProgram, inputs: &[Value]) -> Result<f64, String> {
    match crate::interp::evaluate(body, inputs, &[]) {
        Ok(value) => match value.as_real_f64() {
            Some(fx) => Ok(fx),
            None => Err("optimize body must evaluate to a real scalar".to_string()),
        },
        Err(fault) => Err(fault.to_string()),
    }
}

/// Dense linear solve `H δ = g` by Gaussian elimination with partial
/// pivoting; `Err` when the matrix is numerically singular (the law's
/// vanished-Hessian path).
fn dense_solve(matrix: &[Vec<f64>], rhs: &[f64]) -> Result<Vec<f64>, ()> {
    let n = rhs.len();
    let mut a = matrix.to_vec();
    let mut b = rhs.to_vec();
    for col in 0..n {
        let pivot = (col..n).fold(col, |best, row| {
            if a[row][col].abs() > a[best][col].abs() {
                row
            } else {
                best
            }
        });
        if a[pivot][col].abs() < 1e-30 {
            return Err(());
        }
        a.swap(pivot, col);
        b.swap(pivot, col);
        for row in (col + 1)..n {
            let factor = a[row][col] / a[col][col];
            for k in col..n {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut delta = vec![0.0_f64; n];
    for row in (0..n).rev() {
        let tail: f64 = ((row + 1)..n).map(|k| a[row][k] * delta[k]).sum();
        delta[row] = (b[row] - tail) / a[row][row];
    }
    Ok(delta)
}

fn program_body(value: &Value) -> Result<&EmirProgram, String> {
    match value {
        Value::Program(program) => Ok(program),
        _ => Err("E-TYPE-012: kernel argument must be Program".to_string()),
    }
}

fn vector(value: &Value) -> Result<&[f64], String> {
    match value {
        Value::Vector(entries) => Ok(entries),
        _ => Err("E-TYPE-012: kernel argument must be Vector<Float64>".to_string()),
    }
}

fn scalar(value: &Value) -> Result<f64, String> {
    match value {
        Value::F64(v) => Ok(*v),
        _ => Err("E-TYPE-012: kernel argument must be Float64".to_string()),
    }
}

/// Variable-index vectors carried as `Vector<Float64>`: every entry must
/// be a finite, exact, non-negative integer (the carrier for the law's
/// u32 op fields).
fn integral_vector(value: &Value, refusal: &str) -> Result<Vec<usize>, String> {
    let Value::Vector(entries) = value else {
        return Err("E-TYPE-012: kernel argument must be Vector<Float64>".to_string());
    };
    entries
        .iter()
        .map(|entry| integral_index(*entry, refusal))
        .collect()
}

fn integral_index(entry: f64, refusal: &str) -> Result<usize, String> {
    if entry.is_finite() && entry >= 0.0 && entry.fract() == 0.0 {
        Ok(entry as usize)
    } else {
        Err(refusal.to_string())
    }
}
