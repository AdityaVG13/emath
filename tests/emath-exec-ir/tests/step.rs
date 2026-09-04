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

#[test]
fn euler_decay_one_step() {
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
    assert_eq!(next.get("x").copied(), Some(0.9));
}

#[test]
fn rk4_decay_beats_euler() {
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
    assert!(rk4_err < euler_err, "rk4={rk4_err} euler={euler_err}");
}

#[test]
fn missing_rate_is_refused() {
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
    assert!(error.contains("der_x"), "{error}");
}

#[test]
fn non_positive_dt_is_refused() {
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
    assert!(error.contains("step size"), "{error}");
}

#[test]
fn simulate_decay_includes_endpoints() {
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
    assert_eq!(trajectory.samples.len(), 3);
    assert_eq!(trajectory.samples[0].t, 0.0);
    assert_eq!(trajectory.samples[2].t, 0.2);
    assert_eq!(
        trajectory.samples[2].state.get("x"),
        Some(&Value::F64(0.81))
    );
}

#[test]
fn simulate_zero_span_returns_initial_sample() {
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
    assert_eq!(trajectory.samples.len(), 1);
    assert_eq!(trajectory.samples[0].t, 0.5);
    assert_eq!(
        trajectory.samples[0].state.get("x"),
        Some(&Value::F64(1.0)),
        "0-step simulate must keep the initial state"
    );
}

#[test]
fn rk45_decay_is_finite() {
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
    assert!(next["x"].is_finite());
    assert!(next["x"] > 0.0);
}

#[test]
fn vector_state_euler_steps_componentwise() {
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
    assert_eq!(next.get("x"), Some(&Value::Vector(vec![1.5, 3.0])));
}

#[test]
fn adaptive_decay_uses_fewer_steps_than_tiny_fixed() {
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
    assert!(
        adaptive.samples.len() < fixed.samples.len(),
        "adaptive={} fixed={}",
        adaptive.samples.len(),
        fixed.samples.len()
    );
    let last = match adaptive.samples.last().unwrap().state.get("x") {
        Some(Value::F64(value)) => *value,
        other => panic!("expected scalar x, got {other:?}"),
    };
    assert!((last - (-1.0_f64).exp()).abs() < 1e-4, "last={last}");
}

#[test]
fn non_positive_atol_is_refused() {
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
    assert!(error.contains("atol"), "{error}");
}

#[test]
fn adaptive_refuses_nan_initial_state() {
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
    assert!(
        error.contains("non-finite"),
        "expected non-finite refusal, got: {error}"
    );
}

#[test]
fn event_stops_when_x_crosses_half() {
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
    assert!((x - 0.5).abs() < 1e-6, "x={x} t={}", last.t);
    assert!(last.t < 1.0, "event time should be before t1, t={}", last.t);
}

#[test]
fn event_refuses_non_finite_gap() {
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
    assert!(
        error.contains("non-finite"),
        "expected non-finite event refusal, got: {error}"
    );
}
