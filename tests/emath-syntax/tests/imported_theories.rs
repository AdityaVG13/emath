//! Imported theory/model/morphism kinds with bounded finite checking.

use emath_ir::{ClaimVerdict, EvidenceLevel};
use emath_syntax::{parse_str};

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    Source::from_str(name, source).check()
}

use emath_test_harness::{Probe, Source, boot};

#[test]
fn imported_theories() {
    boot();
    let mut probe = Probe::new("Imported theory/model/morphism kinds with bounded finite checking.");
    probe.case("checks_finite_model_and_power_morphism", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/fixtures/language/intro/imported-theories.emath");
    let (_, parse_diagnostics) = parse_str(source);
    p.demand("1",!parse_diagnostics.has_errors(), stringify!(!parse_diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }

    let checked = check("finite-categories", source);
    p.demand("2",!checked.diagnostics.has_errors(), format!(
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3", (checked
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.kind_label.as_str())
            .collect::<Vec<_>>()) == (["theory", "model", "morphism"]), format!("expected {:?}, got {:?}", (["theory", "model", "morphism"]), (checked
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.kind_label.as_str())
            .collect::<Vec<_>>())));
    if p.failures().len() != f0 { return; }

    let theory = &checked.package.declarations[0];
    p.demand("4",theory
            .evidence
            .iter()
            .all(|claim| claim.verdict == ClaimVerdict::NotRun && claim.level == EvidenceLevel::E1), stringify!(theory
            .evidence
            .iter()
            .all(|claim| claim.verdict == ClaimVerdict::NotRun && claim.level == EvidenceLevel::E1)));
    if p.failures().len() != f0 { return; }
    for declaration in &checked.package.declarations[1..] {
        p.demand("5",declaration.evidence.iter().all(
                |claim| claim.verdict == ClaimVerdict::Pass && claim.level == EvidenceLevel::E2
            ), stringify!(declaration.evidence.iter().all(
                |claim| claim.verdict == ClaimVerdict::Pass && claim.level == EvidenceLevel::E2
            )));
        if p.failures().len() != f0 { return; }
    }

    let repeated = check("finite-categories-repeat", source);
    p.eq("6", checked.package.meaning_id(&[]).unwrap(), repeated.package.meaning_id(&[]).unwrap());

    });
    probe.case("refuses_false_laws_and_unimported_kinds", |p| {
    let f0 = p.failures().len();

    let invalid = check(
        "false-associativity",
        include_str!("../../../tests/invalid/imported_theories.emath"),
    );
    p.demand("1",invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-LAW-003"), stringify!(invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-LAW-003")));
    if p.failures().len() != f0 { return; }
    p.eq("2", invalid.package.declarations.len(), 1);

    let unimported = check(
        "unimported-theory",
        "\
emath theory Monoid:
    structure:
        carrier: \"finite\"
        operation: \"binary\"
        identity: 0
    laws:
        \"associative\"
",
    );
    p.demand("3",unimported
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-100"), stringify!(unimported
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-100")));
    if p.failures().len() != f0 { return; }
    p.demand("4",unimported.package.declarations.is_empty(), stringify!(unimported.package.declarations.is_empty()));
    if p.failures().len() != f0 { return; }

    });
    probe.case("refuses_non_preserving_morphism", |p| {
    let f0 = p.failures().len();

    let source = "\
use std.kinds.theory
use std.kinds.model
use std.kinds.morphism
emath theory Monoid:
    structure:
        carrier: \"finite\"
        operation: \"binary\"
        identity: 0
    laws:
        \"associative\"
emath model Mod17:
    finite:
        theory: \"Monoid\"
        modulus: 17
        left_coefficient: 1
        right_coefficient: 1
        identity: 0
emath model Mod5:
    finite:
        theory: \"Monoid\"
        modulus: 5
        left_coefficient: 1
        right_coefficient: 1
        identity: 0
emath morphism InvalidReduction:
    mapping:
        source: \"Mod17\"
        target: \"Mod5\"
        scale: 1
";
    let checked = check("invalid-morphism", source);
    p.demand("1",checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-LAW-003"), stringify!(checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-LAW-003")));
    if p.failures().len() != f0 { return; }
    p.eq("2", checked.package.declarations.len(), 3);

    });
    probe.finish();
}
