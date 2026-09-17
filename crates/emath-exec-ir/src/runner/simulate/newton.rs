//! Causalized implicit-DAE Newton solving entry points.
//!
//! Native simulate/ODE is not a constructor operation: every entry
//! refuses with the shared `CONSTRUCTOR_SIMULATE_GONE` text. The
//! residual-Newton iteration is authored language machinery, not Rust.

use super::types::StepMethod;
use crate::EmirProgram;
use crate::interp::Value;
use emath_ir::{Declaration, ModelResidual, SemanticPackage};
use std::collections::BTreeMap;

/// Residual-model Euler/RK4 step for Main API routing.
pub fn authored_implicit_explicit_step(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    dt: f64,
    method: StepMethod,
) -> Result<BTreeMap<String, Value>, String> {
    let _ = (package, declaration, inputs, state, dt, method);
    super::gone()
}

/// Residual-model step program, shared by the interpreter and the
/// generated Rust steps. Input frame: declaration inputs then `dt`.
pub fn residual_model_step_program(
    package: &SemanticPackage,
    declaration: &Declaration,
    residuals: &[ModelResidual],
    rk4: bool,
) -> Result<EmirProgram, String> {
    let _ = (package, declaration, residuals, rk4);
    super::gone()
}
