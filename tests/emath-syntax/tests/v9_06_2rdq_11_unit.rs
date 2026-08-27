//! Imported theory/model/morphism kinds with bounded finite checking.

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
fn v9_06_2rdq_11_checks_finite_model_and_power_morphism() {
    let source = include_str!("../../../language/examples/intro/v9_06_2rdq_11.emath");
    let (_, parse_diagnostics) = parse_str(source);
    assert!(!parse_diagnostics.has_errors());

    let checked = check("finite-categories", source);
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
            .map(|declaration| declaration.kind_label.as_str())
            .collect::<Vec<_>>(),
        ["theory", "model", "morphism"]
    );

    let theory = &checked.package.declarations[0];
    assert!(
        theory
            .evidence
            .iter()
            .all(|claim| claim.verdict == ClaimVerdict::NotRun && claim.level == EvidenceLevel::E1)
    );
    for declaration in &checked.package.declarations[1..] {
        assert!(
            declaration.evidence.iter().all(
                |claim| claim.verdict == ClaimVerdict::Pass && claim.level == EvidenceLevel::E2
            )
        );
    }

    let repeated = check("finite-categories-repeat", source);
    assert_eq!(
        checked.package.meaning_id(&[]).unwrap(),
        repeated.package.meaning_id(&[]).unwrap()
    );
}

#[test]
fn v9_06_2rdq_11_refuses_false_laws_and_unimported_kinds() {
    let invalid = check(
        "false-associativity",
        include_str!("../../../tests/invalid/v9_06_2rdq_11.emath"),
    );
    assert!(
        invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-LAW-003")
    );
    assert_eq!(invalid.package.declarations.len(), 1);

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
    assert!(
        unimported
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-100")
    );
    assert!(unimported.package.declarations.is_empty());
}

#[test]
fn v9_06_2rdq_11_refuses_non_preserving_morphism() {
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
    assert!(
        checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-LAW-003")
    );
    assert_eq!(checked.package.declarations.len(), 3);
}
