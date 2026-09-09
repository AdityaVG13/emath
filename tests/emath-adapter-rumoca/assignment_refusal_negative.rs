//! Negative witness: `simulate` must fail closed under `E-PROV-236` when a plan assigns a variable outside the state pair.
use std::collections::BTreeMap;
use emath_adapter_rumoca::lower::lower;
use emath_adapter_rumoca::provider::{SimulationConfig, simulate};
use emath_adapter_rumoca::structural::{EqExpr, Equation, StructuralModel, Unit, VariableDecl, VariableKind};
use emath_ir::TypeNode;
use emath_provider_api::runtime::{Budget, Outcome};
use emath_test_harness::Probe;

#[test]
fn probe() {
    let mut p = Probe::new("rumoca simulate fails closed with E-PROV-236 on non-state assignment");
    let model = StructuralModel {
        variables: vec![
            VariableDecl { name: "x".into(), kind: VariableKind::State, unit: Unit::dimensionless(), ty: TypeNode::Float64 },
            VariableDecl { name: "y".into(), kind: VariableKind::State, unit: Unit::dimensionless(), ty: TypeNode::Float64 },
            VariableDecl { name: "z".into(), kind: VariableKind::Output, unit: Unit::dimensionless(), ty: TypeNode::Float64 },
        ],
        equations: vec![
            Equation { lhs: EqExpr::Der("x".into()), rhs: EqExpr::constant(0.0), origin: "fixture".into() },
            Equation { lhs: EqExpr::Der("y".into()), rhs: EqExpr::constant(0.0), origin: "fixture".into() },
            Equation { lhs: EqExpr::Var("z".into()), rhs: EqExpr::constant(1.0), origin: "fixture".into() },
        ],
        ..StructuralModel::default()
    };
    let plan = lower(&model).expect("model lowers");
    match simulate(&model, &plan, &BTreeMap::new(), &SimulationConfig { dt: 0.001, steps: 5, error_estimate: false }, &Budget::default()) {
        Outcome::Failed(e) => p.eq("code", e.code, "E-PROV-236"),
        other => p.fail("refusal", format!("expected E-PROV-236, got {other:?}")),
    }
    p.finish();
}
