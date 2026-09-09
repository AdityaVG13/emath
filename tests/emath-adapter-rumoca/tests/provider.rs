//! DAE-plan and simulation provider tests.
use std::collections::BTreeMap;
use emath_adapter_rumoca::lower::lower;
use emath_adapter_rumoca::{Dimensions, EqExpr, Equation, InitialCondition, SimulationArtifact, SimulationConfig, StructuralModel, Unit, VariableDecl, VariableKind, build_simulation_artifact, simulate};
use emath_ir::TypeNode;
use emath_provider_api::runtime::{Budget, Outcome, UnresolvedReason};
use emath_test_harness::Probe;

fn two_state_model() -> StructuralModel {
    StructuralModel {
        variables: vec![
            VariableDecl { name: "x".into(), kind: VariableKind::State, unit: Unit::seconds(), ty: TypeNode::Float64 },
            VariableDecl { name: "y".into(), kind: VariableKind::State, unit: Unit::seconds(), ty: TypeNode::Float64 },
        ],
        equations: vec![
            Equation { lhs: EqExpr::Der("x".into()), rhs: EqExpr::constant(0.0), origin: "fixture".into() },
            Equation { lhs: EqExpr::Der("y".into()), rhs: EqExpr::constant(0.0), origin: "fixture".into() },
        ],
        ..StructuralModel::default()
    }
}
fn mass_spring_model() -> StructuralModel {
    let s2 = Dimensions::base([0, 0, -2, 0, 0, 0, 0]);
    StructuralModel {
        variables: vec![
            VariableDecl { name: "m".into(), kind: VariableKind::Parameter, unit: Unit::dimensionless(), ty: TypeNode::Float64 },
            VariableDecl { name: "c".into(), kind: VariableKind::Parameter, unit: Unit::new("s^-1".into(), Dimensions::per_second()), ty: TypeNode::Float64 },
            VariableDecl { name: "k".into(), kind: VariableKind::Parameter, unit: Unit::new("s^-2".into(), s2), ty: TypeNode::Float64 },
            VariableDecl { name: "x".into(), kind: VariableKind::State, unit: Unit::seconds(), ty: TypeNode::Float64 },
            VariableDecl { name: "v".into(), kind: VariableKind::State, unit: Unit::dimensionless(), ty: TypeNode::Float64 },
        ],
        equations: vec![
            Equation { lhs: EqExpr::Der("x".into()), rhs: EqExpr::Var("v".into()), origin: "MassSpring:der(x)".into() },
            Equation {
                lhs: EqExpr::Der("v".into()),
                rhs: EqExpr::Div(
                    Box::new(EqExpr::Sub(
                        Box::new(EqExpr::Neg(Box::new(EqExpr::Mul(Box::new(EqExpr::Var("k".into())), Box::new(EqExpr::Var("x".into())))))),
                        Box::new(EqExpr::Mul(Box::new(EqExpr::Var("c".into())), Box::new(EqExpr::Var("v".into())))),
                    )),
                    Box::new(EqExpr::Var("m".into())),
                ),
                origin: "MassSpring:der(v)".into(),
            },
        ],
        initial_conditions: vec![
            InitialCondition { target: "x".into(), value: EqExpr::constant(1.0) },
            InitialCondition { target: "v".into(), value: EqExpr::constant(0.0) },
        ],
        ..StructuralModel::default()
    }
}
fn resolved(outcome: Outcome<SimulationArtifact, emath_adapter_rumoca::ArtifactError>) -> SimulationArtifact {
    match outcome {
        Outcome::Resolved { value, .. } => value,
        other => panic!("artifact must resolve, got {other:?}"),
    }
}

#[test]
fn probe() {
    let mut p = Probe::new("rumoca provider builds deterministic checked artifacts and fails closed on hostile shapes");
    p.case("mass-spring", |p| {
        let model = mass_spring_model();
        let first = resolved(build_simulation_artifact(&model, &Budget::default()));
        let second = resolved(build_simulation_artifact(&model, &Budget::default()));
        p.eq("deterministic-id", first.content_identity(), second.content_identity());
        p.eq("deterministic-src", first.rust_source.clone(), second.rust_source);
        p.demand("forbid-unsafe", first.rust_source.starts_with("#![forbid(unsafe_code)]"), "must forbid unsafe");
        p.eq("derivatives", first.derivatives.iter().map(|e| (e.state.as_str(), e.equation)).collect::<Vec<_>>(), [("x", 0), ("v", 1)].to_vec());
        let run = first.run(&BTreeMap::from([("m".to_string(), 1.0), ("c".to_string(), 0.0), ("k".to_string(), 1.0)]), &SimulationConfig { dt: 0.0001, steps: 10_000, error_estimate: true }, &Budget::default());
        match run {
            Outcome::Resolved { value, .. } => {

                p.close("position-cos", value.final_position, 1.0_f64.cos(), 0.001);
                p.close("velocity-sin", value.final_velocity, -1.0_f64.sin(), 0.001);
            }
            other => {
                p.fail("mass-spring-run", format!("must run, got {other:?}"));
            }
        }
    });
    p.case("hostile-bounded", |p| {
        let mut model = mass_spring_model();
        model.equations.extend((0..100).map(|i| Equation { lhs: EqExpr::Var(format!("hostile_{i}")), rhs: EqExpr::constant(0.0), origin: "hostile".into() }));
        p.demand("budget", matches!(build_simulation_artifact(&model, &Budget { evaluations: 10, iterations: 10, work_units: 10, ..Budget::default() }), Outcome::Unresolved { reason: UnresolvedReason::BudgetExhausted, .. }), "hostile fan-out must exhaust budget");
        let mut reserved = two_state_model();
        reserved.variables[0].name = "gen".into();
        reserved.equations[0].lhs = EqExpr::Der("gen".into());
        match build_simulation_artifact(&reserved, &Budget::default()) {
            Outcome::Failed(e) => p.eq("keyword/code", e.code, "E-PROV-239"),
            other => p.fail("keyword", format!("Rust keyword must refuse, got {other:?}")),
        };
        let mut deep = two_state_model();
        let mut expr = EqExpr::constant(0.0);
        for _ in 0..100 {
            expr = EqExpr::Neg(Box::new(expr));
        }
        deep.equations[0].rhs = expr;
        match build_simulation_artifact(&deep, &Budget::default()) {
            Outcome::Failed(e) => p.eq("depth/code", e.code, "E-PROV-239"),
            other => p.fail("depth", format!("deep expr must refuse, got {other:?}")),
        };
    });
    p.case("memory-budget", |p| {
        let model = two_state_model();
        let plan = lower(&model).expect("two-state lowers");
        let outcome = simulate(&model, &plan, &BTreeMap::new(), &SimulationConfig { dt: 0.001, steps: 5, error_estimate: false }, &Budget { memory_bytes: 1, ..Budget::default() });
        p.demand("memory", matches!(outcome, Outcome::Unresolved { reason: UnresolvedReason::BudgetExhausted, .. }), format!("tiny memory must refuse, got {outcome:?}"));
    });
    p.case("three-state", |p| {
        let mut model = two_state_model();
        model.variables.push(VariableDecl { name: "z".into(), kind: VariableKind::State, unit: Unit::seconds(), ty: TypeNode::Float64 });
        model.equations.push(Equation { lhs: EqExpr::Der("z".into()), rhs: EqExpr::constant(0.0), origin: "fixture".into() });
        let plan = lower(&model).expect("three-state lowers");
        match simulate(&model, &plan, &BTreeMap::new(), &SimulationConfig { dt: 0.001, steps: 5, error_estimate: false }, &Budget::default()) {
            Outcome::Failed(e) => p.eq("three-state/code", e.code, "E-PROV-238"),
            other => p.fail("three-state", format!("must refuse, got {other:?}")),
        };
    });
    p.finish();
}
