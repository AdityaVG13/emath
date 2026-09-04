//! Typed ODE stepping: the error model and
//! safe wrappers over the strict-f64 kernels in `crate::body`.
//!
//! Refusals are typed, never silent: a non-converged implicit solve
//! (`E-ODE-001`), a non-advancing/non-finite step size (`E-ODE-003`),
//! or non-finite carriers (`E-ODE-004`) each name their diagnostic.
//! The runner/`StepMethod` surface owns its `StepMethod` definitions; this
//! module is the EMIR-op compute path only.

use crate::body::{ode_backward_euler_step, ode_velocity_verlet_step};

/// Typed refusal for the ODE stepping ops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OdeError {
    /// Newton did not converge to machine tolerance: the implicit
    /// point is refused, never approximated silently.
    NotConverged,
    /// Backward Euler requires a positive finite step; velocity
    /// Verlet requires a non-zero finite step (reversibility needs
    /// the sign, but zero cannot advance).
    InvalidStep,
    /// Non-finite rate/acceleration coefficients or state.
    NonFinite,
}

impl OdeError {
    /// Stable diagnostic code (the seed-visible shape).
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::NotConverged => "E-ODE-001",
            Self::InvalidStep => "E-ODE-003",
            Self::NonFinite => "E-ODE-004",
        }
    }
}

/// One backward-Euler step for a scalar ODE `y' = f(y)` with an
/// ascending polynomial rate law (empty law = the zero rate, the
/// established polynomial convention). Classifies typed refusals
/// before the kernel; an empty kernel result is Newton
/// non-convergence (`E-ODE-001`).
pub fn ode_backward_euler(rate: &[f64], y0: f64, h: f64) -> Result<f64, OdeError> {
    if rate.iter().any(|coefficient| !coefficient.is_finite()) || !y0.is_finite() {
        return Err(OdeError::NonFinite);
    }
    if !h.is_finite() || h <= 0.0 {
        return Err(OdeError::InvalidStep);
    }
    // Empty rate law = y' = 0: the step is the identity.
    let law: &[f64] = if rate.is_empty() { &[0.0] } else { rate };
    match ode_backward_euler_step(law, y0, h).first() {
        Some(&y1) => Ok(y1),
        None => Err(OdeError::NotConverged),
    }
}

/// One velocity-Verlet step for the separable system `q' = v`,
/// `v' = a(q)` (ascending polynomial acceleration of position).
/// Returns `[q1, v1]`. `h` may be negative (time reversal); zero and
/// non-finite step sizes refuse.
pub fn ode_velocity_verlet(
    acceleration: &[f64],
    q0: f64,
    v0: f64,
    h: f64,
) -> Result<(f64, f64), OdeError> {
    if acceleration
        .iter()
        .any(|coefficient| !coefficient.is_finite())
        || !q0.is_finite()
        || !v0.is_finite()
    {
        return Err(OdeError::NonFinite);
    }
    if !h.is_finite() || h == 0.0 {
        return Err(OdeError::InvalidStep);
    }
    // Empty acceleration law = free particle (a ≡ 0).
    let law: &[f64] = if acceleration.is_empty() {
        &[0.0]
    } else {
        acceleration
    };
    match ode_velocity_verlet_step(law, q0, v0, h).as_slice() {
        [q1, v1] => Ok((*q1, *v1)),
        _ => Err(OdeError::NonFinite),
    }
}
