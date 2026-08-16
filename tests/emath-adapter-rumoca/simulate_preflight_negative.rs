#![forbid(unsafe_code)]
//! Negative tests: `simulate` fails closed under `E-PROV-235` on
//! malformed plan shapes (fewer than two states, invalid `dt`) instead
//! of indexing past the end of `plan.states` (bug-hunt residual).

use std::collections::BTreeMap;

use emath_adapter_rumoca::lower::lower;
use emath_adapter_rumoca::provider::{simulate, SimulationConfig};
use emath_adapter_rumoca::structural::{
    EqExpr, Equation, StructuralModel, Unit, VariableDecl, VariableKind,
};
use emath_ir::TypeNode;
use emath_runtime::{Budget, Outcome};

fn one_state_model() -> StructuralModel {
    StructuralModel {
        variables: vec![VariableDecl {
            name: "x".into(),
            kind: VariableKind::State,
            unit: Unit::dimensionless(),
            ty: TypeNode::Float64,
        }],
        equations: vec![Equation {
            lhs: EqExpr::Der("x".into()),
            rhs: EqExpr::constant(0.0),
            origin: "fixture".into(),
        }],
        ..StructuralModel::default()
    }
}

fn two_state_model() -> StructuralModel {
    StructuralModel {
        variables: vec![
            VariableDecl {
                name: "x".into(),
                kind: VariableKind::State,
                unit: Unit::dimensionless(),
                ty: TypeNode::Float64,
            },
            VariableDecl {
                name: "y".into(),
                kind: VariableKind::State,
                unit: Unit::dimensionless(),
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
fn one_state_plan_fails_closed_not_panics() {
    let model = one_state_model();
    let plan = lower(&model).expect("one-state model lowers");
    assert_eq!(plan.states.len(), 1);
    let outcome = simulate(
        &model,
        &plan,
        &BTreeMap::new(),
        &SimulationConfig {
            steps: 5,
            ..SimulationConfig::default()
        },
        &Budget::default(),
    );
    assert!(matches!(outcome, Outcome::Failed(_)));
}

#[test]
fn non_finite_dt_fails_closed() {
    let model = two_state_model();
    let plan = lower(&model).expect("two-state model lowers");
    let outcome = simulate(
        &model,
        &plan,
        &BTreeMap::new(),
        &SimulationConfig {
            dt: f64::NAN,
            steps: 5,
            error_estimate: false,
        },
        &Budget::default(),
    );
    assert!(matches!(outcome, Outcome::Failed(_)));
}

#[test]
fn well_formed_plan_still_simulates() {
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
        &Budget::default(),
    );
    assert!(outcome.is_resolved());
}
