//! `emath method` language kind: algorithm + falsifier, proposal-only.

use emath_ir::{ClaimVerdict, EvidenceLevel};
use emath_syntax::{parse_str};

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    Source::from_str(name, source).check()
}

use emath_test_harness::{Probe, Source, boot};

#[test]
fn method_declarations() {
    boot();
    let mut probe = Probe::new("`emath method` language kind: algorithm + falsifier, proposal-only.");
    probe.case("method_kind_admits_as_proposal_only", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/fixtures/language/intro/method-declarations.emath");
    let (_, parse_diagnostics) = parse_str(source);
    p.demand("1",!parse_diagnostics.has_errors(), stringify!(!parse_diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }

    let checked = check("method-kind", source);
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
            .collect::<Vec<_>>()) == (["RungeKutta4"]), format!("expected {:?}, got {:?}", (["RungeKutta4"]), (checked
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.name.leaf())
            .collect::<Vec<_>>())));
    if p.failures().len() != f0 { return; }
    let method = &checked.package.declarations[0];
    p.demand("4", (method.kind_label) == ("method"), format!("expected {:?}, got {:?}", ("method"), (method.kind_label)));
    if p.failures().len() != f0 { return; }
    p.eq("5", method.definitions.len(), 0);
    p.eq("6", method.evidence.len(), 1);
    let claim = &method.evidence[0];
    p.eq("7", claim.verdict, ClaimVerdict::NotRun);
    p.eq("8", claim.level, EvidenceLevel::E1);
    p.demand("9", claim.checker == None, format!("expected None, got {:?}", (claim.checker)));
    if p.failures().len() != f0 { return; }
    p.eq("10", claim.falsifiers.len(), 1);

    let repeated = check("method-kind-repeat", source);
    p.eq("11", checked.package.meaning_id(&[]).unwrap(), repeated.package.meaning_id(&[]).unwrap());

    });
    probe.case("methods_optional_and_refusals", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!plain.diagnostics.has_errors(), format!(
        "{:?}",
        plain.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    // A method cannot raise its own evidence authority (fixture refuses).
    let invalid = check(
        "invalid-method",
        include_str!("../../../tests/invalid/method_declarations.emath"),
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
        "method-without-import",
        "\
emath method RungeKutta4:
    algorithm:
        kind: \"integrator\"
    falsifier:
        condition: \"step-doubling residual exceeds tolerance\"
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
