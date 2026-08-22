//! Continuous `emath model` admission: explicit `derivative(state) = rhs`.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::{simulate_continuous, StepMethod};
use emath_ir::ExprNode;
use emath_sema::admit::CheckResult;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;
use std::collections::BTreeMap;

fn check_source(name: &str, source: &str) -> CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

fn decay_model() -> &'static str {
    "\
emath model Decay:
    inputs:
        k: Float64
    state:
        x: Float64
    equations:
        derivative(x) = -k * x
"
}

#[test]
fn model_derivative_equation_admits_as_rate() {
    let result = check_source("decay", decay_model());
    assert!(
        !result.diagnostics.has_errors(),
        "explicit ODE model must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    assert_eq!(decl.kind_label, "model");
    assert_eq!(decl.state.len(), 1);
    assert!(decl.definitions.contains_key("der_x"));
    let rate = decl.definitions["der_x"];
    assert!(matches!(
        result.package.expr(rate),
        Some(ExprNode::Binary { .. })
    ));
}

#[test]
fn model_der_call_and_wrt_time_admit() {
    let source = "\
emath model Spring:
    inputs:
        m: Float64
        c: Float64
        k: Float64
    state:
        x: Float64
        v: Float64
    equations:
        der(x) = v
        derivative v wrt t = (-c * v - k * x) / m
";
    let result = check_source("spring", source);
    assert!(
        !result.diagnostics.has_errors(),
        "der/wrt spellings must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    assert!(decl.definitions.contains_key("der_x"));
    assert!(decl.definitions.contains_key("der_v"));
}

#[test]
fn scalar_mass_times_derivative_admits_as_rewrite() {
    let source = "\
emath model MassSpring:
    inputs:
        m: Float64
        c: Float64
        k: Float64
    state:
        x: Float64
        v: Float64
    equations:
        der(x) = v
        m * derivative(v) = -c * v - k * x
";
    let result = check_source("mass-matrix", source);
    assert!(
        !result.diagnostics.has_errors(),
        "named scalar mass-matrix must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    assert!(decl.definitions.contains_key("der_x"));
    assert!(decl.definitions.contains_key("der_v"));
    let rate = decl.definitions["der_v"];
    assert!(matches!(
        result.package.expr(rate),
        Some(ExprNode::Binary { .. })
    ));
}

#[test]
fn mass_matrix_spring_matches_explicit() {
    let implicit = check_source(
        "mass-sim",
        "\
emath model MassSpring:
    inputs:
        m: Float64
        c: Float64
        k: Float64
    state:
        x: Float64
        v: Float64
    equations:
        der(x) = v
        m * der(v) = -c * v - k * x
",
    );
    let explicit = check_source(
        "explicit-sim",
        include_str!("../../../language/examples/numerical/explicit-mass-spring.emath"),
    );
    assert!(!implicit.diagnostics.has_errors());
    assert!(!explicit.diagnostics.has_errors());
    let mut inputs = BTreeMap::new();
    inputs.insert("m".into(), Value::F64(1.0));
    inputs.insert("c".into(), Value::F64(0.2));
    inputs.insert("k".into(), Value::F64(1.0));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::F64(1.0));
    state.insert("v".into(), Value::F64(0.0));
    let left = simulate_continuous(
        &implicit.package,
        &implicit.package.declarations[0],
        &inputs,
        &state,
        0.0,
        0.5,
        0.05,
        StepMethod::Rk4,
    )
    .unwrap();
    let right = simulate_continuous(
        &explicit.package,
        &explicit.package.declarations[0],
        &inputs,
        &state,
        0.0,
        0.5,
        0.05,
        StepMethod::Rk4,
    )
    .unwrap();
    let lx = match left.samples.last().unwrap().state.get("x") {
        Some(Value::F64(value)) => *value,
        other => panic!("{other:?}"),
    };
    let rx = match right.samples.last().unwrap().state.get("x") {
        Some(Value::F64(value)) => *value,
        other => panic!("{other:?}"),
    };
    assert!((lx - rx).abs() < 1e-12, "implicit={lx} explicit={rx}");
}

#[test]
fn leftover_implicit_residual_is_still_refused() {
    let source = "\
emath model Residual:
    inputs:
        m: Float64
    state:
        v: Float64
    equations:
        0 = m * derivative(v) + v
";
    let result = check_source("residual", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-TYPE-010"),
        "leftover implicit residual must stay E-TYPE-010, got {codes:?}"
    );
}

#[test]
fn function_cannot_use_equations() {
    let source = "\
emath function NotAModel:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
    equations:
        derivative(x) = 0
";
    let result = check_source("fn-eq", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-KIND-010"),
        "equations on function must be E-KIND-010, got {codes:?}"
    );
}

#[test]
fn incomplete_rates_are_refused() {
    let source = "\
emath model Incomplete:
    state:
        x: Float64
        v: Float64
    equations:
        der(x) = v
";
    let result = check_source("incomplete", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-NAME-025"),
        "missing der(v) must be E-NAME-025, got {codes:?}"
    );
}

#[test]
fn explicit_mass_spring_example_admits() {
    let source = include_str!(
        "../../../language/examples/numerical/explicit-mass-spring.emath"
    );
    let result = check_source("explicit-mass-spring", source);
    assert!(
        !result.diagnostics.has_errors(),
        "A2 mass-spring example must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    assert_eq!(decl.kind_label, "model");
    assert!(decl.definitions.contains_key("der_x"));
    assert!(decl.definitions.contains_key("der_v"));
}

#[test]
fn unit_rates_admit_when_state_is_quantity() {
    let source = "\
emath model UnitSpring:
    inputs:
        v: Float64 in m/s
    state:
        x: Float64 in m
    equations:
        der(x) = v
";
    let result = check_source("unit-rates", source);
    assert!(
        !result.diagnostics.has_errors(),
        "quantity state with matching rate unit must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn unit_rate_mismatch_is_refused() {
    let source = "\
emath model BadUnits:
    inputs:
        v: Float64 in m
    state:
        x: Float64 in m
    equations:
        der(x) = v
";
    let result = check_source("unit-mismatch", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-UNIT-101"),
        "rate unit mismatch must be E-UNIT-101, got {codes:?}"
    );
}

#[test]
fn empty_model_is_refused() {
    let source = "\
emath model Empty:
    inputs:
        x: Float64
";
    let result = check_source("empty-model", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-KIND-011"),
        "empty model must be E-KIND-011, got {codes:?}"
    );
}

#[test]
fn algebraic_definition_in_equations_admits() {
    let source = "\
emath model RCCircuit:
    inputs:
        V: Float64
        R: Float64
        C: Float64
    state:
        q: Float64
    equations:
        I = (V - q / C) / R
        der(q) = I
";
    let result = check_source("rc-circuit", source);
    assert!(
        !result.diagnostics.has_errors(),
        "algebraic definition in equations must admit, got: {:?}",
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
    );
    let decl = &result.package.declarations[0];
    assert!(decl.definitions.contains_key("I"), "algebraic var I must be in definitions");
    assert!(decl.definitions.contains_key("der_q"), "rate der_q must be in definitions");
}

#[test]
fn algebraic_dae_simulates_correctly() {
    let source = "\
emath model RCCircuit:
    inputs:
        V: Float64
        R: Float64
        C: Float64
    state:
        q: Float64
    equations:
        I = (V - q / C) / R
        der(q) = I
";
    let result = check_source("rc-sim", source);
    assert!(!result.diagnostics.has_errors());
    let mut inputs = BTreeMap::new();
    inputs.insert("V".into(), Value::F64(10.0));
    inputs.insert("R".into(), Value::F64(1.0));
    inputs.insert("C".into(), Value::F64(1.0));
    let mut state = BTreeMap::new();
    state.insert("q".into(), Value::F64(0.0));
    let traj = simulate_continuous(
        &result.package,
        &result.package.declarations[0],
        &inputs,
        &state,
        0.0,
        1.0,
        0.01,
        StepMethod::Rk4,
    )
    .unwrap();
    let q_final = match traj.samples.last().unwrap().state.get("q") {
        Some(Value::F64(v)) => *v,
        other => panic!("{other:?}"),
    };
    // Analytical: q(t) = C*V*(1 - exp(-t/(R*C))) = 10*(1 - exp(-1))
    let expected = 10.0 * (1.0 - (-1.0f64).exp());
    assert!(
        (q_final - expected).abs() < 0.01,
        "RC circuit q(1) should be ~{expected:.4}, got {q_final:.4}"
    );
}

#[test]
fn implicit_dae_with_solve_admits_and_simulates() {
    // Implicit DAE: current I is found via Newton's method (solve op)
    // at each time step. I is declared as an input (initial guess).
    let source = "\
emath model ImplicitCircuit:
    inputs:
        V: Float64
        R: Float64
        C: Float64
        I: Float64
    state:
        q: Float64
    equations:
        I_solved = solve(V - R * I - q / C) wrt I
        der(q) = I_solved
";
    let result = check_source("implicit-circuit", source);
    assert!(
        !result.diagnostics.has_errors(),
        "implicit DAE with solve must admit, got: {:?}",
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
    );
    let mut inputs = BTreeMap::new();
    inputs.insert("V".into(), Value::F64(10.0));
    inputs.insert("R".into(), Value::F64(1.0));
    inputs.insert("C".into(), Value::F64(1.0));
    inputs.insert("I".into(), Value::F64(1.0)); // initial guess
    let mut state = BTreeMap::new();
    state.insert("q".into(), Value::F64(0.0));
    let traj = simulate_continuous(
        &result.package,
        &result.package.declarations[0],
        &inputs,
        &state,
        0.0,
        1.0,
        0.01,
        StepMethod::Euler, // Euler for predictable Newton convergence
    )
    .unwrap();
    let q_final = match traj.samples.last().unwrap().state.get("q") {
        Some(Value::F64(v)) => *v,
        other => panic!("{other:?}"),
    };
    // Same analytical solution as the semi-explicit case:
    // q(t) = C*V*(1 - exp(-t/(R*C))) = 10*(1 - exp(-1))
    let expected = 10.0 * (1.0 - (-1.0f64).exp());
    assert!(
        (q_final - expected).abs() < 0.05,
        "implicit RC circuit q(1) should be ~{expected:.4}, got {q_final:.4}"
    );
}
