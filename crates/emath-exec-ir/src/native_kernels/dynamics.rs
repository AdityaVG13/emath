//! Domain-neutral numerical kernels used by dynamics/control/PDE capability adapters.
//!
//! This module deliberately contains no method, model, control-system, or PDE
//! names. Capsules own those meanings. Adapters provide coefficients, stage
//! weights, boundary samples, tolerances, budgets, and cancellation policy.

use crate::interp::Value;
use crate::native_kernel::NativeKernel;

/// Pure Value-ABI kernels that have complete carrier and refusal contracts.
pub static KERNELS: &[NativeKernel] = &[
    NativeKernel {
        kernel_id: "checked-polynomial-ratio",
        signature: "(Vector<Float64>,Vector<Float64>,Float64)->Float64",
        arity: 3,
        handler: checked_polynomial_ratio,
    },
    NativeKernel {
        kernel_id: "checked-linear-projection",
        signature: "(Matrix<Float64>,Vector<Float64>,Vector<Float64>)->Float64",
        arity: 3,
        handler: checked_linear_projection,
    },
    NativeKernel {
        kernel_id: "checked-sign-table",
        signature: "(Vector<Float64>)->Bool",
        arity: 1,
        handler: checked_sign_table,
    },
    NativeKernel {
        kernel_id: "second-difference-clamp",
        signature: "(Vector<Float64>,PositiveLiteralFloat64)->SameVector<Float64>",
        arity: 2,
        handler: second_difference_clamp,
    },
    NativeKernel {
        kernel_id: "second-difference-neumann",
        signature: "(Vector<Float64>,PositiveLiteralFloat64)->SameVector<Float64>",
        arity: 2,
        handler: second_difference_neumann,
    },
    NativeKernel {
        kernel_id: "second-difference-dirichlet",
        signature: "(Vector<Float64>,PositiveLiteralFloat64,LiteralFloat64,LiteralFloat64)->SameVector<Float64>",
        arity: 4,
        handler: second_difference_dirichlet,
    },
    NativeKernel {
        kernel_id: "centered-first-difference",
        signature: "(Vector<Float64>,PositiveLiteralFloat64)->SameVector<Float64>",
        arity: 2,
        handler: centered_first_difference,
    },
    NativeKernel {
        kernel_id: "five-point-sum-clamp",
        signature: "(Matrix<Float64>,PositiveLiteralFloat64)->SameMatrix<Float64>",
        arity: 2,
        handler: five_point_sum_clamp,
    },
    NativeKernel {
        kernel_id: "five-point-sum-neumann",
        signature: "(Matrix<Float64>,PositiveLiteralFloat64)->SameMatrix<Float64>",
        arity: 2,
        handler: five_point_sum_neumann,
    },
    NativeKernel {
        kernel_id: "axis-0-first-difference",
        signature: "(Matrix<Float64>,PositiveLiteralFloat64)->SameMatrix<Float64>",
        arity: 2,
        handler: axis_0_first_difference,
    },
    NativeKernel {
        kernel_id: "axis-1-first-difference",
        signature: "(Matrix<Float64>,PositiveLiteralFloat64)->SameMatrix<Float64>",
        arity: 2,
        handler: axis_1_first_difference,
    },
    NativeKernel {
        kernel_id: "sum-axis-first-differences",
        signature: "(Matrix<Float64>,SameMatrix<Float64>,PositiveLiteralFloat64,PositiveLiteralFloat64?)->SameMatrix<Float64>",
        arity: 3,
        handler: sum_axis_first_differences,
    },
    NativeKernel {
        kernel_id: "checked-backward-euler-step",
        signature: "(Vector<Float64>,Float64,Float64)->Float64",
        arity: 3,
        handler: checked_backward_euler_step,
    },
    NativeKernel {
        kernel_id: "checked-velocity-verlet-step",
        signature: "(Vector<Float64>,Float64,Float64,Float64)->Vector<Float64>",
        arity: 4,
        handler: checked_velocity_verlet_step,
    },
    NativeKernel {
        kernel_id: "checked-poisson-dirichlet-sine",
        signature: "(Vector<Float64>)->SameVector<Float64>",
        arity: 1,
        handler: checked_poisson_dirichlet_sine,
    },
    NativeKernel {
        kernel_id: "checked-stencil-3d-clamp",
        signature: "(Tensor<Float64>,Vector<Float64>,I64,I64,I64)->SameTensor<Float64>",
        arity: 5,
        handler: checked_stencil_3d_clamp,
    },
    NativeKernel {
        kernel_id: "checked-stencil-3d-neumann",
        signature: "(Tensor<Float64>,Vector<Float64>,I64,I64,I64)->SameTensor<Float64>",
        arity: 5,
        handler: checked_stencil_3d_neumann,
    },
    NativeKernel {
        kernel_id: "checked-stencil-3d-one-sided",
        signature: "(Tensor<Float64>,Vector<Float64>,I64,I64,I64)->SameTensor<Float64>",
        arity: 5,
        handler: checked_stencil_3d_one_sided,
    },
];

fn checked_polynomial_ratio(args: &[Value]) -> Result<Value, String> {
    let [
        Value::Vector(numerator),
        Value::Vector(denominator),
        Value::F64(point),
    ] = args
    else {
        return Err(
            "E-TYPE-012: checked-polynomial-ratio expects two vectors and Float64".to_string(),
        );
    };
    emath_rt::checked_polynomial_ratio(numerator, denominator, *point)
        .map(Value::F64)
        .map_err(|error| error.to_string())
}

fn checked_linear_projection(args: &[Value]) -> Result<Value, String> {
    let [
        Value::Matrix { rows, cols, data },
        Value::Vector(input),
        Value::Vector(output),
    ] = args
    else {
        return Err(
            "E-TYPE-012: checked-linear-projection expects Matrix, Vector, Vector".to_string(),
        );
    };
    let expected = rows
        .checked_mul(*cols)
        .ok_or_else(|| "E-CONTROL-004: matrix extent overflow".to_string())?;
    if data.len() != expected || *cols == 0 {
        return Err("E-CONTROL-004: matrix data length does not match its shape".to_string());
    }
    let matrix = data
        .chunks_exact(*cols)
        .map(|row| row.to_vec())
        .collect::<Vec<_>>();
    emath_rt::checked_linear_projection(&matrix, input, output)
        .map(Value::F64)
        .map_err(|error| error.to_string())
}

fn checked_sign_table(args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(denominator)] = args else {
        return Err("E-TYPE-012: checked-sign-table expects Vector<Float64>".to_string());
    };
    emath_rt::checked_sign_table(denominator)
        .map(Value::Bool)
        .map_err(|error| error.to_string())
}

fn vector_spacing(args: &[Value]) -> Result<(&[f64], f64), String> {
    let [Value::Vector(field), Value::F64(spacing)] = args else {
        return Err("E-TYPE-012: stencil expects Vector<Float64>, Float64".to_string());
    };
    validate_spacing(*spacing)?;
    validate_finite(field)?;
    Ok((field, *spacing))
}

fn validate_spacing(spacing: f64) -> Result<(), String> {
    if spacing.is_finite() && spacing > 0.0 {
        Ok(())
    } else {
        Err("E-PDE-001: spacing must be a positive finite Float64".to_string())
    }
}

fn validate_finite(values: &[f64]) -> Result<(), String> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err("E-PDE-001: stencil carriers must be finite".to_string())
    }
}

fn second_difference_clamp(args: &[Value]) -> Result<Value, String> {
    second_difference(args, emath_rt::EdgePolicy::Clamp)
}

fn second_difference_neumann(args: &[Value]) -> Result<Value, String> {
    second_difference(args, emath_rt::EdgePolicy::Neumann)
}

fn second_difference(args: &[Value], edge: emath_rt::EdgePolicy) -> Result<Value, String> {
    let (field, spacing) = vector_spacing(args)?;
    let inverse = 1.0 / (spacing * spacing);
    Ok(Value::Vector(emath_rt::stencil_1d(
        field,
        &[inverse, -2.0 * inverse, inverse],
        1,
        edge,
    )))
}

fn second_difference_dirichlet(args: &[Value]) -> Result<Value, String> {
    let [
        Value::Vector(field),
        Value::F64(spacing),
        Value::F64(left),
        Value::F64(right),
    ] = args
    else {
        return Err(
            "E-TYPE-012: Dirichlet stencil expects Vector and three Float64 values".to_string(),
        );
    };
    validate_spacing(*spacing)?;
    validate_finite(field)?;
    validate_finite(&[*left, *right])?;
    let inverse = 1.0 / (*spacing * *spacing);
    Ok(Value::Vector(emath_rt::stencil_1d(
        field,
        &[inverse, -2.0 * inverse, inverse],
        1,
        emath_rt::EdgePolicy::Dirichlet {
            left: *left,
            right: *right,
        },
    )))
}

fn centered_first_difference(args: &[Value]) -> Result<Value, String> {
    let (field, spacing) = vector_spacing(args)?;
    let inverse = 1.0 / (2.0 * spacing);
    Ok(Value::Vector(emath_rt::stencil_1d(
        field,
        &[-inverse, 0.0, inverse],
        1,
        emath_rt::EdgePolicy::OneSided,
    )))
}

fn matrix_spacing(args: &[Value]) -> Result<(usize, usize, &[f64], f64), String> {
    let [Value::Matrix { rows, cols, data }, Value::F64(spacing)] = args else {
        return Err("E-TYPE-012: stencil expects Matrix<Float64>, Float64".to_string());
    };
    validate_spacing(*spacing)?;
    validate_finite(data)?;
    let expected = rows
        .checked_mul(*cols)
        .ok_or_else(|| "E-SHAPE-005: matrix extent overflow".to_string())?;
    if *rows == 0 || *cols == 0 || data.len() != expected {
        return Err(
            "E-SHAPE-005: matrix data length does not match its nonempty shape".to_string(),
        );
    }
    Ok((*rows, *cols, data, *spacing))
}

fn matrix_stencil(
    args: &[Value],
    weights: impl FnOnce(f64) -> [f64; 9],
    edge: emath_rt::EdgePolicy,
) -> Result<Value, String> {
    let (rows, cols, data, spacing) = matrix_spacing(args)?;
    let matrix = data
        .chunks_exact(cols)
        .map(|row| row.to_vec())
        .collect::<Vec<_>>();
    let output = emath_rt::stencil_2d(&matrix, &weights(spacing), (1, 1), edge);
    Ok(Value::Matrix {
        rows,
        cols,
        data: output.into_iter().flatten().collect(),
    })
}

fn five_point_weights(spacing: f64) -> [f64; 9] {
    let inverse = 1.0 / (spacing * spacing);
    [
        0.0,
        inverse,
        0.0,
        inverse,
        -4.0 * inverse,
        inverse,
        0.0,
        inverse,
        0.0,
    ]
}

fn first_difference_weights(axis: usize, spacing: f64) -> [f64; 9] {
    let inverse = 1.0 / (2.0 * spacing);
    if axis == 0 {
        [0.0, 0.0, 0.0, -inverse, 0.0, inverse, 0.0, 0.0, 0.0]
    } else {
        [0.0, -inverse, 0.0, 0.0, 0.0, 0.0, 0.0, inverse, 0.0]
    }
}

fn five_point_sum_clamp(args: &[Value]) -> Result<Value, String> {
    matrix_stencil(args, five_point_weights, emath_rt::EdgePolicy::Clamp)
}

fn five_point_sum_neumann(args: &[Value]) -> Result<Value, String> {
    matrix_stencil(args, five_point_weights, emath_rt::EdgePolicy::Neumann)
}

fn axis_0_first_difference(args: &[Value]) -> Result<Value, String> {
    matrix_stencil(
        args,
        |spacing| first_difference_weights(0, spacing),
        emath_rt::EdgePolicy::OneSided,
    )
}

fn axis_1_first_difference(args: &[Value]) -> Result<Value, String> {
    matrix_stencil(
        args,
        |spacing| first_difference_weights(1, spacing),
        emath_rt::EdgePolicy::OneSided,
    )
}

fn sum_axis_first_differences(args: &[Value]) -> Result<Value, String> {
    let (horizontal, vertical, dx, dy) = match args {
        [
            horizontal @ Value::Matrix { .. },
            vertical @ Value::Matrix { .. },
            Value::F64(spacing),
        ] => (horizontal, vertical, *spacing, *spacing),
        [
            horizontal @ Value::Matrix { .. },
            vertical @ Value::Matrix { .. },
            Value::F64(dx),
            Value::F64(dy),
        ] => (horizontal, vertical, *dx, *dy),
        _ => {
            return Err(
                "E-TYPE-012: sum-axis-first-differences expects two matrices and one or two Float64 spacings"
                    .to_string(),
            );
        }
    };
    let x = axis_0_first_difference(&[horizontal.clone(), Value::F64(dx)])?;
    let y = axis_1_first_difference(&[vertical.clone(), Value::F64(dy)])?;
    let (
        Value::Matrix {
            rows,
            cols,
            data: x,
        },
        Value::Matrix {
            rows: y_rows,
            cols: y_cols,
            data: y,
        },
    ) = (x, y)
    else {
        unreachable!("axis adapters return matrices")
    };
    if rows != y_rows || cols != y_cols || x.len() != y.len() {
        return Err("E-SHAPE-005: divergence field matrices must have equal shapes".to_string());
    }
    Ok(Value::Matrix {
        rows,
        cols,
        data: x.iter().zip(y).map(|(left, right)| left + right).collect(),
    })
}

fn checked_backward_euler_step(args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(rate), Value::F64(y0), Value::F64(step)] = args else {
        return Err(
            "E-TYPE-012: backward-euler expects Vector<Float64> and two Float64 values".to_string(),
        );
    };
    // Typed gating over the public body kernel (the private emath_rt
    // wrapper's exact classification): E-ODE-004 carriers, E-ODE-003
    // step, E-ODE-001 Newton non-convergence.
    if rate.iter().any(|c| !c.is_finite()) || !y0.is_finite() {
        return Err("E-ODE-004".to_string());
    }
    if !step.is_finite() || *step <= 0.0 {
        return Err("E-ODE-003".to_string());
    }
    let law: &[f64] = if rate.is_empty() { &[0.0] } else { rate };
    match emath_rt::ode_backward_euler_step(law, *y0, *step).first() {
        Some(&y1) => Ok(Value::F64(y1)),
        None => Err("E-ODE-001".to_string()),
    }
}

fn checked_velocity_verlet_step(args: &[Value]) -> Result<Value, String> {
    let [
        Value::Vector(acceleration),
        Value::F64(q0),
        Value::F64(v0),
        Value::F64(step),
    ] = args
    else {
        return Err(
            "E-TYPE-012: velocity-verlet expects Vector<Float64> and three Float64 values"
                .to_string(),
        );
    };
    // Verlet law: h may be negative (time reversal) but never zero or
    // non-finite; non-finite carriers refuse E-ODE-004.
    if acceleration.iter().any(|c| !c.is_finite()) || !q0.is_finite() || !v0.is_finite() {
        return Err("E-ODE-004".to_string());
    }
    if !step.is_finite() || *step == 0.0 {
        return Err("E-ODE-003".to_string());
    }
    let law: &[f64] = if acceleration.is_empty() {
        &[0.0]
    } else {
        acceleration
    };
    match emath_rt::ode_velocity_verlet_step(law, *q0, *v0, *step).as_slice() {
        [q1, v1] => Ok(Value::Vector(vec![*q1, *v1])),
        _ => Err("E-ODE-004".to_string()),
    }
}

fn checked_poisson_dirichlet_sine(args: &[Value]) -> Result<Value, String> {
    let [Value::Vector(load)] = args else {
        return Err("E-TYPE-012: poisson-dirichlet-sine expects Vector<Float64>".to_string());
    };
    // E-PDE-001 empty interior; E-PDE-002 non-finite load and any
    // non-finite field (fail closed, never a wrong field).
    if load.is_empty() {
        return Err("E-PDE-001".to_string());
    }
    if load.iter().any(|value| !value.is_finite()) {
        return Err("E-PDE-002".to_string());
    }
    let field = emath_rt::poisson_dirichlet_sine(load);
    if field.len() != load.len() || field.iter().any(|value| !value.is_finite()) {
        return Err("E-PDE-002".to_string());
    }
    Ok(Value::Vector(field))
}

/// The rank-3 carrier is honest Value ABI: the tensor field, the 27
/// stencil weights, and the three center offsets are all Values; the
/// boundary class is kernel identity (Dirichlet is not representable in
/// the 3D kernel and refuses there — an honest no-claim, never a shim).
fn checked_stencil_3d(args: &[Value], edge: emath_rt::EdgePolicy) -> Result<Value, String> {
    let [
        Value::Tensor { shape, data },
        Value::Vector(weights),
        Value::I64(center_x),
        Value::I64(center_y),
        Value::I64(center_z),
    ] = args
    else {
        return Err(
            "E-TYPE-012: 3d stencil expects Tensor<Float64>, 27 weights, and three Int centers"
                .to_string(),
        );
    };
    if weights.len() != 27 {
        return Err("E-SHAPE-005: 3d stencil weights must have exactly 27 entries".to_string());
    }
    validate_finite(weights)?;
    validate_finite(data)?;
    let mut kernel_weights = [0.0_f64; 27];
    kernel_weights.copy_from_slice(weights);
    let center = (*center_x, *center_y, *center_z);
    match emath_rt::stencil_3d_slices_checked(shape, data, &kernel_weights, center, edge) {
        Ok(tensor) => Ok(Value::Tensor {
            shape: tensor.shape,
            data: tensor.data,
        }),
        Err(detail) => Err(format!("E-SHAPE-005: {detail}")),
    }
}

fn checked_stencil_3d_clamp(args: &[Value]) -> Result<Value, String> {
    checked_stencil_3d(args, emath_rt::EdgePolicy::Clamp)
}

fn checked_stencil_3d_neumann(args: &[Value]) -> Result<Value, String> {
    checked_stencil_3d(args, emath_rt::EdgePolicy::Neumann)
}

fn checked_stencil_3d_one_sided(args: &[Value]) -> Result<Value, String> {
    checked_stencil_3d(args, emath_rt::EdgePolicy::OneSided)
}

/// Stable refusal classes preserved across native and reference adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelRefusal {
    Cancelled,
    BudgetExhausted,
    InvalidInput,
    ShapeMismatch,
    NonFinite,
    NonConverged,
    Unbracketed,
}

/// Per-invocation work budget and cancellation source.
///
/// `checkpoint` is called before every iterative unit. Cancellation wins over
/// budget exhaustion, matching the runner contract that cancelled work never
/// reports an unrelated convergence failure.
pub struct KernelControl<'a> {
    remaining: usize,
    cancelled: &'a dyn Fn() -> bool,
}

impl<'a> KernelControl<'a> {
    #[must_use]
    pub fn new(iterations: usize, cancelled: &'a dyn Fn() -> bool) -> Self {
        Self {
            remaining: iterations,
            cancelled,
        }
    }

    #[must_use]
    pub fn remaining(&self) -> usize {
        self.remaining
    }

    pub fn checkpoint(&mut self) -> Result<(), KernelRefusal> {
        if (self.cancelled)() {
            return Err(KernelRefusal::Cancelled);
        }
        if self.remaining == 0 {
            return Err(KernelRefusal::BudgetExhausted);
        }
        self.remaining -= 1;
        Ok(())
    }
}

fn finite(values: &[f64]) -> bool {
    values.iter().all(|value| value.is_finite())
}

/// Compute `state + step * sum(weight[i] * increment[i])`.
///
/// Stage order and weights are capsule data. The kernel checks carriers and
/// performs one deterministic left-to-right strict-f64 accumulation.
pub fn scaled_state_combination(
    state: &[f64],
    increments: &[&[f64]],
    weights: &[f64],
    step: f64,
    control: &mut KernelControl<'_>,
) -> Result<Vec<f64>, KernelRefusal> {
    control.checkpoint()?;
    if increments.len() != weights.len()
        || increments
            .iter()
            .any(|increment| increment.len() != state.len())
    {
        return Err(KernelRefusal::ShapeMismatch);
    }
    if !step.is_finite()
        || !finite(state)
        || !finite(weights)
        || increments.iter().any(|values| !finite(values))
    {
        return Err(KernelRefusal::NonFinite);
    }

    let mut output = Vec::with_capacity(state.len());
    for index in 0..state.len() {
        let mut delta = 0.0;
        for (weight, increment) in weights.iter().zip(increments) {
            delta += *weight * increment[index];
        }
        let value = state[index] + step * delta;
        if !value.is_finite() {
            return Err(KernelRefusal::NonFinite);
        }
        output.push(value);
    }
    Ok(output)
}

/// Bounded scalar nonlinear iteration with an adapter-supplied residual and
/// derivative. The last iterate is never returned after exhaustion.
pub fn bounded_newton(
    initial: f64,
    tolerance: f64,
    control: &mut KernelControl<'_>,
    mut residual_and_derivative: impl FnMut(f64) -> (f64, f64),
) -> Result<f64, KernelRefusal> {
    if !initial.is_finite() || !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(KernelRefusal::InvalidInput);
    }
    let mut value = initial;
    loop {
        control.checkpoint().map_err(|refusal| match refusal {
            KernelRefusal::BudgetExhausted => KernelRefusal::NonConverged,
            other => other,
        })?;
        let (residual, derivative) = residual_and_derivative(value);
        if !residual.is_finite() || !derivative.is_finite() {
            return Err(KernelRefusal::NonFinite);
        }
        if residual.abs() <= tolerance {
            return Ok(value);
        }
        if derivative == 0.0 {
            return Err(KernelRefusal::NonConverged);
        }
        value -= residual / derivative;
        if !value.is_finite() {
            return Err(KernelRefusal::NonFinite);
        }
    }
}

/// Result of one embedded-error decision.
#[derive(Clone, Debug, PartialEq)]
pub enum AdaptiveDecision {
    Accepted {
        state: Vec<f64>,
        next_step: f64,
        error_ratio: f64,
    },
    Rejected {
        next_step: f64,
        error_ratio: f64,
    },
}

/// Compare lower/higher embedded estimates and choose a bounded next step.
/// A rejected estimate never leaks its state as an accepted value.
pub fn adaptive_error_control(
    current: &[f64],
    lower: &[f64],
    higher: &[f64],
    step: f64,
    absolute_tolerance: f64,
    relative_tolerance: f64,
    maximum_step: Option<f64>,
    control: &mut KernelControl<'_>,
) -> Result<AdaptiveDecision, KernelRefusal> {
    control.checkpoint()?;
    if current.len() != lower.len() || lower.len() != higher.len() {
        return Err(KernelRefusal::ShapeMismatch);
    }
    if !finite(current)
        || !finite(lower)
        || !finite(higher)
        || !step.is_finite()
        || step <= 0.0
        || !absolute_tolerance.is_finite()
        || absolute_tolerance < 0.0
        || !relative_tolerance.is_finite()
        || relative_tolerance < 0.0
        || (absolute_tolerance == 0.0 && relative_tolerance == 0.0)
        || maximum_step.is_some_and(|limit| !limit.is_finite() || limit <= 0.0)
    {
        return Err(KernelRefusal::InvalidInput);
    }

    let mut ratio = 0.0_f64;
    for ((old, low), high) in current.iter().zip(lower).zip(higher) {
        let scale = absolute_tolerance + relative_tolerance * old.abs().max(high.abs());
        if scale <= 0.0 || !scale.is_finite() {
            return Err(KernelRefusal::InvalidInput);
        }
        ratio = ratio.max((high - low).abs() / scale);
    }
    if !ratio.is_finite() {
        return Err(KernelRefusal::NonFinite);
    }

    let factor = if ratio == 0.0 {
        5.0
    } else {
        (0.9 * ratio.powf(-0.2)).clamp(0.2, 5.0)
    };
    let mut next_step = step * factor;
    if let Some(limit) = maximum_step {
        next_step = next_step.min(limit);
    }
    if !next_step.is_finite() || next_step <= 0.0 {
        return Err(KernelRefusal::NonFinite);
    }
    if ratio <= 1.0 {
        Ok(AdaptiveDecision::Accepted {
            state: higher.to_vec(),
            next_step: next_step
                .max(step)
                .min(maximum_step.unwrap_or(f64::INFINITY)),
            error_ratio: ratio,
        })
    } else {
        let shrunken = next_step.min(step * 0.999_999_999_999);
        if shrunken <= 0.0 || shrunken >= step {
            return Err(KernelRefusal::NonConverged);
        }
        Ok(AdaptiveDecision::Rejected {
            next_step: shrunken,
            error_ratio: ratio,
        })
    }
}

/// Apply one one-dimensional weighted stencil at every input position.
///
/// The adapter owns axis and boundary meaning. It supplies out-of-range ghost
/// samples explicitly; this kernel only applies offsets and weights.
pub fn weighted_stencil(
    input: &[f64],
    offsets: &[isize],
    weights: &[f64],
    mut ghost: impl FnMut(isize, usize) -> Option<f64>,
    control: &mut KernelControl<'_>,
) -> Result<Vec<f64>, KernelRefusal> {
    control.checkpoint()?;
    if input.is_empty() || offsets.is_empty() || offsets.len() != weights.len() {
        return Err(KernelRefusal::ShapeMismatch);
    }
    if !finite(input) || !finite(weights) {
        return Err(KernelRefusal::NonFinite);
    }
    let mut output = Vec::with_capacity(input.len());
    for center in 0..input.len() {
        let mut value = 0.0;
        for (offset, weight) in offsets.iter().zip(weights) {
            let index = center as isize + *offset;
            let sample = if let Ok(index) = usize::try_from(index) {
                input.get(index).copied()
            } else {
                None
            }
            .or_else(|| ghost(index, input.len()))
            .ok_or(KernelRefusal::ShapeMismatch)?;
            if !sample.is_finite() {
                return Err(KernelRefusal::NonFinite);
            }
            value += *weight * sample;
        }
        if !value.is_finite() {
            return Err(KernelRefusal::NonFinite);
        }
        output.push(value);
    }
    Ok(output)
}

/// Locate a sign-bracketed crossing with deterministic bisection. Endpoint
/// hits are accepted; an unbracketed interval refuses rather than fabricating
/// an event. The supplied budget is the hard iteration ceiling.
pub fn bracketed_bisection(
    left: f64,
    right: f64,
    tolerance: f64,
    control: &mut KernelControl<'_>,
    mut gap: impl FnMut(f64) -> f64,
) -> Result<f64, KernelRefusal> {
    if !left.is_finite()
        || !right.is_finite()
        || right < left
        || !tolerance.is_finite()
        || tolerance <= 0.0
    {
        return Err(KernelRefusal::InvalidInput);
    }
    let mut lo = left;
    let mut hi = right;
    let mut g_lo = gap(lo);
    let g_hi = gap(hi);
    if !g_lo.is_finite() || !g_hi.is_finite() {
        return Err(KernelRefusal::NonFinite);
    }
    if g_lo == 0.0 {
        return Ok(lo);
    }
    if g_hi == 0.0 {
        return Ok(hi);
    }
    if g_lo.is_sign_positive() == g_hi.is_sign_positive() {
        return Err(KernelRefusal::Unbracketed);
    }

    while hi - lo > tolerance {
        control.checkpoint()?;
        let mid = lo + (hi - lo) * 0.5;
        let g_mid = gap(mid);
        if !g_mid.is_finite() {
            return Err(KernelRefusal::NonFinite);
        }
        if g_mid == 0.0 {
            return Ok(mid);
        }
        if g_mid.is_sign_positive() == g_lo.is_sign_positive() {
            lo = mid;
            g_lo = g_mid;
        } else {
            hi = mid;
        }
    }
    Ok(lo + (hi - lo) * 0.5)
}
