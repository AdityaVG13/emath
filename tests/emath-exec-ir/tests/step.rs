//! Explicit Euler / RK4 on admitted `der_<state>` rates.

use emath_core::{QualifiedName, Span};
use emath_exec_ir::interp::Value;
use emath_exec_ir::{simulate_continuous, step_continuous, step_continuous_values, StepMethod};
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
    let rk4 = step_continuous(&package, declaration, &inputs, &state, 0.5, StepMethod::Rk4)
        .unwrap();
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
