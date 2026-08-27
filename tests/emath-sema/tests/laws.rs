//! Executable `emath law` declarations and their metadata boundary.

use emath_core::limits::Limits;
use emath_exec_ir::runner::run_package;
use emath_ir::{EvidenceLevel, GoalId, MeaningError};
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

fn error_codes(result: &emath_sema::admit::CheckResult) -> Vec<&str> {
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn assert_pack(name: &str, source: &str, expected_laws: usize) {
    let result = check(name, source);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.errors().collect::<Vec<_>>()
    );
    assert_eq!(result.package.declarations.len(), expected_laws);
    assert_eq!(result.package.law_metadata.len(), expected_laws);
    let report = run_package(&result.package);
    assert_eq!(report.summary.tests as usize, expected_laws);
    assert_eq!(report.summary.passed as usize, expected_laws);
}

#[test]
fn newton_second_admits_runs_and_preserves_metadata() {
    let result = check(
        "newton-second",
        include_str!("../../../language/examples/physics/newton-second.emath"),
    );
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.errors().collect::<Vec<_>>()
    );

    let declaration = &result.package.declarations[0];
    assert_eq!(declaration.kind.0, "law");
    assert_eq!(declaration.kind_label, "law");
    assert_eq!(declaration.evidence[0].level, EvidenceLevel::E2);

    let metadata = result.package.law_metadata.get(&declaration.id).unwrap();
    assert_eq!(metadata.domain, "classical mechanics");
    assert_eq!(
        metadata.assumptions,
        ["The mass is constant in the chosen inertial frame."]
    );
    assert_eq!(declaration.evidence[0].assumptions, metadata.assumptions);
    assert_eq!(metadata.provenance.len(), 1);
    assert_eq!(metadata.citations.len(), 1);

    let report = run_package(&result.package);
    assert_eq!(report.summary.tests, 1);
    assert_eq!(report.summary.passed, 1);
    assert_eq!(report.declarations[0].law_metadata.as_ref(), Some(metadata));

    let revised_source = include_str!("../../../language/examples/physics/newton-second.emath")
        .replace("classical mechanics", "relativistic mechanics");
    let revised = check("newton-second-revised-domain", &revised_source);
    assert_ne!(
        result.package.identity.as_ref().unwrap().content,
        revised.package.identity.as_ref().unwrap().content,
        "law metadata must participate in package identity"
    );
    assert_eq!(
        result.package.meaning_id(&[]).unwrap(),
        revised.package.meaning_id(&[]).unwrap(),
        "non-authoritative law prose must not change admitted mathematical meaning"
    );
}

#[test]
fn law_requires_assumptions() {
    let source = include_str!("../../../language/examples/physics/newton-second.emath")
        .replace(
            "    assumptions:\n        assume: \"The mass is constant in the chosen inertial frame.\"\n        require mass >= 0 kg\n\n",
            "",
        );
    let result = check("law-without-assumptions", &source);
    assert!(error_codes(&result).contains(&"E-LAW-002"));
}

#[test]
fn law_definition_still_enforces_units() {
    let source = include_str!("../../../language/examples/physics/newton-second.emath")
        .replace("force = mass * acceleration", "force = mass + acceleration");
    let result = check("law-unit-mismatch", &source);
    assert!(error_codes(&result).contains(&"E-UNIT-101"));
}

#[test]
fn law_refuses_unknown_evidence_level() {
    let source = include_str!("../../../language/examples/physics/newton-second.emath")
        .replace("level E2", "level E9");
    let result = check("law-unknown-evidence", &source);
    assert!(error_codes(&result).contains(&"E-EVID-115"));
}

#[test]
fn unresolved_law_package_import_is_explicit() {
    let source = format!(
        "use physics::NewtonSecond\n\n{}",
        include_str!("../../../language/examples/physics/newton-second.emath")
    );
    let result = check("law-import", &source);
    assert!(error_codes(&result).contains(&"E-PKG-052"));
}

#[test]
fn multiple_embedded_law_packages_refuse_explicitly() {
    let result = check(
        "multiple-law-packages",
        "use physics::classical::{NewtonSecond}\nuse analysis::laws::{TaylorQuadratic}\n",
    );
    assert!(error_codes(&result).contains(&"E-PKG-053"));
}

#[test]
fn malformed_law_goal_refuses_meaning_identity() {
    let mut result = check(
        "malformed-law-goal",
        include_str!("../../../language/examples/physics/newton-second.emath"),
    );
    result.package.declarations[0].goals.push(GoalId(u32::MAX));
    assert!(matches!(
        result.package.meaning_id(&[]),
        Err(MeaningError::MissingGoal(GoalId(u32::MAX)))
    ));
}

#[test]
fn classical_law_pack_evaluates_all_listed_symbols() {
    assert_pack(
        "physics-classical",
        include_str!("../../../language/stdlib/laws/physics-classical.emath"),
        5,
    );
}

#[test]
fn classical_law_symbols_resolve_from_embedded_package() {
    let result = check(
        "physics-classical-import",
        "use physics::classical::{NewtonSecond, Hooke}",
    );
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.errors().collect::<Vec<_>>()
    );
    assert_eq!(result.package.declarations.len(), 2);
    assert_eq!(result.package.law_metadata.len(), 2);
    let report = run_package(&result.package);
    assert_eq!(report.summary.tests, 2);
    assert_eq!(report.summary.passed, 2);
}

#[test]
fn relativity_and_cs_law_packs_evaluate() {
    assert_pack(
        "physics-relativity",
        include_str!("../../../language/stdlib/laws/physics-relativity.emath"),
        1,
    );
    assert_pack(
        "computer-science",
        include_str!("../../../language/stdlib/laws/computer-science.emath"),
        3,
    );
}

#[test]
fn probability_and_analysis_law_packs_evaluate() {
    assert_pack(
        "probability-statistics",
        include_str!("../../../language/stdlib/laws/probability-statistics.emath"),
        3,
    );
    assert_pack(
        "analysis",
        include_str!("../../../language/stdlib/laws/analysis.emath"),
        3,
    );
}

#[test]
fn algebra_and_control_law_packs_evaluate() {
    assert_pack(
        "algebra-number-theory",
        include_str!("../../../language/stdlib/laws/algebra-number-theory.emath"),
        3,
    );
    assert_pack(
        "optimization-control",
        include_str!("../../../language/stdlib/laws/optimization-control.emath"),
        3,
    );
}

#[test]
fn violated_law_assumption_refuses_before_partial_evaluation() {
    let source = include_str!("../../../language/stdlib/laws/probability-statistics.emath")
        .replace("given normalizer = 0.4", "given normalizer = 0");
    let result = check("probability-invalid-normalizer", &source);
    assert!(!result.diagnostics.has_errors());
    let report = run_package(&result.package);
    assert_eq!(report.summary.refused, 1);
    assert_eq!(report.summary.passed, 2);
}

#[test]
fn every_embedded_law_package_resolves_a_symbol() {
    let imports = [
        "use physics::classical::NewtonSecond",
        "use physics::relativity::MassEnergyEquivalence",
        "use cs::laws::AmdahlSpeedup",
        "use probability::laws::BayesPosterior",
        "use analysis::laws::TaylorQuadratic",
        "use number_theory::laws::ModularInverse",
        "use optimization_control::laws::BellmanTwoActionBackup",
    ];
    for source in imports {
        let result = check("law-package-import", source);
        assert!(
            !result.diagnostics.has_errors(),
            "{source}: {:?}",
            result.diagnostics.errors().collect::<Vec<_>>()
        );
        assert_eq!(result.package.declarations.len(), 1, "{source}");
        assert_eq!(run_package(&result.package).summary.passed, 1, "{source}");
    }
}
