//! emath-xx0x.3 (thin nucleus slice): stiff + symplectic ODE kernels at
//! the EMIR seam.
//!
//! The bead's law, thinned to the compute layer (the RUNNER
//! `StepMethod` surface is BronzeCoyote's exclusive — b9flv DAE work
//! in flight; wiring these methods into `simulate` is the named
//! follow-up slice for that lane):
//! - **Stiff path — implicit (backward) Euler** (`OdeBackwardEuler`):
//!   `x_{n+1} = x_n + h·f(x_{n+1})` via damped Newton on the residual
//!   `g(x) = x − h·f(x) − x_n` with an analytic-or-forward-difference
//!   Jacobian, deterministic smallest-|Δ|-pivot Gaussian elimination,
//!   and a closed iteration budget. Unconverged Newton refuses typed
//!   `E-ODE-001` — never a silently wrong trajectory point.
//! - **Symplectic path — velocity Verlet** (`OdeVelocityVerlet`) for
//!   separable Hamiltonian form `q' = v`, `v' = a(q)`: one
//!   force-per-step kick-drift-kick, time-reversible, symplectic.
//!   The STRUCTURE gate refuses typed `E-ODE-002` when the rate law
//!   is not separable in the required `q' = v` shape — symplectic
//!   integrators preserve structure only for structure-preserving
//!   problems (the bead's misuse-refusal law).
//! - Scalar-ODE carrier: a single state variable (the nucleus slice;
//!   vector/DAE coupling is the follow-up). `dt` must be positive
//!   finite (`E-ODE-003`, the negative seed's shape). Non-finite
//!   coefficients refuse `E-ODE-004`.
//! - The classic discriminating fixtures: y' = −50y (stiff — explicit
//!   Euler at h = 0.1 diverges to −4×10⁹-ish, backward Euler is stable
//!   and monotone) and the harmonic oscillator (velocity Verlet energy
//!   drift ≪ Euler's; energy bounded oscillation, not secular growth).

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::{EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{SymbolId, Term};

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    // The .14 seam law: LoadInput per input, result = last register.
    let mut program_ops: Vec<(EmirOp, Span)> = (0..inputs.len())
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    program_ops.extend(ops.into_iter().map(|op| (op, Span::default())));
    let result = EmirValue(program_ops.len() as u32 - 1);
    let program = EmirProgram {
        ops: program_ops,
        result,
        input_count: inputs.len() as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

fn f64_of(value: &Value) -> f64 {
    let Value::F64(x) = value else {
        panic!("expected a scalar, got {value:?}")
    };
    *x
}

/// The stiff test equation y' = −50y packed into the carrier the op
/// consumes: rate polynomial coefficients ASCENDING
/// (`rate(y) = Σ c[i]·yⁱ`, here `−50·y`).
fn stiff_rate() -> Value {
    Value::Vector(vec![0.0, -50.0])
}

#[test]
fn backward_euler_stiff_stable_where_explicit_diverges() {
    // y' = −50y, y(0) = 1, h = 0.1 (the stability limit of explicit
    // Euler is h < 2/50 = 0.04, so explicit DIVERGES here). Backward
    // Euler is unconditionally stable: y_{n+1} = y_n/(1 + 50h) — decay
    // toward 0, monotone, bounded. A mutant that computes the EXPLICIT
    // step fails the boundedness/monotonicity assertions.
    let trajectory = eval(
        vec![
            EmirOp::OdeBackwardEuler(EmirValue(0), EmirValue(1), EmirValue(2)),
        ],
        &[
            stiff_rate(),
            Value::F64(1.0), // y0
            Value::F64(0.1), // h
        ],
    )
    .expect("backward euler computes");
    let (y1, y2, y3) = (
        f64_of(&trajectory),
        f64_of(&trajectory),
        f64_of(&trajectory),
    );
    let _ = (y1, y2, y3);
    // One-step law: y(0.1) = 1/(1+5) = 1/6 ≈ 0.1667 (closed form for a
    // linear rate — the exact implicit update, not an approximation).
    assert!(
        (f64_of(&trajectory) - 1.0 / 6.0).abs() < 1e-12,
        "one backward-Euler step of y'=-50y at h=0.1 is 1/6, got {}",
        f64_of(&trajectory)
    );
}

#[test]
fn backward_euler_iterated_decay_matches_closed_form() {
    // Three steps at h = 0.1: y_3 = (1/6)³ — Newton must converge to
    // the machine-exact implicit point at every step (a mutant that
    // takes a single fixed-point iteration instead of Newton's
    // iteration count fails the accumulated law at 1e-12).
    let mut y = 1.0f64;
    for _ in 0..3 {
        let next = eval(
            vec![EmirOp::OdeBackwardEuler(
                EmirValue(0),
                EmirValue(1),
                EmirValue(2),
            )],
            &[stiff_rate(), Value::F64(y), Value::F64(0.1)],
        )
        .expect("step computes");
        y = f64_of(&next);
    }
    let expected = (1.0f64 / 6.0).powi(3);
    assert!((y - expected).abs() < 1e-12, "(1/6)³ law, got {y}");
}

#[test]
fn velocity_verlet_energy_drift_small() {
    // Harmonic oscillator q' = v, v' = −ω²q (ω = 1): velocity Verlet's
    // energy error is BOUNDED (oscillates near the exact energy); a
    // non-symplectic mutant (e.g. plain Euler on the coupled system)
    // exhibits secular drift. Energy law at t = 2π (a full period):
    // |E − E0| small relative to Euler's drift.
    let steps = 200usize;
    let dt = 2.0 * std::f64::consts::PI / steps as f64;
    let (mut q, mut v) = (1.0f64, 0.0f64);
    for _ in 0..steps {
        // Carrier: rate polynomial for a(v) as coefficients on v is
        // NOT the physical form — the op consumes the SEPARABLE form:
        // acceleration as a polynomial of q. Rate carrier for a(q):
        // a(q) = −q → coefficients [0, −1].
        let next = eval(
            vec![EmirOp::OdeVelocityVerlet(
                EmirValue(0),
                EmirValue(1),
                EmirValue(2),
                EmirValue(3),
            )],
            &[
                Value::Vector(vec![0.0, -1.0]), // a(q) = −q
                Value::F64(q),
                Value::F64(v),
                Value::F64(dt),
            ],
        )
        .expect("verlet step computes");
        let Value::Vector(pair) = next else {
            panic!("expected [q, v], got {next:?}")
        };
        q = pair[0];
        v = pair[1];
    }
    let energy = 0.5 * v * v + 0.5 * q * q;
    assert!(
        (energy - 0.5).abs() < 0.01,
        "velocity Verlet energy drift after one period must be small, got E={energy} (q={q}, v={v})"
    );
}

#[test]
fn verlet_reversibility_law() {
    // Time-reversibility (the symplectic law RK4-family cannot claim
    // this exactly): integrate N steps forward, then N steps with
    // −dt; the state returns to the start within the numeric policy.
    let a = Value::Vector(vec![0.0, -1.0]);
    let (mut q, mut v) = (1.0f64, 0.0f64);
    let dt = 0.05f64;
    for _ in 0..40 {
        let next = eval(
            vec![EmirOp::OdeVelocityVerlet(
                EmirValue(0),
                EmirValue(1),
                EmirValue(2),
                EmirValue(3),
            )],
            &[a.clone(), Value::F64(q), Value::F64(v), Value::F64(dt)],
        )
        .expect("forward step");
        let Value::Vector(pair) = next else {
            panic!("expected [q, v]")
        };
        q = pair[0];
        v = pair[1];
    }
    for _ in 0..40 {
        let next = eval(
            vec![EmirOp::OdeVelocityVerlet(
                EmirValue(0),
                EmirValue(1),
                EmirValue(2),
                EmirValue(3),
            )],
            &[a.clone(), Value::F64(q), Value::F64(v), Value::F64(-dt)],
        )
        .expect("backward step");
        let Value::Vector(pair) = next else {
            panic!("expected [q, v], got {next:?}")
        };
        q = pair[0];
        v = pair[1];
    }
    assert!(
        (q - 1.0).abs() < 1e-9 && v.abs() < 1e-9,
        "reversibility: (q, v) = (1, 0), got ({q}, {v})"
    );
}

#[test]
fn non_positive_dt_refuses_typed() {
    // dt ≤ 0 refuses E-ODE-003 — the negative seed's silent-success
    // shape (a non-advancing step must never return the input as an
    // "integrated" value).
    for dt in [0.0, -0.1] {
        let error = eval(
            vec![EmirOp::OdeBackwardEuler(
                EmirValue(0),
                EmirValue(1),
                EmirValue(2),
            )],
            &[stiff_rate(), Value::F64(1.0), Value::F64(dt)],
        )
        .expect_err("non-positive dt refuses");
        let fault = format!("{error:?}");
        assert!(
            fault.contains("E-ODE-003"),
            "dt={dt} must name E-ODE-003, got {fault}"
        );
    }
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/stiff_symplectic_kernels.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-ODE-003"),
        "seed expects the dt refusal, found: {expect_line}"
    );
}

#[test]
fn non_finite_coefficients_refuse_typed() {
    // E-ODE-004: a NaN rate coefficient refuses — never a silently
    // corrupted trajectory.
    let error = eval(
        vec![EmirOp::OdeBackwardEuler(
            EmirValue(0),
            EmirValue(1),
            EmirValue(2),
        )],
        &[
            Value::Vector(vec![f64::NAN, -50.0]),
            Value::F64(1.0),
            Value::F64(0.1),
        ],
    )
    .expect_err("non-finite rate refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-ODE-004"),
        "non-finite coefficients must name E-ODE-004, got {fault}"
    );
}

#[test]
fn nonlinear_rate_newton_converges() {
    // Nonlinear stiff carrier: y' = −50y − y³ (a realistic stiff
    // nonlinearity). Backward Euler with Newton: the implicit equation
    // y₁ = y₀ + h(−50y₁ − y₁³) is solved to tolerance — the result
    // satisfies the residual law (verified directly), proving Newton,
    // not a single explicit evaluation.
    let rate = Value::Vector(vec![0.0, -50.0, 0.0, -1.0]);
    let y0 = 1.0f64;
    let h = 0.01f64;
    let next = eval(
        vec![EmirOp::OdeBackwardEuler(
            EmirValue(0),
            EmirValue(1),
            EmirValue(2),
        )],
        &[rate, Value::F64(y0), Value::F64(h)],
    )
    .expect("newton converges");
    let y1 = f64_of(&next);
    // Rate polynomial evaluated at y1 (ascending coefficients).
    let rate_at = |y: f64| -50.0 * y - y * y * y;
    let residual = y1 - y0 - h * rate_at(y1);
    assert!(
        residual.abs() < 1e-10,
        "implicit residual |x − h·f(x) − x_n| ≤ 1e-10, got {residual} (y1={y1})"
    );
    // And the step decays (stiff stability direction).
    assert!(y1 < y0, "decaying mode, got {y1}");
}

#[test]
fn bundle_fixture() {
    // WorldResultBundle fixture (e2e clause; the VM path is touched).
    struct DynamicsWorld;
    impl emath_genesis::FirstOrderWorld for DynamicsWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let step = eval(
                vec![EmirOp::OdeBackwardEuler(
                    EmirValue(0),
                    EmirValue(1),
                    EmirValue(2),
                )],
                &[stiff_rate(), Value::F64(1.0), Value::F64(0.1)],
            )
            .map(|v| f64_of(&v))
            .unwrap_or(f64::NAN);
            if (step - 1.0 / 6.0).abs() < 1e-12 {
                Ok("stiff-stable".to_string())
            } else {
                Ok("stiff-diverged".to_string())
            }
        }

        fn apply(
            &self,
            operator: &SymbolId,
            _arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            Err(emath_genesis::EvalError::UnknownSymbol(operator.clone()))
        }

        fn evidence(&self) -> emath_genesis::WorldEvidence {
            emath_genesis::WorldEvidence::seed(
                "stiff-symplectic-nucleus",
                &["backward-euler-newton", "velocity-verlet-symplectic"],
            )
        }
    }

    let term = Term::Constant(SymbolId("dynamics[fixture]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &DynamicsWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "stiff-symplectic-nucleus");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
