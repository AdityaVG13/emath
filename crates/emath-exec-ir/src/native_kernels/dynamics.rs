//! Generic invocation budget and cancellation machinery for dynamics/control/PDE
//! capability adapters, plus the (currently empty) descriptor registry.
//!
//! All capability-bound numerical algorithms (finite differences, stencils and
//! edge policies, polynomial ratios, projections, sign tables) execute from
//! authored reference bodies in
//! `language/spec/capabilities/numerics/dynamics-control-pde.emath`.
//! This module deliberately contains no method, model, control-system, or PDE
//! names. Native mathematical implementations live here no longer.

use crate::native_kernel::NativeKernel;

/// Descriptors to append to the immutable native-kernel registry.
///
/// Empty: every dynamics/control/PDE capability resolves to its authored
/// reference program through the installed Language Image.
pub static KERNELS: &[NativeKernel] = &[];

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
