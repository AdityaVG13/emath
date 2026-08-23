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
fn scalar_rate_residual_spelling_is_refused_with_guidance() {
    // The surface grammar parses `m * derivative(v) + v` as
    // `m * derivative(v + v)` (the derivative keyword greedily consumes
    // the additive tail), so a scalar implicit rate cannot be spelled as
    // a residual tail. The refusal names the boundary: only plain
    // `der(state)` on a state field is admitted inside residuals. The
    // non-scalar mass form (`M * der(v) == f`) is the admitted implicit
    // rate spelling (see `matrix_mass_residual_simulates_like_scalar`).
    let source = "\
emath model ResidualDecay:
    inputs:
        m: Float64
    state:
        v: Float64
    equations:
        0 = m * derivative(v) + v
";
    let result = check_source("residual-decay", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-TYPE-010"),
        "complex derivative residual must be E-TYPE-010, got {codes:?}"
    );
}

#[test]
fn causalized_implicit_dae_admits_and_simulates() {
    // Full causalization: the current I is declared in `algebraic:` and
    // found by the coupled Newton solve at each step — no manual
    // `solve(...) wrt I` wrapping.
    let source = "\
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
    let result = check_source("causal-circuit", source);
    assert!(
        !result.diagnostics.has_errors(),
        "causalized implicit DAE must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let residuals = result
        .package
        .residuals
        .get(&result.package.declarations[0].id)
        .expect("residual must be recorded");
    assert_eq!(residuals[0].algebraic, vec!["I"]);
    assert!(residuals[0].rates.is_empty());
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
        StepMethod::Rk4,
    )
    .unwrap();
    let q_final = match traj.samples.last().unwrap().state.get("q") {
        Some(Value::F64(v)) => *v,
        other => panic!("{other:?}"),
    };
    let expected = 10.0 * (1.0 - (-1.0f64).exp());
    assert!(
        (q_final - expected).abs() < 0.01,
        "causal RC q(1) should be ~{expected:.4}, got {q_final:.4}"
    );
}

#[test]
fn coupled_algebraic_system_solves_together() {
    // Two residuals, two unknowns: Newton solves the coupled system
    // a=6, b=4 at every step; the rate uses the solved algebraic value.
    let source = "\
emath model CoupledSys:
    algebraic:
        a: Float64
        b: Float64
    state:
        q: Float64
    equations:
        a + b == 10
        a - b - 2 == 0
        der(q) = a + b - 4
";
    let result = check_source("coupled", source);
    assert!(
        !result.diagnostics.has_errors(),
        "coupled algebraic system must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let mut inputs = BTreeMap::new();
    inputs.insert("a".into(), Value::F64(0.0));
    inputs.insert("b".into(), Value::F64(0.0));
    let mut state = BTreeMap::new();
    state.insert("q".into(), Value::F64(0.0));
    let traj = simulate_continuous(
        &result.package,
        &result.package.declarations[0],
        &inputs,
        &state,
        0.0,
        0.1,
        0.1,
        StepMethod::Euler,
    )
    .unwrap();
    let q_final = match traj.samples.last().unwrap().state.get("q") {
        Some(Value::F64(v)) => *v,
        other => panic!("{other:?}"),
    };
    assert!(
        (q_final - 0.6).abs() < 1e-9,
        "a=6, b=4 → der(q)=6, so q(0.1)=0.6, got {q_final}"
    );
}

#[test]
fn matrix_mass_residual_simulates_like_scalar() {
    // Non-scalar mass: `M * der(v) == f` with a 2x2 matrix cannot be
    // rewritten to `der(v) = f / M`. Causalization keeps it as a vector
    // residual over the vector rate unknown der(v); Newton solves
    // M * u = f each step.
    let source = "\
emath model MatrixMass:
    inputs:
        M: Matrix[2, 2]
        f: Vector[2]
    state:
        x: Vector[2]
        v: Vector[2]
    equations:
        der(x) = v
        M * der(v) == f
";
    let result = check_source("matrix-mass", source);
    assert!(
        !result.diagnostics.has_errors(),
        "matrix-mass residual must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let residuals = result
        .package
        .residuals
        .get(&result.package.declarations[0].id)
        .expect("residual must be recorded");
    assert_eq!(residuals[0].components, 2);
    assert_eq!(residuals[0].rates, vec!["v"]);
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "M".into(),
        Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![2.0, 0.0, 0.0, 2.0],
        },
    );
    inputs.insert("f".into(), Value::Vector(vec![1.0, 2.0]));
    let mut state = BTreeMap::new();
    state.insert("x".into(), Value::Vector(vec![0.0, 0.0]));
    state.insert("v".into(), Value::Vector(vec![0.0, 0.0]));
    let traj = simulate_continuous(
        &result.package,
        &result.package.declarations[0],
        &inputs,
        &state,
        0.0,
        0.5,
        0.05,
        StepMethod::Rk4,
    )
    .unwrap();
    let last = traj.samples.last().unwrap();
    let vf = match last.state.get("v") {
        Some(Value::Vector(v)) => v.clone(),
        other => panic!("{other:?}"),
    };
    // M = diag(2), so der(v) = f / 2 = [0.5, 1.0]; v(0.5) = [0.25, 0.5].
    assert!((vf[0] - 0.25).abs() < 1e-9, "v[0] should be 0.25, got {}", vf[0]);
    assert!((vf[1] - 0.5).abs() < 1e-9, "v[1] should be 0.5, got {}", vf[1]);
    let xf = match last.state.get("x") {
        Some(Value::Vector(x)) => x.clone(),
        other => panic!("{other:?}"),
    };
    // x(t) = v0*t + a*t^2/2 → x(0.5) = [0.5,1] * 0.125 = [0.0625, 0.125].
    assert!((xf[0] - 0.0625).abs() < 1e-9, "x[0] should be 0.0625, got {}", xf[0]);
    assert!((xf[1] - 0.125).abs() < 1e-9, "x[1] should be 0.125, got {}", xf[1]);
}

#[test]
fn residual_without_unknowns_is_refused() {
    // An implicit residual whose variables are all parameters or state —
    // no `algebraic:` variable, no implicit rate — still refuses.
    let source = "\
emath model NoUnknowns:
    inputs:
        V: Float64
        R: Float64
    state:
        q: Float64
    equations:
        V * R - q == 0
";
    let result = check_source("no-unknowns", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-TYPE-010"),
        "residual without unknowns must be E-TYPE-010, got {codes:?}"
    );
}

#[test]
fn non_square_residual_system_is_refused() {
    let source = "\
emath model Underdetermined:
    algebraic:
        a: Float64
        b: Float64
    state:
        q: Float64
    equations:
        a + b == 10
        der(q) = 0
";
    let result = check_source("underdetermined", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-TYPE-010"),
        "non-square residual system must be E-TYPE-010, got {codes:?}"
    );
}

#[test]
fn unused_algebraic_variable_is_refused() {
    let source = "\
emath model UnusedAlgebraic:
    algebraic:
        a: Float64
        b: Float64
    state:
        q: Float64
    equations:
        a == 5
        der(q) = 0
";
    let result = check_source("unused-algebraic", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-TYPE-002"),
        "unused algebraic variable must be E-TYPE-002, got {codes:?}"
    );
}

#[test]
fn algebraic_without_residual_is_refused() {
    let source = "\
emath model BareAlgebraic:
    algebraic:
        I: Float64
    state:
        q: Float64
    equations:
        der(q) = I
";
    let result = check_source("bare-algebraic", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-TYPE-010"),
        "algebraic without residual must be E-TYPE-010, got {codes:?}"
    );
}

#[test]
fn explicit_rate_residual_conflict_is_refused() {
    let source = "\
emath model RateConflict:
    inputs:
        m: Float64
    state:
        v: Float64
    equations:
        der(v) = -v
        0 = m * derivative(v) + v
";
    let result = check_source("rate-conflict", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-TYPE-010"),
        "residual referencing an explicitly defined rate must be E-TYPE-010, got {codes:?}"
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

#[test]
fn heat_rod_model_simulates_and_conserves_total_heat() {
    // 1D heat equation as a continuous model: der(u) = alpha * laplacian(u, dx)
    // with an insulated (Clamp) boundary. The runner integrates the vector-
    // valued state with RK4. Total heat sum(u) is conserved (the Clamp
    // laplacian sums to zero), and an initial hot spot diffuses to its
    // neighbors.
    let result = check_source(
        "heat-rod-sim",
        include_str!("../../../language/examples/numerical/heat-rod-sim.emath"),
    );
    assert!(
        !result.diagnostics.has_errors(),
        "heat-rod model must admit, got: {:?}",
        result.diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let mut inputs = BTreeMap::new();
    inputs.insert("alpha".into(), Value::F64(1.0));
    let mut state = BTreeMap::new();
    // Hot spot at index 1; total heat = 1.0.
    state.insert("u".into(), Value::Vector(vec![0.0, 1.0, 0.0, 0.0, 0.0]));
    let traj = simulate_continuous(
        &result.package,
        &result.package.declarations[0],
        &inputs,
        &state,
        0.0,
        0.5,
        0.01,
        StepMethod::Rk4,
    )
    .expect("heat-rod simulation should not fault");
    let final_u = match traj.samples.last().unwrap().state.get("u") {
        Some(Value::Vector(v)) => v.clone(),
        other => panic!("expected Vector state `u`, got {other:?}"),
    };
    // Insulated boundary: total heat is conserved.
    let total: f64 = final_u.iter().sum();
    assert!(
        (total - 1.0).abs() < 1e-9,
        "total heat should be conserved at 1.0, got {total}"
    );
    // The hot spot diffuses: the peak drops and heat reaches both neighbors.
    assert!(
        final_u[1] < 1.0,
        "hot spot should diffuse down, got u[1] = {}",
        final_u[1]
    );
    assert!(
        final_u[0] > 0.0,
        "heat should reach the left neighbor, got u[0] = {}",
        final_u[0]
    );
    assert!(
        final_u[2] > 0.0,
        "heat should reach the right neighbor, got u[2] = {}",
        final_u[2]
    );
}

#[test]
fn heat_plate_model_simulates_and_conserves_total_heat() {
    // 2D heat equation as a continuous model:
    //   der(u) = alpha * laplacian_2d(u, dx)
    // with an insulated (Clamp) boundary. The runner integrates the
    // matrix-valued state with RK4. Total heat sum(u) is conserved (the
    // 5-point Clamp laplacian sums to zero), and an initial hot spot at
    // the center diffuses to its four neighbors.
    let result = check_source(
        "heat-plate-sim",
        include_str!("../../../language/examples/numerical/heat-plate-sim.emath"),
    );
    assert!(
        !result.diagnostics.has_errors(),
        "heat-plate model must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let mut inputs = BTreeMap::new();
    inputs.insert("alpha".into(), Value::F64(1.0));
    let mut state = BTreeMap::new();
    // Hot spot at the center cell; total heat = 1.0.
    state.insert(
        "u".into(),
        Value::Matrix {
            rows: 3,
            cols: 3,
            data: vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
        },
    );
    let traj = simulate_continuous(
        &result.package,
        &result.package.declarations[0],
        &inputs,
        &state,
        0.0,
        0.5,
        0.01,
        StepMethod::Rk4,
    )
    .expect("heat-plate simulation should not fault");
    let final_u = match traj.samples.last().unwrap().state.get("u") {
        Some(Value::Matrix { data, .. }) => data.clone(),
        other => panic!("expected Matrix state `u`, got {other:?}"),
    };
    // Insulated boundary: total heat is conserved.
    let total: f64 = final_u.iter().sum();
    assert!(
        (total - 1.0).abs() < 1e-9,
        "total heat should be conserved at 1.0, got {total}"
    );
    // The hot spot diffuses: the center drops and heat reaches all four
    // neighbors (up=1, left=3, right=5, down=7 in row-major order).
    assert!(
        final_u[4] < 1.0,
        "center hot spot should diffuse down, got u[4] = {}",
        final_u[4]
    );
    assert!(final_u[1] > 0.0, "heat should reach the top neighbor");
    assert!(final_u[3] > 0.0, "heat should reach the left neighbor");
    assert!(final_u[5] > 0.0, "heat should reach the right neighbor");
    assert!(final_u[7] > 0.0, "heat should reach the bottom neighbor");
}
