//! `emath method` language kind: algorithm + falsifier, proposal-only.

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
fn method_kind_admits_as_proposal_only() {
    let source = include_str!("../../../tests/fixtures/language/intro/method-declarations.emath");
    let (_, parse_diagnostics) = parse_str(source);
    assert!(!parse_diagnostics.has_errors());

    let checked = check("method-kind", source);
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
        ["RungeKutta4"]
    );
    let method = &checked.package.declarations[0];
    assert_eq!(method.kind_label, "method");
    assert_eq!(method.definitions.len(), 0);
    assert_eq!(method.evidence.len(), 1);
    let claim = &method.evidence[0];
    assert_eq!(claim.verdict, ClaimVerdict::NotRun);
    assert_eq!(claim.level, EvidenceLevel::E1);
    assert_eq!(claim.checker, None);
    assert_eq!(claim.falsifiers.len(), 1);

    let repeated = check("method-kind-repeat", source);
    assert_eq!(
        checked.package.meaning_id(&[]).unwrap(),
        repeated.package.meaning_id(&[]).unwrap()
    );
}

#[test]
fn methods_optional_and_refusals() {
    // Methods are not required on ordinary files: a plain function with no
    // method involvement still admits.
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

    // A method cannot raise its own evidence authority (fixture refuses).
    let invalid = check(
        "invalid-method",
        include_str!("../../../tests/invalid/method_declarations.emath"),
    );
    assert!(
        invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-027")
    );
    assert!(invalid.package.declarations.is_empty());

    // The schema requires exactly one algorithm and one falsifier section.
    let incomplete = check(
        "incomplete-method",
        "\
use std.kinds.method

emath method RungeKutta4:
    falsifier:
        condition: \"step-doubling residual exceeds tolerance\"
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
        "method-without-import",
        "\
emath method RungeKutta4:
    algorithm:
        kind: \"integrator\"
    falsifier:
        condition: \"step-doubling residual exceeds tolerance\"
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
