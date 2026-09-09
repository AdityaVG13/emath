//! failure-first tests: the native-path DAE
//! disposition record ('s "disposition artifact, not a naked
//! trajectory" contract, on the existing causalized Newton slice).
//!
//! Contracts (each must FAIL against the pre-surface):
//! - `simulate_continuous_dispositioned` returns a
//!   `DAEDisposition` beside the trajectory: an ODE-only model records
//!   `index: Ode` (no algebraic unknowns), and a causalized
//!   index-1 model (`algebraic:` + coupled residual) records
//!   `index: One` with a consistent-initialization verdict (the t0
//!   projection converged) and the constraint/differential partition.
//! - Non-convergent initialization at t0 is NOT presented as a
//!   trajectory: a missing algebraic guess or an inconsistent IC is a
//!   typed refusal with a continuation note, never a silent drop of
//!   the constraint.
//! - Replay: same source + inputs + policy → same disposition fields
//!   (determinism class).

use emath_exec_ir::interp::Value;
use emath_exec_ir::{SimulateOptions, StepMethod, simulate_continuous_dispositioned};
use emath_test_harness::{Probe, Source, boot};
use std::collections::BTreeMap;

const CAUSAL_RC: &str = "\
emath model CausalCircuit:
    inputs:
        V: Float64
        R: Float64
        C: Float64
    algebraic:
        I: Float64
    state:
        q: Float64
    equations:
        V - R * I - q / C == 0
        der(q) = I
";

const PURE_ODE: &str = "\
emath model Decay:
    inputs:
        k: Float64
    state:
        x: Float64
    equations:
        derivative(x) = -k * x
";

fn rc_inputs() -> BTreeMap<String, Value> {
    [("V", 10.0), ("R", 1.0), ("C", 1.0), ("I", 1.0)]
        .iter()
        .map(|(name, v)| (name.to_string(), Value::F64(*v)))
        .collect()
}

fn sim(
    probe: &mut Probe,
    name: &str,
    source: &str,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t1: f64,
    dt: f64,
    method: StepMethod,
) -> Result<
    (
        emath_exec_ir::Trajectory,
        emath_exec_ir::DAEDisposition,
    ),
    String,
> {
    let result = Source::from_str(name, source).must_admit(probe);
    if result.package.declarations.is_empty() {
        probe.fail(format!("{name}:decl"), "no declaration to simulate");
        return Err("admission failed; see diagnostics".to_string());
    }
    let decl = &result.package.declarations[0];
    simulate_continuous_dispositioned(
        &result.package,
        decl,
        inputs,
        state,
        0.0,
        t1,
        dt,
        method,
        &SimulateOptions::default(),
    )
}

#[test]
fn dae_disposition() {
    boot();
    let mut p = Probe::new("DAE runs return a disposition artifact beside the trajectory");
    p.case("index1", |p| {
        let mut state = BTreeMap::new();
        state.insert("q".into(), Value::F64(0.0));
        match sim(&mut *p, "rc", CAUSAL_RC, &rc_inputs(), &state, 1.0, 0.01, StepMethod::Rk4) {
            Err(error) => {
                p.fail("index1:run", format!("index-1 DAE must integrate: {error}"));
            }
            Ok((trajectory, disposition)) => {
                p.eq("index1:index", disposition.index, emath_exec_ir::DAEIndex::One);
                p.eq("index1:differential", disposition.differential_states, vec!["q".to_string()]);
                p.eq("index1:constraints", disposition.constraint_unknowns, vec!["I".to_string()]);
                p.eq(
                    "index1:init",
                    disposition.initialization,
                    emath_exec_ir::InitializationVerdict::Consistent,
                );
                p.eq("index1:continuation", disposition.continuation.is_none(), true);
                match trajectory.samples.last().and_then(|s| s.state.get("q")) {
                    Some(Value::F64(q)) => {
                        p.close("index1:q1", *q, 10.0 * (1.0 - (-1.0f64).exp()), 0.01);
                    }
                    _ => {
                        p.fail("index1:q1", "final q must be scalar");
                    }
                };
            }
        };
    });
    p.case("ode", |p| {
        let inputs = [("k", 1.0)].iter().map(|(n, v)| (n.to_string(), Value::F64(*v))).collect();
        let state = [("x", 1.0)].iter().map(|(n, v)| (n.to_string(), Value::F64(*v))).collect();
        match sim(&mut *p, "decay", PURE_ODE, &inputs, &state, 1.0, 0.1, StepMethod::Euler) {
            Err(error) => {
                p.fail("ode:run", format!("ODE must integrate: {error}"));
            }
            Ok((_, disposition)) => {
                p.eq("ode:index", disposition.index, emath_exec_ir::DAEIndex::Ode);
                p.ne("ode:not-index1", disposition.index, emath_exec_ir::DAEIndex::One);
                p.eq("ode:differential", disposition.differential_states, vec!["x".to_string()]);
                p.eq("ode:no-constraints", disposition.constraint_unknowns.is_empty(), true);
                p.eq(
                    "ode:init",
                    disposition.initialization,
                    emath_exec_ir::InitializationVerdict::Consistent,
                );
            }
        };
    });
    p.case("missing-guess", |p| {
        let inputs = [("V", 10.0), ("R", 1.0), ("C", 1.0)]
            .iter()
            .map(|(n, v)| (n.to_string(), Value::F64(*v)))
            .collect();
        let mut state = BTreeMap::new();
        state.insert("q".into(), Value::F64(0.0));
        match sim(&mut *p, "rc", CAUSAL_RC, &inputs, &state, 1.0, 0.01, StepMethod::Rk4) {
            Ok(_) => {
                p.fail("missing-guess:refuse", "missing algebraic guess must refuse, not simulate");
            }
            Err(error) => {
                p.contains("missing-guess:code", &error, "E-DAE-INIT");
                p.contains("missing-guess:target", &error, "algebraic");
            }
        };
    });
    p.case("singular", |p| {
        let inputs = [("V", 10.0), ("R", 0.0), ("C", 1.0), ("I", 1.0)]
            .iter()
            .map(|(n, v)| (n.to_string(), Value::F64(*v)))
            .collect();
        let mut state = BTreeMap::new();
        state.insert("q".into(), Value::F64(0.0));
        match sim(&mut *p, "rc", CAUSAL_RC, &inputs, &state, 1.0, 0.01, StepMethod::Rk4) {
            Ok(_) => {
                p.fail("singular:refuse", "singular residual system must refuse, not fake a trajectory");
            }
            Err(error) => {
                p.contains("singular:code", &error, "E-DAE-INIT");
            }
        };
    });
    p.case("replay", |p| {
        let result = Source::from_str("rc", CAUSAL_RC).must_admit(&mut *p);
        if result.package.declarations.is_empty() {
            p.fail("replay:decl", "no declaration to simulate");
            return;
        }
        let decl = &result.package.declarations[0];
        let mut state = BTreeMap::new();
        state.insert("q".into(), Value::F64(0.0));
        let run = || {
            simulate_continuous_dispositioned(
                &result.package,
                decl,
                &rc_inputs(),
                &state,
                0.0,
                1.0,
                0.01,
                StepMethod::Rk4,
                &SimulateOptions::default(),
            )
            .map(|(_, disposition)| disposition)
        };
        match (run(), run()) {
            (Ok(a), Ok(b)) => {
                p.eq("replay:disposition", a, b);
            }
            (first, second) => {
                p.fail("replay:run", format!("replay runs must succeed: {first:?} vs {second:?}"));
            }
        };
    });
    p.finish();
}
