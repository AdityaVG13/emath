//! `emath experiment` research-programme sections: references, not embedding.

use emath_core::limits::Limits;
use emath_ir::{ClaimVerdict, EvidenceLevel};
use emath_sema::CompilerSession;
use emath_syntax::{install_source_parser, parse_str};

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

#[test]
fn experiment_programme_admits_as_reference_only() {
    let source = include_str!("../../../tests/fixtures/language/intro/experiment-sections.emath");
    let (_, parse_diagnostics) = parse_str(source);
    assert!(!parse_diagnostics.has_errors());

    let checked = check("experiment-programme", source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    assert_eq!(
        checked
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.name.leaf())
            .collect::<Vec<_>>(),
        ["RootMethodPortfolio"]
    );
    let experiment = &checked.package.declarations[0];
    assert_eq!(experiment.kind_label, "experiment");
    assert_eq!(experiment.evidence.len(), 1);
    let claim = &experiment.evidence[0];
    assert_eq!(claim.verdict, ClaimVerdict::NotRun);
    assert_eq!(claim.level, EvidenceLevel::E1);
    assert_eq!(claim.checker, None);
    // One falsifier per tracked problem; a keep-gate cannot self-promote.
    assert_eq!(claim.falsifiers.len(), 2);

    let repeated = check("experiment-programme-repeat", source);
    assert_eq!(
        checked.package.meaning_id(&[]).unwrap(),
        repeated.package.meaning_id(&[]).unwrap()
    );
}

#[test]
fn methods_stay_optional_and_refusals() {
    // A function without any methods: section still admits (constitutional).
    let plain = check(
        "plain-function",
        "\
emath function Add:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x + x
",
    );
    assert!(
        !plain.diagnostics.has_errors(),
        "{:?}",
        plain.diagnostics.errors().collect::<Vec<_>>()
    );

    // A keep-gate cannot grant authority by declaration (fixture refuses).
    let invalid = check(
        "invalid-experiment",
        include_str!("../../../tests/invalid/experiment_sections.emath"),
    );
    assert!(
        invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-027")
    );
    assert!(invalid.package.declarations.is_empty());

    // The schema requires exactly one problems section.
    let incomplete = check(
        "incomplete-experiment",
        "\
use std.kinds.experiment

emath experiment EmptyProgramme:
    methods:
        \"RungeKutta4\"
",
    );
    assert!(
        incomplete
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-003")
    );
    assert!(incomplete.package.declarations.is_empty());

    // Without the schema import the kind is an unknown custom kind.
    let missing_kind = check(
        "experiment-without-import",
        "\
emath experiment RootMethodPortfolio:
    problems:
        \"an open problem\"
",
    );
    assert!(
        missing_kind
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-100")
    );
    assert!(missing_kind.package.declarations.is_empty());
}
