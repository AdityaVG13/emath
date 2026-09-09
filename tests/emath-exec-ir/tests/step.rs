//! Explicit Euler / RK4 on admitted `der_<state>` rates.

use emath_core::{QualifiedName, Span};
use emath_exec_ir::interp::Value;
use emath_exec_ir::{
    SimulateOptions, StepMethod, simulate_continuous, simulate_continuous_with, step_continuous,
    step_continuous_values,
};
use emath_ir::{
    Declaration, DeclarationId, ExprNode, Field, SemanticPackage, TypeNode, UnaryOp, Visibility,
};
use std::collections::BTreeMap;

use emath_test_harness::Probe;

fn float_field(name: &str, ty: emath_ir::TypeId) -> Field {
    Field {
        name: name.to_string(),
        ty,
        visibility: Visibility::Public,
        source: Span::default(),
    }
}

/// Autonomous `ẋ = -x`.
fn decay_package() -> SemanticPackage {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("state.x")),
        Span::default(),
    );
    let rate = package.push_expr(
        ExprNode::Unary {
            operation: UnaryOp::Negate,
            value: x,
        },
        Span::default(),
    );
    let mut definitions = BTreeMap::new();
    definitions.insert("der_x".to_string(), rate);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Decay"),
        kind: QualifiedName::single("model"),
        kind_label: "model".to_string(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        state: vec![float_field("x", ty)],
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: emath_ir::CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    package
}


/// assert_eq/assert_ne semantics over references: borrows both operands
/// (like the macros) and allows PartialEq between distinct types.
fn eq_ref<T: ?Sized + std::fmt::Debug, U: ?Sized + std::fmt::Debug>(
    ph: &mut Probe,
    name: impl Into<String>,
    actual: &T,
    expected: &U,
) where
    T: PartialEq<U>,
{
    ph.demand(name, actual == expected, format!("expected {expected:?}, got {actual:?}"));
}

fn ne_ref<T: ?Sized + std::fmt::Debug, U: ?Sized + std::fmt::Debug>(
    ph: &mut Probe,
    name: impl Into<String>,
    actual: &T,
    unexpected: &U,
) where
    T: PartialEq<U>,
{
    ph.demand(name, actual != unexpected, format!("got forbidden value {unexpected:?}"));
}

fn euler_decay_one_step(ph: &mut Probe) {
    ph.case("euler_decay_one_step", |ph| {
    let package = decay_package();
    let declaration = &package.declarations[0];
    let inputs = BTreeMap::new();
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), 1.0);
    let next = step_continuous(
        &package,
        declaration,
        &inputs,
        &state,
        0.1,
        StepMethod::Euler,
    )
    .unwrap();
    eq_ref(ph, "1", &(next.get("x").copied()), &( Some(0.9)));

    });
}

fn rk4_decay_beats_euler(ph: &mut Probe) {
    ph.case("rk4_decay_beats_euler", |ph| {
    let package = decay_package();
    let declaration = &package.declarations[0];
    let inputs = BTreeMap::new();
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), 1.0);
    let euler = step_continuous(
        &package,
        declaration,
        &inputs,
        &state,
        0.5,
        StepMethod::Euler,
    )
    .unwrap();
    let rk4 =
        step_continuous(&package, declaration, &inputs, &state, 0.5, StepMethod::Rk4).unwrap();
    let exact = (-0.5_f64).exp();
    let euler_err = (euler["x"] - exact).abs();
    let rk4_err = (rk4["x"] - exact).abs();
    ph.demand("1", rk4_err < euler_err, format!( "rk4={rk4_err} euler={euler_err}"));

    });
}

fn missing_rate_is_refused(ph: &mut Probe) {
    ph.case("missing_rate_is_refused", |ph| {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Float64);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("Bare"),
        kind: QualifiedName::single("model"),
        kind_label: "model".to_string(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        state: vec![float_field("x", ty)],
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions: BTreeMap::new(),
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: emath_ir::CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let declaration = &package.declarations[0];
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), 1.0);
    let error = step_continuous(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.1,
        StepMethod::Euler,
    )
    .unwrap_err();
    ph.demand("1", error.contains("der_x"), format!( "{error}"));

    });
}

fn non_positive_dt_is_refused(ph: &mut Probe) {
    ph.case("non_positive_dt_is_refused", |ph| {
    let package = decay_package();
    let declaration = &package.declarations[0];
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), 1.0);
    let error = step_continuous(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.0,
        StepMethod::Euler,
    )
    .unwrap_err();
    ph.demand("1", error.contains("step size"), format!( "{error}"));

    });
}

fn simulate_decay_includes_endpoints(ph: &mut Probe) {
    ph.case("simulate_decay_includes_endpoints", |ph| {
    let package = decay_package();
    let declaration = &package.declarations[0];
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), Value::F64(1.0));
    let trajectory = simulate_continuous(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.0,
        0.2,
        0.1,
        StepMethod::Euler,
    )
    .unwrap();
    eq_ref(ph, "1", &(trajectory.samples.len()), &( 3));
    eq_ref(ph, "2", &(trajectory.samples[0].t), &( 0.0));
    eq_ref(ph, "3", &(trajectory.samples[2].t), &( 0.2));
    eq_ref(ph, "4", &(
        trajectory.samples[2].state.get("x")), &(
        Some(&Value::F64(0.81))
    ));

    });
}

fn simulate_zero_span_returns_initial_sample(ph: &mut Probe) {
    ph.case("simulate_zero_span_returns_initial_sample", |ph| {
    let package = decay_package();
    let declaration = &package.declarations[0];
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), Value::F64(1.0));
    let trajectory = simulate_continuous(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.5,
        0.5,
        0.1,
        StepMethod::Euler,
    )
    .expect("t1 == t0 is a 0-step trajectory, not a panic");
    eq_ref(ph, "1", &(trajectory.samples.len()), &( 1));
    eq_ref(ph, "2", &(trajectory.samples[0].t), &( 0.5));
    eq_ref(ph, format!(
        "0-step simulate must keep the initial state"
    ), &(
        trajectory.samples[0].state.get("x")), &(
        Some(&Value::F64(1.0))));

    });
}

fn rk45_decay_is_finite(ph: &mut Probe) {
    ph.case("rk45_decay_is_finite", |ph| {
    let package = decay_package();
    let declaration = &package.declarations[0];
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), 1.0);
    let next = step_continuous(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.5,
        StepMethod::Rk45,
    )
    .unwrap();
    ph.demand("1", next["x"].is_finite(), "assertion failed: next[\"x\"].is_finite()");
    ph.demand("2", next["x"] > 0.0, "assertion failed: next[\"x\"] > 0.0");

    });
}

fn vector_state_euler_steps_componentwise(ph: &mut Probe) {
    ph.case("vector_state_euler_steps_componentwise", |ph| {
    let mut package = SemanticPackage::new();
    let ty = package.push_type(TypeNode::Vector {
        element: Box::new(TypeNode::Float64),
        extent: Some(emath_ir::Extent::Fixed(2)),
    });
    let x = package.push_expr(
        ExprNode::Variable(QualifiedName::single("state.x")),
        Span::default(),
    );
    let mut definitions = BTreeMap::new();
    definitions.insert("der_x".to_string(), x);
    package.declarations.push(Declaration {
        id: DeclarationId(0),
        name: QualifiedName::single("VecDecay"),
        kind: QualifiedName::single("model"),
        kind_label: "model".to_string(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        state: vec![float_field("x", ty)],
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions,
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: emath_ir::CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: Span::default(),
    });
    let declaration = &package.declarations[0];
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), Value::Vector(vec![1.0, 2.0]));
    let next = step_continuous_values(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.5,
        StepMethod::Euler,
    )
    .unwrap();
    eq_ref(ph, "1", &(next.get("x")), &( Some(&Value::Vector(vec![1.5, 3.0]))));

    });
}

fn adaptive_decay_uses_fewer_steps_than_tiny_fixed(ph: &mut Probe) {
    ph.case("adaptive_decay_uses_fewer_steps_than_tiny_fixed", |ph| {
    let package = decay_package();
    let declaration = &package.declarations[0];
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), Value::F64(1.0));
    let fixed = simulate_continuous(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.0,
        1.0,
        1e-4,
        StepMethod::Rk45,
    )
    .unwrap();
    let adaptive = simulate_continuous_with(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.0,
        1.0,
        0.1,
        StepMethod::Rk45,
        &SimulateOptions {
            atol: Some(1e-6),
            rtol: Some(1e-6),
            dt_max: Some(0.2),
            event: None,
        },
    )
    .unwrap();
    ph.demand("1", 
        adaptive.samples.len() < fixed.samples.len(), format!(
        "adaptive={} fixed={}", 
        adaptive.samples.len(), 
        fixed.samples.len()
    ));
    let last = match adaptive.samples.last().unwrap().state.get("x") {
        Some(Value::F64(value)) => *value,
        other => panic!("expected scalar x, got {other:?}"),
    };
    ph.demand("2", (last - (-1.0_f64).exp()).abs() < 1e-4, format!( "last={last}"));

    });
}

fn non_positive_atol_is_refused(ph: &mut Probe) {
    ph.case("non_positive_atol_is_refused", |ph| {
    let package = decay_package();
    let declaration = &package.declarations[0];
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), Value::F64(1.0));
    let error = simulate_continuous_with(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.0,
        1.0,
        0.1,
        StepMethod::Rk45,
        &SimulateOptions {
            atol: Some(0.0),
            rtol: None,
            dt_max: None,
            event: None,
        },
    )
    .unwrap_err();
    ph.demand("1", error.contains("atol"), format!( "{error}"));

    });
}

fn adaptive_refuses_nan_initial_state(ph: &mut Probe) {
    ph.case("adaptive_refuses_nan_initial_state", |ph| {
    // NaN fourth/fifth pairs used to report err=0 via f64::max ignoring NaN,
    // so adaptive RK45 silently "converged" on a poisoned trajectory.
    let package = decay_package();
    let declaration = &package.declarations[0];
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), Value::F64(f64::NAN));
    let error = simulate_continuous_with(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.0,
        1.0,
        0.1,
        StepMethod::Rk45,
        &SimulateOptions {
            atol: Some(1e-6),
            rtol: Some(1e-6),
            dt_max: Some(0.2),
            event: None,
        },
    )
    .unwrap_err();
    ph.demand("1", 
        error.contains("non-finite"), format!(
        "expected non-finite refusal, got: {error}"
    ));

    });
}

fn event_stops_when_x_crosses_half(ph: &mut Probe) {
    ph.case("event_stops_when_x_crosses_half", |ph| {
    let package = decay_package();
    let declaration = &package.declarations[0];
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), Value::F64(1.0));
    let trajectory = simulate_continuous_with(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.0,
        2.0,
        0.1,
        StepMethod::Rk45,
        &SimulateOptions {
            atol: None,
            rtol: None,
            dt_max: None,
            event: Some(("x".to_string(), 0.5)),
        },
    )
    .unwrap();
    let last = trajectory.samples.last().unwrap();
    let x = match last.state.get("x") {
        Some(Value::F64(value)) => *value,
        other => panic!("expected scalar x, got {other:?}"),
    };
    ph.demand("1", (x - 0.5).abs() < 1e-6, format!( "x={x} t={}",  last.t));
    ph.demand("2", last.t < 1.0, format!( "event time should be before t1, t={}",  last.t));

    });
}

fn event_refuses_non_finite_gap(ph: &mut Probe) {
    ph.case("event_refuses_non_finite_gap", |ph| {
    // NaN gaps make `g0 * g1 > 0` false, so the locator treated a blow-up
    // as a bracketed crossing and bisected garbage.
    let package = decay_package();
    let declaration = &package.declarations[0];
    let mut state = BTreeMap::new();
    state.insert("x".to_string(), Value::F64(f64::NAN));
    let error = simulate_continuous_with(
        &package,
        declaration,
        &BTreeMap::new(),
        &state,
        0.0,
        1.0,
        0.1,
        StepMethod::Euler,
        &SimulateOptions {
            atol: None,
            rtol: None,
            dt_max: None,
            event: Some(("x".to_string(), 0.0)),
        },
    )
    .unwrap_err();
    ph.demand("1", 
        error.contains("non-finite"), format!(
        "expected non-finite event refusal, got: {error}"
    ));

    });
}

#[test]
fn stepping_contracts() {
    let mut ph = Probe::new("explicit Euler/RK4 step and simulate trajectories with typed refusals; adaptive RK45 and event handling under certified tolerances");
    euler_decay_one_step(&mut ph);
    rk4_decay_beats_euler(&mut ph);
    missing_rate_is_refused(&mut ph);
    non_positive_dt_is_refused(&mut ph);
    simulate_decay_includes_endpoints(&mut ph);
    simulate_zero_span_returns_initial_sample(&mut ph);
    rk45_decay_is_finite(&mut ph);
    vector_state_euler_steps_componentwise(&mut ph);
    adaptive_decay_uses_fewer_steps_than_tiny_fixed(&mut ph);
    non_positive_atol_is_refused(&mut ph);
    adaptive_refuses_nan_initial_state(&mut ph);
    event_stops_when_x_crosses_half(&mut ph);
    event_refuses_non_finite_gap(&mut ph);
    ph.finish();
}
