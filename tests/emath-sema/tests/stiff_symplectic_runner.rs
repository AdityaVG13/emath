//! Runner slice: wire the compute
//! layer's stiff/symplectic kernels into the simulate runner surface.
//!
//! Contracts:
//! - `StepMethod::BackwardEuler` is an IMPLICIT step: Newton on
//!   `r(x) = x − x_n − h·f(x)`. The stiff discriminator `y' = −50y` at
//!   h = 0.1 lands on the exact implicit point 1/6 (explicit Euler
//!   diverges to −4 there).
//! - Non-convergence refuses typed (`E-ODE-001`) — never a silently
//!   wrong trajectory point.
//! - `StepMethod::VelocityVerlet` is kick-drift-kick for the separable
//!   system `q' = v`, `v' = a(q)`: bounded energy drift and
//!   time-reversibility.
//! - The STRUCTURE gate refuses typed (`E-ODE-002`) when the model is
//!   not separable; non-positive `dt` refuses for BackwardEuler
//!   (`E-ODE-003`), while VelocityVerlet admits negative `h`.

use std::collections::BTreeMap;

use emath_exec_ir::interp::Value;
use emath_exec_ir::{StepMethod, step_continuous_values};
use emath_test_harness::{boot, Probe, Source};

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

fn scalar(p: &mut Probe, state: &BTreeMap<String, Value>, name: &str) -> f64 {
    match state.get(name) {
        Some(Value::F64(v)) => *v,
        other => {
            p.fail(name, format!("{name} must be F64, got {other:?}"));
            f64::NAN
        }
    }
}

#[test]
fn stiff_symplectic_contract() {
    boot();
    let mut p = Probe::new("stiff/symplectic runner kernels");
    p.case("implicit-point", |p| {
        let result = Source::from_str("stiff", stiff_model()).must_admit(p);
        let decl = &result.package.declarations[0];
        let next = step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state_of(&[("y", 1.0)]),
            0.1,
            StepMethod::BackwardEuler,
        );
        match next {
            Ok(state) => {
                let y1 = scalar(p, &state, "y");
                p.close("y1-is-1/6", y1, 1.0 / 6.0, 1e-12);
                p.demand("stable-envelope", y1 > 0.0, format!("explicit would be -4, got {y1}"));
            }
            Err(error) => {
                p.fail("implicit-step", format!("implicit step computes, got {error}"));
            }
        };
    });
    p.case("iterated-closed-form", |p| {
        let result = Source::from_str("stiff", stiff_model()).must_admit(p);
        let decl = &result.package.declarations[0];
        let mut y = 1.0f64;
        let mut ok = true;
        for _ in 0..3 {
            match step_continuous_values(
                &result.package,
                decl,
                &BTreeMap::new(),
                &state_of(&[("y", y)]),
                0.1,
                StepMethod::BackwardEuler,
            ) {
                Ok(next) => y = scalar(p, &next, "y"),
                Err(error) => {
                    ok = false;
                    p.fail("step", format!("step computes, got {error}"));
                    break;
                }
            }
        }
        if ok {
            p.close("y-is-(1/6)^3", y, (1.0f64 / 6.0).powi(3), 1e-12);
        }
    });
    p.case("nonlinear-residual", |p| {
        let result = Source::from_str("nonlinear", nonlinear_stiff_model()).must_admit(p);
        let decl = &result.package.declarations[0];
        let (y0, h) = (1.0f64, 0.01f64);
        match step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state_of(&[("y", y0)]),
            h,
            StepMethod::BackwardEuler,
        ) {
            Ok(next) => {
                let y1 = scalar(p, &next, "y");
                let residual = y1 - y0 - h * (-50.0 * y1 - y1 * y1 * y1);
                p.close("implicit-residual", residual, 0.0, 1e-10);
                p.demand("decaying", y1 < y0, format!("decaying mode, got {y1}"));
            }
            Err(error) => {
                p.fail("newton", format!("newton converges, got {error}"));
            }
        };
    });
    p.case("non-convergence", |p| {
        let result = Source::from_str(
            "blowup",
            "\
emath model BlowUp:
    state:
        y: Float64
    equations:
        derivative(y) = y * y
",
        )
        .must_admit(p);
        let decl = &result.package.declarations[0];
        match step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state_of(&[("y", 1.0)]),
            1.0,
            StepMethod::BackwardEuler,
        ) {
            Ok(state) => {
                let got = scalar(p, &state, "y");
                p.fail(
                    "must-refuse",
                    format!("no real implicit solution exists, got {got:?}"),
                )
            }
            Err(error) => p.contains("E-ODE-001", &error, "E-ODE-001"),
        };
    });
    p.case("non-positive-dt", |p| {
        let result = Source::from_str("stiff", stiff_model()).must_admit(p);
        let decl = &result.package.declarations[0];
        for dt in [0.0, -0.1] {
            match step_continuous_values(
                &result.package,
                decl,
                &BTreeMap::new(),
                &state_of(&[("y", 1.0)]),
                dt,
                StepMethod::BackwardEuler,
            ) {
                Ok(_) => p.fail(format!("dt={dt}"), "non-positive dt must refuse".to_string()),
                Err(error) => p.contains(format!("dt={dt}"), &error, "E-ODE-003"),
            };
        }
    });
    p.case("verlet-energy", |p| {
        let result = Source::from_str("osc", oscillator_model()).must_admit(p);
        let decl = &result.package.declarations[0];
        let steps = 200usize;
        let dt = 2.0 * std::f64::consts::PI / steps as f64;
        let mut state = state_of(&[("q", 1.0), ("v", 0.0)]);
        let mut ok = true;
        for _ in 0..steps {
            match step_continuous_values(
                &result.package,
                decl,
                &BTreeMap::new(),
                &state,
                dt,
                StepMethod::VelocityVerlet,
            ) {
                Ok(next) => state = next,
                Err(error) => {
                    ok = false;
                    p.fail("verlet-step", format!("verlet step computes, got {error}"));
                    break;
                }
            }
        }
        if ok {
            let (q, v) = (scalar(p, &state, "q"), scalar(p, &state, "v"));
            p.close("energy-drift", 0.5 * v * v + 0.5 * q * q, 0.5, 0.01);
        }
    });
    p.case("verlet-reversible", |p| {
        let result = Source::from_str("osc", oscillator_model()).must_admit(p);
        let decl = &result.package.declarations[0];
        let dt = 0.05f64;
        let mut state = state_of(&[("q", 1.0), ("v", 0.0)]);
        let mut ok = true;
        for step in std::iter::repeat(dt).take(40).chain(std::iter::repeat(-dt).take(40)) {
            match step_continuous_values(
                &result.package,
                decl,
                &BTreeMap::new(),
                &state,
                step,
                StepMethod::VelocityVerlet,
            ) {
                Ok(next) => state = next,
                Err(error) => {
                    ok = false;
                    p.fail("reversible-step", format!("step computes, got {error}"));
                    break;
                }
            }
        }
        if ok {
            let (q, v) = (scalar(p, &state, "q"), scalar(p, &state, "v"));
            p.close("q-returns", q, 1.0, 1e-9);
            p.close("v-returns", v, 0.0, 1e-9);
        }
    });
    p.case("structure-gate-damped", |p| {
        let result = Source::from_str("damped", damped_oscillator_model()).must_admit(p);
        let decl = &result.package.declarations[0];
        match step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state_of(&[("q", 1.0), ("v", 0.0)]),
            0.05,
            StepMethod::VelocityVerlet,
        ) {
            Ok(_) => p.fail("must-refuse", "non-separable model must refuse velocity Verlet".to_string()),
            Err(error) => p.contains("E-ODE-002", &error, "E-ODE-002"),
        };
    });
    p.case("structure-gate-shape", |p| {
        let result = Source::from_str(
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
        )
        .must_admit(p);
        let decl = &result.package.declarations[0];
        match step_continuous_values(
            &result.package,
            decl,
            &BTreeMap::new(),
            &state_of(&[("q", 1.0), ("v", 0.0)]),
            0.05,
            StepMethod::VelocityVerlet,
        ) {
            Ok(_) => p.fail("must-refuse", "q' = 2v is not the separable carrier".to_string()),
            Err(error) => p.contains("E-ODE-002", &error, "E-ODE-002"),
        };
    });
    p.finish();
}
