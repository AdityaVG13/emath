//! Domain-neutral program-carrier kernels for authored calculus goals.
//!
//! This module deliberately contains no model or method-domain names; capsules
//! own those meanings. The kernels execute `Value::Program(EmirProgram)`
//! bodies over an explicit `Float64` environment and mirror the executable
//! laws of `interp::ops::eval_flow` at the pure Value ABI: composite Simpson
//! integration (positive even steps, loop-variable binding, `Float64`-only
//! body results, `acc * h / 3`) and scalar Newton root solving (variable
//! index, tolerance, iteration budget, converged-residual acceptance, and the
//! deterministic bracket-scan fallback used when Newton is unreliable).

use crate::EmirProgram;
use crate::interp::{Value, evaluate};
use crate::native_kernel::NativeKernel;

/// Symmetric-difference epsilon for the Newton derivative.
const NEWTON_FD_EPSILON: f64 = 1e-8;

/// Numerical derivatives at analytically flat seeds carry roundoff on the
/// order of the finite-difference increment squared.
const NEWTON_VANISHED_DERIVATIVE: f64 = 1e-12;

const BRACKET_SCAN_LEVELS: usize = 48;
const BRACKET_SCAN_GROWTH: f64 = 8.0;
const BISECTION_ITERATIONS: usize = 120;

/// Descriptors to chain into `native_kernel::NATIVE_KERNELS`.
pub const KERNELS: &[NativeKernel] = &[
    NativeKernel {
        kernel_id: "checked-simpson-integral",
        signature: "(Program,Vector<Float64>,Float64,Float64,I64,I64)->Float64",
        arity: 6,
        handler: checked_simpson_integral,
    },
    NativeKernel {
        kernel_id: "checked-newton-scalar-solve",
        signature: "(Program,Vector<Float64>,I64,Float64,I64)->Float64",
        arity: 5,
        handler: checked_newton_scalar_solve,
    },
];

/// Composite Simpson integral of an authored `Float64` body.
///
/// `steps` must be positive and even and within the reference op's `u16`
/// range; `loop_var_index` names the environment slot that receives each
/// node; the body must return `Float64`. Node weights are `1, 4, 2, ..., 4, 1`
/// and the result is the weighted sum times `h / 3`.
fn checked_simpson_integral(args: &[Value]) -> Result<Value, String> {
    let [
        Value::Program(body),
        Value::Vector(environment),
        Value::F64(start),
        Value::F64(end),
        Value::I64(steps),
        Value::I64(loop_var_index),
    ] = args
    else {
        return Err(
            "E-TYPE-012: checked-simpson-integral expects Program, Vector<Float64>, two Float64 bounds, and two Int values"
                .to_string(),
        );
    };
    if *steps <= 0 {
        return Err("integral steps must be positive and even".to_string());
    }
    let nodes = usize::from(u16::try_from(*steps).map_err(|_| {
        "E-TYPE-012: integral steps exceed the composite Simpson op range (u16)".to_string()
    })?);
    if nodes % 2 != 0 {
        return Err("integral steps must be positive and even".to_string());
    }
    let binding = usize::try_from(*loop_var_index)
        .map_err(|_| "E-TYPE-012: integral loop variable index must be non-negative".to_string())?;
    if binding >= environment.len() {
        return Err("E-TYPE-012: integral loop variable index outside the environment".to_string());
    }
    let h = (end - start) / nodes as f64;
    let mut accumulator = 0.0_f64;
    for node in 0..=nodes {
        let x = start + h * node as f64;
        let weight = if node == 0 || node == nodes {
            1.0
        } else if node % 2 == 1 {
            4.0
        } else {
            2.0
        };
        let sample = evaluate(body, &bound_environment(environment, binding, x), &[])
            .map_err(|fault| format!("{fault:?}"))?;
        let Value::F64(sample) = sample else {
            return Err("E-TYPE-012: integral body must evaluate to Float64".to_string());
        };
        accumulator += weight * sample;
    }
    Ok(Value::F64(accumulator * h / 3.0))
}

/// Scalar Newton root solve of an authored `Float64` body.
///
/// The seed is `environment[var_index]`; the loop runs at most `max_iter`
/// iterations and accepts the iterate only when `|residual| < tolerance`.
/// When Newton cannot take a reliable step, a fixed geometric scan and
/// bisection are attempted before preserving the corresponding typed refusal.
fn checked_newton_scalar_solve(args: &[Value]) -> Result<Value, String> {
    let [
        Value::Program(body),
        Value::Vector(environment),
        Value::I64(var_index),
        Value::F64(tolerance),
        Value::I64(max_iter),
    ] = args
    else {
        return Err(
            "E-TYPE-012: checked-newton-scalar-solve expects Program, Vector<Float64>, Int variable index, Float64 tolerance, and Int iteration budget"
                .to_string(),
        );
    };
    let iterations = u32::try_from(*max_iter).map_err(|_| {
        "E-TYPE-012: solve max_iter must be non-negative and within the solve op range (u32)"
            .to_string()
    })?;
    let binding = usize::try_from(*var_index)
        .map_err(|_| "E-TYPE-012: solve variable index must be non-negative".to_string())?;
    if binding >= environment.len() {
        return Err("E-TYPE-012: solve variable index outside the environment".to_string());
    }
    let mut x = environment[binding];
    let seed = x;
    for _ in 0..iterations {
        let residual = scalar_at(body, environment, binding, x)?;
        if residual.abs() < *tolerance {
            return Ok(Value::F64(x));
        }
        let slope = (scalar_at(body, environment, binding, x + NEWTON_FD_EPSILON)?
            - scalar_at(body, environment, binding, x - NEWTON_FD_EPSILON)?)
            / (2.0 * NEWTON_FD_EPSILON);
        if slope.abs() < NEWTON_VANISHED_DERIVATIVE {
            return solve_bracket_fallback(body, environment, binding, seed, *tolerance)?
                .map(Value::F64)
                .ok_or_else(|| "solve derivative vanished before convergence".to_string());
        }
        let step = x - residual / slope;
        if !residual.is_finite() || !slope.is_finite() || !step.is_finite() {
            return solve_bracket_fallback(body, environment, binding, seed, *tolerance)?
                .map(Value::F64)
                .ok_or_else(|| "solve produced a nonfinite value before convergence".to_string());
        }
        x = step;
    }
    let residual = scalar_at(body, environment, binding, x)?;
    if residual.abs() < *tolerance {
        return Ok(Value::F64(x));
    }
    Err("solve did not converge within max_iter".to_string())
}

fn solve_bracket_fallback(
    body: &EmirProgram,
    environment: &[f64],
    binding: usize,
    seed: f64,
    tolerance: f64,
) -> Result<Option<f64>, String> {
    let seed_residual = scalar_at(body, environment, binding, seed)?;
    if seed_residual.abs() < tolerance {
        return Ok(Some(seed));
    }

    let mut radius = 1.0_f64;
    for _ in 0..BRACKET_SCAN_LEVELS {
        for candidate in [seed + radius, seed - radius] {
            let candidate_residual = scalar_at(body, environment, binding, candidate)?;
            if candidate_residual.abs() < tolerance {
                return Ok(Some(candidate));
            }
            if seed_residual.is_finite()
                && candidate_residual.is_finite()
                && seed_residual.is_sign_positive() != candidate_residual.is_sign_positive()
            {
                let (left, right, left_residual) = if candidate < seed {
                    (candidate, seed, candidate_residual)
                } else {
                    (seed, candidate, seed_residual)
                };
                return bisect_bracket(
                    body,
                    environment,
                    binding,
                    left,
                    right,
                    left_residual,
                    tolerance,
                );
            }
        }
        radius *= BRACKET_SCAN_GROWTH;
    }
    Ok(None)
}

fn bisect_bracket(
    body: &EmirProgram,
    environment: &[f64],
    binding: usize,
    mut left: f64,
    mut right: f64,
    mut left_residual: f64,
    tolerance: f64,
) -> Result<Option<f64>, String> {
    for _ in 0..BISECTION_ITERATIONS {
        let midpoint = left + (right - left) * 0.5;
        let residual = scalar_at(body, environment, binding, midpoint)?;
        if residual.abs() < tolerance {
            return Ok(Some(midpoint));
        }
        if !residual.is_finite() {
            return Ok(None);
        }
        if left_residual.is_sign_positive() == residual.is_sign_positive() {
            left = midpoint;
            left_residual = residual;
        } else {
            right = midpoint;
        }
    }
    Ok(None)
}

/// Bind `x` into the environment slot the body reads, leaving every other
/// slot at its supplied value. The supplied vector is the body's complete
/// `Float64` environment.
fn bound_environment(environment: &[f64], binding: usize, x: f64) -> Vec<Value> {
    let mut inputs = environment
        .iter()
        .copied()
        .map(Value::F64)
        .collect::<Vec<_>>();
    if let Some(slot) = inputs.get_mut(binding) {
        *slot = Value::F64(x);
    }
    inputs
}

/// Evaluate the authored body at `x` and require a `Float64` result.
fn scalar_at(
    body: &EmirProgram,
    environment: &[f64],
    binding: usize,
    x: f64,
) -> Result<f64, String> {
    let value = evaluate(body, &bound_environment(environment, binding, x), &[])
        .map_err(|fault| format!("{fault:?}"))?;
    match value {
        Value::F64(residual) => Ok(residual),
        _ => Err("E-TYPE-012: solve body must evaluate to Float64".to_string()),
    }
}
