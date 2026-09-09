//! `emath experiment` research-programme sections: references, not embedding.

use emath_ir::{ClaimVerdict, EvidenceLevel};
use emath_syntax::{parse_str};

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    Source::from_str(name, source).check()
}

use emath_test_harness::{Probe, Source, boot};

#[test]
fn experiment_sections() {
    boot();
    let mut probe = Probe::new("`emath experiment` research-programme sections: references, not embedding.");
    probe.case("experiment_programme_admits_as_reference_only", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/fixtures/language/intro/experiment-sections.emath");
    let (_, parse_diagnostics) = parse_str(source);
    p.demand("1",!parse_diagnostics.has_errors(), stringify!(!parse_diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }

    let checked = check("experiment-programme", source);
    p.demand("2",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3", (checked
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.name.leaf())
            .collect::<Vec<_>>()) == (["RootMethodPortfolio"]), format!("expected {:?}, got {:?}", (["RootMethodPortfolio"]), (checked
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.name.leaf())
            .collect::<Vec<_>>())));
    if p.failures().len() != f0 { return; }
    let experiment = &checked.package.declarations[0];
    p.demand("4", (experiment.kind_label) == ("experiment"), format!("expected {:?}, got {:?}", ("experiment"), (experiment.kind_label)));
    if p.failures().len() != f0 { return; }
    p.eq("5", experiment.evidence.len(), 1);
    let claim = &experiment.evidence[0];
    p.eq("6", claim.verdict, ClaimVerdict::NotRun);
    p.eq("7", claim.level, EvidenceLevel::E1);
    p.demand("8", claim.checker == None, format!("expected None, got {:?}", (claim.checker)));
    if p.failures().len() != f0 { return; }
    // One falsifier per tracked problem; a keep-gate cannot self-promote.
    p.eq("9", claim.falsifiers.len(), 2);

    let repeated = check("experiment-programme-repeat", source);
    p.eq("10", checked.package.meaning_id(&[]).unwrap(), repeated.package.meaning_id(&[]).unwrap());

    });
    probe.case("methods_stay_optional_and_refusals", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!plain.diagnostics.has_errors(), format!(
        "{:?}",
        plain.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    // A keep-gate cannot grant authority by declaration (fixture refuses).
    let invalid = check(
        "invalid-experiment",
        include_str!("../../../tests/invalid/experiment_sections.emath"),
    );
    p.demand("2",invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-027"), stringify!(invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-027")));
    if p.failures().len() != f0 { return; }
    p.demand("3",invalid.package.declarations.is_empty(), stringify!(invalid.package.declarations.is_empty()));
    if p.failures().len() != f0 { return; }

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
    p.demand("4",incomplete
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-003"), stringify!(incomplete
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-003")));
    if p.failures().len() != f0 { return; }
    p.demand("5",incomplete.package.declarations.is_empty(), stringify!(incomplete.package.declarations.is_empty()));
    if p.failures().len() != f0 { return; }

    // Without the schema import the kind is an unknown custom kind.
    let missing_kind = check(
        "experiment-without-import",
        "\
emath experiment RootMethodPortfolio:
    problems:
        \"an open problem\"
",
    );
    p.demand("6",missing_kind
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-100"), stringify!(missing_kind
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-100")));
    if p.failures().len() != f0 { return; }
    p.demand("7",missing_kind.package.declarations.is_empty(), stringify!(missing_kind.package.declarations.is_empty()));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
