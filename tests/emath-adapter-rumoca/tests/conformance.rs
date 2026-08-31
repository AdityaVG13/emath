//! MSL ladder honesty: the adapter's conformance ladder runs against a
//! synthetic production seam and never claims upstream conformance
//! (DISC-004, ACCEPTED under `tests/conformance/DISCREPANCIES.md`).
//!
//! The SimulationReference tier is genuinely unresolved: no upstream
//! Rumoca engine and no MSL corpus gate exist, so the level-5 row must
//! stay an ExpectedFailure bound to `DISC-004` — never silently
//! admitted to the ladder, never claimed as a passing gate.

use emath_adapter_dew_tests::TestResult;
use emath_adapter_rumoca::conformance::{FeatureStatus, Tier, evaluate_msl};
use emath_adapter_rumoca::structural::{
    Component, ComponentKind, Dimensions, StructuralModel, Unit, VariableDecl, VariableKind,
};

/// Minimal valid synthetic model: one model component and one state
/// variable, no equations/connections/initial conditions, so the
/// deterministic ladder runs without manufactured failures.
fn synthetic_model() -> StructuralModel {
    StructuralModel {
        components: vec![Component {
            name: "sys".into(),
            kind: ComponentKind::Model,
        }],
        variables: vec![VariableDecl {
            name: "x".into(),
            kind: VariableKind::State,
            unit: Unit::new("m".into(), Dimensions::meters()),
            ty: emath_ir::TypeNode::Float64,
        }],
        equations: Vec::new(),
        connections: Vec::new(),
        initial_conditions: Vec::new(),
        events: Vec::new(),
    }
}

/// Honest ladder verdict: the SimulationReference row may only Pass
/// behind a real upstream MSL corpus; until then it is an
/// ExpectedFailure bound to DISC-004. A row admitted without the
/// corpus, or failing once admitted, is a Fail.
fn ladder_verdict(report: &emath_adapter_rumoca::conformance::ConformanceReport) -> TestResult {
    match report.status_of("simulation-reference") {
        None => TestResult::ExpectedFailure {
            discrepancy_id: "DISC-004".into(),
        },
        Some(FeatureStatus::Pass) => TestResult::Pass,
        Some(_) => TestResult::Fail,
    }
}

/// The ladder executes against a synthetic model with no Fail rows:
/// tiers 1-4 run on the production seam, and causal completion stays
/// Skipped without a plan (never a fabricated Pass).
#[test]
fn msl_ladder_runs_on_synthetic_model_without_fail_rows() {
    let report = evaluate_msl(&synthetic_model(), None);
    assert_eq!(report.tier, Tier::FlattenedEquations);
    assert!(
        report
            .results
            .iter()
            .all(|row| row.status != FeatureStatus::Fail),
        "synthetic model must produce no Fail rows: {}",
        report.canonical()
    );
    assert_eq!(
        report.status_of("causal-completion"),
        Some(FeatureStatus::Skipped),
        "structural-analysis tier must be Skipped without a plan"
    );
}

/// The SimulationReference tier stays an ExpectedFailure bound to
/// DISC-004: `evaluate_msl` must never admit the level-5 row without
/// an upstream MSL corpus, and once admitted the row is
/// failing-capable (a wrong reference makes it Fail).
#[test]
fn simulation_reference_tier_stays_expected_failure_bound_to_disc_004() {
    let report = evaluate_msl(&synthetic_model(), None);
    assert_eq!(
        report.status_of("simulation-reference"),
        None,
        "tier-5 must not be admitted to the ladder without an upstream MSL corpus"
    );
    assert_eq!(
        ladder_verdict(&report),
        TestResult::ExpectedFailure {
            discrepancy_id: "DISC-004".into(),
        },
        "DISC-004 (no upstream engine) must keep the level-5 row an expected failure"
    );
}
