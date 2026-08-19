//! DAE-plan and simulation provider tests.
//!
//! Moved from #[cfg(test)] in crates/emath-adapter-rumoca/src/provider.rs.

use std::collections::BTreeMap;

use emath_adapter_rumoca::lower::lower;
use emath_adapter_rumoca::{
    EqExpr, Equation, SimulationConfig, StructuralModel, Unit, VariableDecl, VariableKind, simulate,
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
