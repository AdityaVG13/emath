//! Public simulation entry points.
//!
//! Native simulate/ODE is not a constructor operation: every entry
//! refuses with the shared `CONSTRUCTOR_SIMULATE_GONE` text. Steppers,
//! event location, and implicit-DAE solving live in authored language
//! definitions (`language/spec/capabilities/**`), not in this crate.

use super::{SemanticPackage, Declaration, BTreeMap, StepMethod, Value, Trajectory, SimulateOptions, DAEDisposition};

/// Advance one explicit step. Rates come from admitted `der_<name>` definitions.
pub fn step_continuous(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, f64>,
    state: &BTreeMap<String, f64>,
    dt: f64,
    method: StepMethod,
) -> Result<BTreeMap<String, f64>, String> {
    let _ = (package, declaration, inputs, state, dt, method);
    super::gone()
}

/// Advance one explicit step, allowing vector-valued state and rates.
pub fn step_continuous_values(
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

/// Integrate from `t0` to `t1` with fixed `dt`. Includes the sample at `t0`.
pub fn simulate_continuous(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
) -> Result<Trajectory, String> {
    let _ = (package, declaration, inputs, state, t0, t1, dt, method);
    super::gone()
}

/// Integrate from `t0` to `t1`. Adaptive dt and one event locator are optional.
pub fn simulate_continuous_with(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
    options: &SimulateOptions,
) -> Result<Trajectory, String> {
    let _ = (package, declaration, inputs, state, t0, t1, dt, method, options);
    super::gone()
}

/// `simulate_continuous_with` plus the disposition record: the
/// structural index, the constraint/differential partition, the t0
/// initialization verdict — or the shared constructor refusal.
pub fn simulate_continuous_dispositioned(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
    options: &SimulateOptions,
) -> Result<(Trajectory, DAEDisposition), String> {
    let _ = (package, declaration, inputs, state, t0, t1, dt, method, options);
    super::gone()
}

/// Lower explicit rate definitions into a vector-argument callback.
/// The authored stepper cells own the integration policy; this entry
/// refuses as a non-constructor operation.
pub fn explicit_rate_program(
    package: &SemanticPackage,
    declaration: &Declaration,
) -> Result<crate::EmirProgram, String> {
    let _ = (package, declaration);
    super::gone()
}

/// VM inputs end with precomputed rates. Generated steps compute rates
/// from the frame. The authored stepper cells own the step formulas;
/// this entry refuses as a non-constructor operation.
pub fn explicit_step_program(
    package: &SemanticPackage,
    declaration: &Declaration,
    rk4: bool,
    generated: bool,
) -> Result<crate::EmirProgram, String> {
    let _ = (package, declaration, rk4, generated);
    super::gone()
}
