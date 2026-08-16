#![forbid(unsafe_code)]
//! Negative witness: `simulate` must fail closed under `E-PROV-236` when a
//! plan assigns a variable outside the state pair. Previously the
//! assignment was silently dropped and the run still resolved.

use std::collections::BTreeMap;

use emath_adapter_rumoca::lower::lower;
use emath_adapter_rumoca::provider::{simulate, SimulationConfig};
use emath_adapter_rumoca::structural::{
    EqExpr, Equation, StructuralModel, Unit, VariableDecl, VariableKind,
};
use emath_ir::TypeNode;
use emath_runtime::{Budget, Outcome};

fn model_with_output() -> StructuralModel {
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
            VariableDecl {
                name: "z".into(),
                kind: VariableKind::Output,
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
            Equation {
                lhs: EqExpr::Var("z".into()),
                rhs: EqExpr::constant(1.0),
                origin: "fixture".into(),
            },
        ],
        ..StructuralModel::default()
    }
}

#[test]
fn assignment_to_non_state_variable_fails_closed() {
    let model = model_with_output();
    let plan = lower(&model).expect("model lowers");
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
    assert!(
        matches!(outcome, Outcome::Failed(ref e) if e.code == "E-PROV-236"),
        "expected E-PROV-236 failure for non-state assignment"
    );
}
