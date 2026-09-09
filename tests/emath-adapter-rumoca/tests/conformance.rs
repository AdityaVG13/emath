//! MSL ladder honesty: the adapter's conformance ladder runs against a synthetic production seam and never claims upstream conformance.
use emath_adapter_dew_tests::TestResult;
use emath_adapter_rumoca::conformance::{FeatureStatus, Tier, evaluate_msl};
use emath_adapter_rumoca::structural::{Component, ComponentKind, Dimensions, StructuralModel, Unit, VariableDecl, VariableKind};
use emath_test_harness::Probe;

fn synthetic_model() -> StructuralModel {
    StructuralModel {
        components: vec![Component { name: "sys".into(), kind: ComponentKind::Model }],
        variables: vec![VariableDecl { name: "x".into(), kind: VariableKind::State, unit: Unit::new("m".into(), Dimensions::meters()), ty: emath_ir::TypeNode::Float64 }],
        equations: vec![],
        connections: vec![],
        initial_conditions: vec![],
        events: vec![],
    }
}
fn ladder_verdict(report: &emath_adapter_rumoca::conformance::ConformanceReport) -> TestResult {
    match report.status_of("simulation-reference") {
        None => TestResult::ExpectedFailure { discrepancy_id: "DISC-004".into() },
        Some(FeatureStatus::Pass) => TestResult::Pass,
        Some(_) => TestResult::Fail,
    }
}

#[test]
fn probe() {
    let mut p = Probe::new("rumoca MSL ladder runs synthetic tiers without fail and keeps simulation-reference an expected failure");
    let report = evaluate_msl(&synthetic_model(), None);
    p.eq("tier", report.tier, Tier::FlattenedEquations);
    p.demand("no-fail", report.results.iter().all(|r| r.status != FeatureStatus::Fail), format!("no Fail rows: {}", report.canonical()));
    p.eq("causal-skipped", report.status_of("causal-completion"), Some(FeatureStatus::Skipped));
    p.eq("tier5-absent", report.status_of("simulation-reference"), None);
    p.eq("disc-004", ladder_verdict(&report), TestResult::ExpectedFailure { discrepancy_id: "DISC-004".into() });
    p.finish();
}
