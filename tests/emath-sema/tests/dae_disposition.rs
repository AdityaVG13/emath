//! emath-dae-disposition-b9flv failure-first tests: the native-path DAE
//! disposition record (bead's "disposition artifact, not a naked
//! trajectory" contract, on the existing causalized Newton slice).
//!
//! Contracts (each must FAIL against the pre-bead surface):
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

mod common;

use crate::common::check_source;
use emath_exec_ir::interp::Value;
use emath_exec_ir::{SimulateOptions, StepMethod, simulate_continuous_dispositioned};
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
    let mut inputs = BTreeMap::new();
    inputs.insert("V".into(), Value::F64(10.0));
    inputs.insert("R".into(), Value::F64(1.0));
    inputs.insert("C".into(), Value::F64(1.0));
    inputs.insert("I".into(), Value::F64(1.0));
    inputs
}

fn rc_state() -> BTreeMap<String, Value> {
    let mut state = BTreeMap::new();
    state.insert("q".into(), Value::F64(0.0));
    state
}

/// Positive: the causalized index-1 RC circuit integrates AND the
/// disposition names index One, a differential/constraint partition
/// (1 differential state `q`, 1 constraint unknown `I`), a
/// consistent-initialization verdict from the t0 projection, and a
/// `None` continuation (nothing owed).
#[test]
fn index1_dae_emits_disposition() {
    let result = check_source("rc", CAUSAL_RC);
    assert!(
        !result.diagnostics.has_errors(),
        "causalized DAE must admit, got: {:?}",
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    let (traj, disposition) = simulate_continuous_dispositioned(
        &result.package,
        decl,
        &rc_inputs(),
        &rc_state(),
        0.0,
        1.0,
        0.01,
        StepMethod::Rk4,
        &SimulateOptions::default(),
    )
    .expect("index-1 DAE must integrate with a disposition");
    assert_eq!(disposition.index, emath_exec_ir::DAEIndex::One);
    assert_eq!(disposition.differential_states, vec!["q".to_string()]);
    assert_eq!(disposition.constraint_unknowns, vec!["I".to_string()]);
    assert_eq!(
        disposition.initialization,
        emath_exec_ir::InitializationVerdict::Consistent
    );
    assert!(disposition.continuation.is_none());
    let q_final = match traj.samples.last().unwrap().state.get("q") {
        Some(Value::F64(v)) => *v,
        other => panic!("{other:?}"),
    };
    let expected = 10.0 * (1.0 - (-1.0f64).exp());
    assert!(
        (q_final - expected).abs() < 0.01,
        "trajectory unchanged by dispositioning: q(1) ~{expected:.4}, got {q_final:.4}"
    );
}

/// Positive control: an ODE-only model records `index: Ode` with an
/// empty constraint partition — the record exists for plain models
/// too, not just DAEs.
#[test]
fn ode_model_records_ode_index() {
    let result = check_source("decay", PURE_ODE);
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let mut inputs = BTreeMap::new();
    inputs.insert("k".into(), Value::F64(1.0));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::F64(1.0));
    let (_, disposition) = simulate_continuous_dispositioned(
        &result.package,
        decl,
        &inputs,
        &state,
        0.0,
        1.0,
        0.1,
        StepMethod::Euler,
        &SimulateOptions::default(),
    )
    .expect("ODE must integrate with a disposition");
    assert_eq!(disposition.index, emath_exec_ir::DAEIndex::Ode);
    assert_ne!(
        disposition.index,
        emath_exec_ir::DAEIndex::One,
        "an ODE model must NOT be classified as an index-1 DAE (misclassification would \
         pretend an algebraic constraint exists)"
    );
    assert_eq!(disposition.differential_states, vec!["x".to_string()]);
    assert!(disposition.constraint_unknowns.is_empty());
    assert_eq!(
        disposition.initialization,
        emath_exec_ir::InitializationVerdict::Consistent
    );
}

/// Negative control: a missing algebraic guess at t0 is a typed
/// refusal with a continuation note — never a trajectory that silently
/// dropped the algebraic constraint.
#[test]
fn missing_algebraic_guess_refuses_with_continuation() {
    let result = check_source("rc", CAUSAL_RC);
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let mut inputs = BTreeMap::new();
    inputs.insert("V".into(), Value::F64(10.0));
    inputs.insert("R".into(), Value::F64(1.0));
    inputs.insert("C".into(), Value::F64(1.0));
    // `I` guess deliberately absent.
    let err = simulate_continuous_dispositioned(
        &result.package,
        decl,
        &inputs,
        &rc_state(),
        0.0,
        1.0,
        0.01,
        StepMethod::Rk4,
        &SimulateOptions::default(),
    )
    .expect_err("missing algebraic guess must refuse, not simulate");
    assert!(
        err.contains("E-DAE-INIT"),
        "refusal must carry the E-DAE-INIT code and a continuation note, got: {err}"
    );
    assert!(
        err.contains("algebraic"),
        "refusal must name the algebraic unknown, got: {err}"
    );
}

/// Negative control: an inconsistent initialization (constraint can
/// never be satisfied for these inputs — R = 0 makes the residual
/// `V - q/C == 0` with no I dependence, and the Newton system for I is
/// singular) is a typed refusal with a continuation note, not a
/// trajectory presented as the DAE solution.
#[test]
fn singular_system_refuses_with_continuation() {
    let result = check_source("rc", CAUSAL_RC);
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let mut inputs = BTreeMap::new();
    inputs.insert("V".into(), Value::F64(10.0));
    inputs.insert("R".into(), Value::F64(0.0)); // I leaves the residual
    inputs.insert("C".into(), Value::F64(1.0));
    inputs.insert("I".into(), Value::F64(1.0));
    let err = simulate_continuous_dispositioned(
        &result.package,
        decl,
        &inputs,
        &rc_state(),
        0.0,
        1.0,
        0.01,
        StepMethod::Rk4,
        &SimulateOptions::default(),
    )
    .expect_err("singular residual system must refuse, not fake a trajectory");
    assert!(
        err.contains("E-DAE-INIT"),
        "singular system refusal must carry E-DAE-INIT and a continuation note, got: {err}"
    );
}

/// Replay: same source + inputs + numeric policy → identical
/// disposition fields (determinism class).
#[test]
fn disposition_is_replay_deterministic() {
    let result = check_source("rc", CAUSAL_RC);
    assert!(!result.diagnostics.has_errors());
    let decl = &result.package.declarations[0];
    let run = || {
        simulate_continuous_dispositioned(
            &result.package,
            decl,
            &rc_inputs(),
            &rc_state(),
            0.0,
            1.0,
            0.01,
            StepMethod::Rk4,
            &SimulateOptions::default(),
        )
        .expect("replay run must succeed")
        .1
    };
    let a = run();
    let b = run();
    assert_eq!(
        a, b,
        "same source + inputs + policy must replay the same disposition"
    );
}
