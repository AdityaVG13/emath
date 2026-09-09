//! Negative tests: `simulate` fails closed under `E-PROV-235` on malformed plan shapes instead of indexing past `plan.states`.
use std::collections::BTreeMap;
use emath_adapter_rumoca::lower::lower;
use emath_adapter_rumoca::provider::{SimulationConfig, simulate};
use emath_adapter_rumoca::structural::{EqExpr, Equation, StructuralModel, Unit, VariableDecl, VariableKind};
use emath_ir::TypeNode;
use emath_provider_api::runtime::{Budget, Outcome};
use emath_test_harness::Probe;

fn state_model(n: usize) -> StructuralModel {
    let names = ["x", "y"];
    StructuralModel {
        variables: (0..n).map(|i| VariableDecl { name: names[i].into(), kind: VariableKind::State, unit: Unit::dimensionless(), ty: TypeNode::Float64 }).collect(),
        equations: (0..n).map(|i| Equation { lhs: EqExpr::Der(names[i].into()), rhs: EqExpr::constant(0.0), origin: "fixture".into() }).collect(),
        ..StructuralModel::default()
    }
}

#[test]
fn probe() {
    let mut p = Probe::new("rumoca simulate fails closed on malformed plans and still resolves well-formed ones");
    p.case("one-state", |p| {
        let model = state_model(1);
        let plan = lower(&model).expect("one-state lowers");
        p.eq("states", plan.states.len(), 1);
        p.demand("refused", matches!(simulate(&model, &plan, &BTreeMap::new(), &SimulationConfig { steps: 5, ..SimulationConfig::default() }, &Budget::default()), Outcome::Failed(_)), "one-state must fail closed");
    });
    p.case("non-finite-dt", |p| {
        let model = state_model(2);
        let plan = lower(&model).expect("two-state lowers");
        p.demand("refused", matches!(simulate(&model, &plan, &BTreeMap::new(), &SimulationConfig { dt: f64::NAN, steps: 5, error_estimate: false }, &Budget::default()), Outcome::Failed(_)), "NaN dt must fail closed");
    });
    p.case("well-formed", |p| {
        let model = state_model(2);
        let plan = lower(&model).expect("two-state lowers");
        p.demand("resolved", simulate(&model, &plan, &BTreeMap::new(), &SimulationConfig { dt: 0.001, steps: 5, error_estimate: false }, &Budget::default()).is_resolved(), "well-formed must resolve");
    });
    p.finish();
}
