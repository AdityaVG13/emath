//! emath-xx0x.3 runner slice (BronzeCoyote lane): wire the compute
//! layer's stiff/symplectic kernels into the simulate runner surface.
//!
//! Contracts (each must FAIL against the pre-slice runner):
//! - `StepMethod::BackwardEuler` is an IMPLICIT step: the model rate is
//!   evaluated at the NEXT state via Newton on
//!   `r(x) = x − x_n − h·f(x)` (scalar carrier per the nucleus slice).
//!   The stiff discriminator `y' = −50y` at h = 0.1 lands on the exact
//!   implicit point 1/6 (explicit Euler diverges to −4 there).
//! - Non-convergence refuses typed (`E-ODE-001` in the message) — never
//!   a silently wrong trajectory point.
//! - `StepMethod::VelocityVerlet` is kick-drift-kick for the separable
//!   system `q' = v`, `v' = a(q)`: energy drift ≪ Euler's on the
//!   harmonic oscillator, and time-reversible (N forward + N backward
//!   steps with −dt return the start state).
//! - The STRUCTURE gate refuses typed (`E-ODE-002` in the message) when
//!   the model is not separable (`q' = v` identity fails or the
//!   acceleration depends on `v`) — symplectic integrators preserve
//!   structure only for structure-preserving problems.
//! - Non-positive `dt` refuses for BackwardEuler (`E-ODE-003` shape);
//!   VelocityVerlet admits negative `h` (time reversal is the point).

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::{SimulateOptions, StepMethod, step_continuous_values};
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;
use std::collections::BTreeMap;

fn check_source(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

fn stiff_model() -> &'static str {
    "\
emath model StiffDecay:
    state:
        y: Float64
    equations:
        derivative(y) = -50 * y
"
}

fn nonlinear_stiff_model() -> &'static str {
    "\
emath model NonlinearStiff:
    state:
        y: Float64
    equations:
        derivative(y) = -50 * y - y * y * y
"
}

fn oscillator_model() -> &'static str {
    "\
emath model HarmonicOscillator:
    state:
        q: Float64
        v: Float64
    equations:
        derivative(q) = v
        derivative(v) = -q
"
}

fn damped_oscillator_model() -> &'static str {
    "\
emath model DampedOscillator:
    state:
        q: Float64
        v: Float64
    equations:
        derivative(q) = v
        derivative(v) = -q - 0.1 * v
"
}

fn state_of(names: &[(&str, f64)]) -> BTreeMap<String, Value> {
    names
        .iter()
        .map(|(name, value)| ((*name).to_string(), Value::F64(*value)))
        .collect()
}

/// The stiff discriminator: one implicit step of y' = −50y at h = 0.1
/// lands on the closed-form implicit point y₁ = 1/(1 + 50h) = 1/6
/// (machine-exact — Newton on a linear rate converges exactly), while
/// the EXPLICIT step would be 1 − 5 = −4 (already outside the stable
/// decay envelope).
#[test]
fn backward_euler_one_step_is_implicit_point() {
    let result = check_source("stiff", stiff_model());
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let state = state_of(&[("y", 1.0)]);
    let next = step_continuous_values(
        &result.package,
        decl,
        &BTreeMap::new(),
        &state,
        0.1,
        StepMethod::BackwardEuler,
    )
    .expect("implicit step computes");
    let y1 = match next.get("y") {
        Some(Value::F64(v)) => *v,
        other => panic!("{other:?}"),
    };
    assert!(
        (y1 - 1.0 / 6.0).abs() < 1e-12,
        "backward Euler one step must be the implicit point 1/6, got {y1} (explicit would be -4)"
    );
    assert!(y1 > 0.0, "decay stays in the stable envelope, got {y1}");
}

/// Iterated implicit steps accumulate the closed form (1/6)³ — a
/// mutant that takes ONE fixed-point iteration instead of Newton's
/// full convergence fails the accumulated law.
#[test]
fn backward_euler_iterated_matches_closed_form() {
    let result = check_source("stiff", stiff_model());
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let mut y = 1.0f64;
    for _ in 0..3 {
        let next = step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state_of(&[("y", y)]),
            0.1,
            StepMethod::BackwardEuler,
        )
        .expect("step computes");
        y = match next.get("y") {
            Some(Value::F64(v)) => *v,
            other => panic!("{other:?}"),
        };
    }
    let expected = (1.0f64 / 6.0).powi(3);
    assert!((y - expected).abs() < 1e-12, "(1/6)³ law, got {y}");
}

/// The implicit residual law holds at the returned point for a
/// NONLINEAR rate (proves Newton, not a single explicit evaluation):
/// |y₁ − y₀ − h·f(y₁)| ≤ 1e-10.
#[test]
fn backward_euler_nonlinear_satisfies_implicit_residual() {
    let result = check_source("nonlinear", nonlinear_stiff_model());
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let y0 = 1.0f64;
    let h = 0.01f64;
    let next = step_continuous_values(
        &result.package,
        decl,
        &BTreeMap::new(),
        &state_of(&[("y", y0)]),
        h,
        StepMethod::BackwardEuler,
    )
    .expect("newton converges");
    let y1 = match next.get("y") {
        Some(Value::F64(v)) => *v,
        other => panic!("{other:?}"),
    };
    let rate_at = |y: f64| -50.0 * y - y * y * y;
    let residual = y1 - y0 - h * rate_at(y1);
    assert!(
        residual.abs() < 1e-10,
        "implicit residual law, got {residual} (y1={y1})"
    );
    assert!(y1 < y0, "decaying mode, got {y1}");
}

/// Negative control: when the implicit equation has NO real solution
/// (y' = y² at h = 1: discriminant 1 − 4h·y₀ < 0) Newton cannot
/// converge and the step refuses typed — never a silently wrong point.
#[test]
fn backward_euler_non_convergence_refuses_typed() {
    let result = check_source(
        "blowup",
        "\
emath model BlowUp:
    state:
        y: Float64
    equations:
        derivative(y) = y * y
",
    );
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let error = step_continuous_values(
        &result.package,
        decl,
        &BTreeMap::new(),
        &state_of(&[("y", 1.0)]),
        1.0, // no real implicit solution exists
        StepMethod::BackwardEuler,
    )
    .expect_err("non-convergent implicit step must refuse");
    assert!(
        error.contains("E-ODE-001"),
        "refusal must name E-ODE-001, got: {error}"
    );
}

/// Non-positive dt refuses for the implicit step (E-ODE-003 shape).
#[test]
fn backward_euler_non_positive_dt_refuses() {
    let result = check_source("stiff", stiff_model());
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    for dt in [0.0, -0.1] {
        let error = step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state_of(&[("y", 1.0)]),
            dt,
            StepMethod::BackwardEuler,
        )
        .expect_err("non-positive dt refuses");
        assert!(
            error.contains("E-ODE-003"),
            "dt={dt} must name E-ODE-003, got: {error}"
        );
    }
}

/// Velocity Verlet on the harmonic oscillator: energy drift after a
/// full period is bounded and small (≪ explicit Euler's secular
/// growth).
#[test]
fn velocity_verlet_energy_bounded() {
    let result = check_source("osc", oscillator_model());
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let steps = 200usize;
    let dt = 2.0 * std::f64::consts::PI / steps as f64;
    let mut state = state_of(&[("q", 1.0), ("v", 0.0)]);
    for _ in 0..steps {
        state = step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state,
            dt,
            StepMethod::VelocityVerlet,
        )
        .expect("verlet step computes");
    }
    let (q, v) = match (state.get("q"), state.get("v")) {
        (Some(Value::F64(q)), Some(Value::F64(v))) => (*q, *v),
        other => panic!("{other:?}"),
    };
    let energy = 0.5 * v * v + 0.5 * q * q;
    assert!(
        (energy - 0.5).abs() < 0.01,
        "velocity Verlet energy drift after one period must be small, got E={energy} (q={q}, v={v})"
    );
}

/// Time-reversibility (the symplectic law): N steps forward then N
/// steps with −dt return the start state within the numeric policy.
/// A negative step dt is legal for Verlet (the compute layer allows
/// it; the runner surface honors the same contract).
#[test]
fn velocity_verlet_reversibility() {
    let result = check_source("osc", oscillator_model());
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let dt = 0.05f64;
    let mut state = state_of(&[("q", 1.0), ("v", 0.0)]);
    for _ in 0..40 {
        state = step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state,
            dt,
            StepMethod::VelocityVerlet,
        )
        .expect("forward step");
    }
    for _ in 0..40 {
        state = step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state,
            -dt,
            StepMethod::VelocityVerlet,
        )
        .expect("backward step");
    }
    let (q, v) = match (state.get("q"), state.get("v")) {
        (Some(Value::F64(q)), Some(Value::F64(v))) => (*q, *v),
        other => panic!("{other:?}"),
    };
    assert!(
        (q - 1.0).abs() < 1e-9 && v.abs() < 1e-9,
        "reversibility: (q, v) = (1, 0), got ({q}, {v})"
    );
}

/// The STRUCTURE gate (negative control): a damped oscillator
/// (`v' = −q − 0.1·v`) is NOT separable — the acceleration depends on
/// v — so velocity Verlet refuses typed instead of silently applying
/// a structure-violating integrator.
#[test]
fn velocity_verlet_non_separable_refuses_typed() {
    let result = check_source("damped", damped_oscillator_model());
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let error = step_continuous_values(
        &result.package,
        decl,
        &BTreeMap::new(),
        &state_of(&[("q", 1.0), ("v", 0.0)]),
        0.05,
        StepMethod::VelocityVerlet,
    )
    .expect_err("non-separable model must refuse velocity Verlet");
    assert!(
        error.contains("E-ODE-002"),
        "structure gate must name E-ODE-002, got: {error}"
    );
}

/// A model whose `q` rate is not the velocity identity (`q' ≠ v`) is
/// also outside the separable form — refused, not silently integrated.
#[test]
fn velocity_verlet_non_identity_q_rate_refuses() {
    let result = check_source(
        "wrongshape",
        "\
emath model WrongShape:
    state:
        q: Float64
        v: Float64
    equations:
        derivative(q) = 2 * v
        derivative(v) = -q
",
    );
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let error = step_continuous_values(
        &result.package,
        decl,
        &BTreeMap::new(),
        &state_of(&[("q", 1.0), ("v", 0.0)]),
        0.05,
        StepMethod::VelocityVerlet,
    )
    .expect_err("q' = 2v is not the separable carrier");
    assert!(
        error.contains("E-ODE-002"),
        "structure gate must name E-ODE-002, got: {error}"
    );
}
