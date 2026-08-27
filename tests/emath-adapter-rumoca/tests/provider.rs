//! DAE-plan and simulation provider tests.
//!
//! Moved from #[cfg(test)] in crates/emath-adapter-rumoca/src/provider.rs.

use std::collections::BTreeMap;

use emath_adapter_rumoca::lower::lower;
use emath_adapter_rumoca::{
    Dimensions, EqExpr, Equation, InitialCondition, SimulationArtifact, SimulationConfig,
    StructuralModel, Unit, VariableDecl, VariableKind, build_simulation_artifact, simulate,
};
use emath_ir::TypeNode;
use emath_runtime::{Budget, Outcome, UnresolvedReason};

fn two_state_model() -> StructuralModel {
    StructuralModel {
        variables: vec![
            VariableDecl {
                name: "x".into(),
                kind: VariableKind::State,
                unit: Unit::seconds(),
                ty: TypeNode::Float64,
            },
            VariableDecl {
                name: "y".into(),
                kind: VariableKind::State,
                unit: Unit::seconds(),
                ty: TypeNode::Float64,
            },
        ],
        equations: vec![
            Equation {
                lhs: EqExpr::Der("x".into()),
                rhs: EqExpr::constant(0.0),
                origin: "fixture".into(),
            },
            Equation {
                lhs: EqExpr::Der("y".into()),
                rhs: EqExpr::constant(0.0),
                origin: "fixture".into(),
            },
        ],
        ..StructuralModel::default()
    }
}

fn mass_spring_model() -> StructuralModel {
    let inverse_seconds_squared = Dimensions::base([0, 0, -2, 0, 0, 0, 0]);
    StructuralModel {
        variables: vec![
            VariableDecl {
                name: "m".into(),
                kind: VariableKind::Parameter,
                unit: Unit::dimensionless(),
                ty: TypeNode::Float64,
            },
            VariableDecl {
                name: "c".into(),
                kind: VariableKind::Parameter,
                unit: Unit::new("s^-1".into(), Dimensions::per_second()),
                ty: TypeNode::Float64,
            },
            VariableDecl {
                name: "k".into(),
                kind: VariableKind::Parameter,
                unit: Unit::new("s^-2".into(), inverse_seconds_squared),
                ty: TypeNode::Float64,
            },
            VariableDecl {
                name: "x".into(),
                kind: VariableKind::State,
                unit: Unit::seconds(),
                ty: TypeNode::Float64,
            },
            VariableDecl {
                name: "v".into(),
                kind: VariableKind::State,
                unit: Unit::dimensionless(),
                ty: TypeNode::Float64,
            },
        ],
        equations: vec![
            Equation {
                lhs: EqExpr::Der("x".into()),
                rhs: EqExpr::Var("v".into()),
                origin: "MassSpring:der(x)".into(),
            },
            Equation {
                lhs: EqExpr::Der("v".into()),
                rhs: EqExpr::Div(
                    Box::new(EqExpr::Sub(
                        Box::new(EqExpr::Neg(Box::new(EqExpr::Mul(
                            Box::new(EqExpr::Var("k".into())),
                            Box::new(EqExpr::Var("x".into())),
                        )))),
                        Box::new(EqExpr::Mul(
                            Box::new(EqExpr::Var("c".into())),
                            Box::new(EqExpr::Var("v".into())),
                        )),
                    )),
                    Box::new(EqExpr::Var("m".into())),
                ),
                origin: "MassSpring:der(v)".into(),
            },
        ],
        initial_conditions: vec![
            InitialCondition {
                target: "x".into(),
                value: EqExpr::constant(1.0),
            },
            InitialCondition {
                target: "v".into(),
                value: EqExpr::constant(0.0),
            },
        ],
        ..StructuralModel::default()
    }
}

fn resolved_artifact(
    outcome: Outcome<SimulationArtifact, emath_adapter_rumoca::ArtifactError>,
) -> SimulationArtifact {
    match outcome {
        Outcome::Resolved { value, .. } => value,
        other => panic!("simulation artifact must resolve, got {other:?}"),
    }
}

#[test]
fn mass_spring_artifact_is_deterministic_runnable_and_physically_checked() {
    let model = mass_spring_model();
    let first = resolved_artifact(build_simulation_artifact(&model, &Budget::default()));
    let second = resolved_artifact(build_simulation_artifact(&model, &Budget::default()));
    assert_eq!(first.content_identity(), second.content_identity());
    assert_eq!(first.rust_source, second.rust_source);
    assert!(first.rust_source.starts_with("#![forbid(unsafe_code)]"));
    assert_eq!(
        first
            .derivatives
            .iter()
            .map(|entry| (entry.state.as_str(), entry.equation))
            .collect::<Vec<_>>(),
        [("x", 0), ("v", 1)]
    );

    let parameters = BTreeMap::from([
        ("m".to_string(), 1.0),
        ("c".to_string(), 0.0),
        ("k".to_string(), 1.0),
    ]);
    let run = first.run(
        &parameters,
        &SimulationConfig {
            dt: 0.0001,
            steps: 10_000,
            error_estimate: true,
        },
        &Budget::default(),
    );
    let Outcome::Resolved { value, .. } = run else {
        panic!("mass-spring artifact must run, got {run:?}");
    };
    assert!(
        (value.final_position - 1.0_f64.cos()).abs() < 0.001,
        "Euler provider must agree with the independent analytic solution at t=1: {} vs {}",
        value.final_position,
        1.0_f64.cos()
    );
    assert!(
        (value.final_velocity + 1.0_f64.sin()).abs() < 0.001,
        "velocity must agree with -sin(1): {} vs {}",
        value.final_velocity,
        -1.0_f64.sin()
    );
}

#[test]
fn hostile_artifact_input_is_bounded_before_flattening() {
    let mut model = mass_spring_model();
    model.equations.extend((0..100).map(|index| Equation {
        lhs: EqExpr::Var(format!("hostile_{index}")),
        rhs: EqExpr::constant(0.0),
        origin: "hostile".into(),
    }));
    let outcome = build_simulation_artifact(
        &model,
        &Budget {
            evaluations: 10,
            iterations: 10,
            work_units: 10,
            ..Budget::default()
        },
    );
    assert!(matches!(
        outcome,
        Outcome::Unresolved {
            reason: UnresolvedReason::BudgetExhausted,
            ..
        }
    ));

    let mut reserved_name = two_state_model();
    reserved_name.variables[0].name = "gen".into();
    reserved_name.equations[0].lhs = EqExpr::Der("gen".into());
    match build_simulation_artifact(&reserved_name, &Budget::default()) {
        Outcome::Failed(error) => assert_eq!(error.code, "E-PROV-239"),
        other => panic!("Rust 2024 keyword must refuse before code generation, got {other:?}"),
    }

    let mut deeply_nested = two_state_model();
    let mut expression = EqExpr::constant(0.0);
    for _ in 0..100 {
        expression = EqExpr::Neg(Box::new(expression));
    }
    deeply_nested.equations[0].rhs = expression;
    match build_simulation_artifact(&deeply_nested, &Budget::default()) {
        Outcome::Failed(error) => assert_eq!(error.code, "E-PROV-239"),
        other => panic!("deep expression must refuse before recursive rendering, got {other:?}"),
    }
}

#[test]
fn tiny_memory_budget_refuses_simulation() {
    // Budget.memory_bytes must be consulted: a footprint below the
    // trace size refuses up front instead of running.
    let model = two_state_model();
    let plan = lower(&model).expect("two-state model lowers");
    let outcome = simulate(
        &model,
        &plan,
        &BTreeMap::new(),
        &SimulationConfig {
            dt: 0.001,
            steps: 5,
            error_estimate: false,
        },
        &Budget {
            memory_bytes: 1,
            ..Budget::default()
        },
    );
    assert!(
        matches!(
            outcome,
            Outcome::Unresolved {
                reason: UnresolvedReason::BudgetExhausted,
                ..
            }
        ),
        "a memory footprint below the trace size must refuse, got {outcome:?}"
    );
}

#[test]
fn three_state_plan_refused_by_two_state_recorder() {
    // The fixture-time recorder represents at most two states; a
    // 3-state plan must be refused under E-PROV-238 instead of
    // silently dropping state three from the recorded trajectory.
    let model = StructuralModel {
        variables: vec![
            VariableDecl {
                name: "x".into(),
                kind: VariableKind::State,
                unit: Unit::seconds(),
                ty: TypeNode::Float64,
            },
            VariableDecl {
                name: "y".into(),
                kind: VariableKind::State,
                unit: Unit::seconds(),
                ty: TypeNode::Float64,
            },
            VariableDecl {
                name: "z".into(),
                kind: VariableKind::State,
                unit: Unit::seconds(),
                ty: TypeNode::Float64,
            },
        ],
        equations: vec![
            Equation {
                lhs: EqExpr::Der("x".into()),
                rhs: EqExpr::constant(0.0),
                origin: "fixture".into(),
            },
            Equation {
                lhs: EqExpr::Der("y".into()),
                rhs: EqExpr::constant(0.0),
                origin: "fixture".into(),
            },
            Equation {
                lhs: EqExpr::Der("z".into()),
                rhs: EqExpr::constant(0.0),
                origin: "fixture".into(),
            },
        ],
        ..StructuralModel::default()
    };
    let plan = lower(&model).expect("three-state model lowers");
    let outcome = simulate(
        &model,
        &plan,
        &BTreeMap::new(),
        &SimulationConfig {
            dt: 0.001,
            steps: 5,
            error_estimate: false,
        },
        &Budget::default(),
    );
    match outcome {
        Outcome::Failed(error) => assert_eq!(error.code, "E-PROV-238"),
        other => panic!("three-state plan must be refused, got {other:?}"),
    }
}
