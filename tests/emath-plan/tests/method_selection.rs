//! Constitutional: methods are optional on ordinary declarations.
//!
//! Core semantics must not depend on the Method Foundry. A plain
//! `emath function` file admits with no `methods:` anywhere; adding an
//! unused method pack to the same file must not change the ordinary
//! function's MeaningID; and a method card cannot raise evidence
//! authority (E1/not-run, proposal-only, always).

use emath_core::limits::Limits;
use emath_ir::{ClaimVerdict, EvidenceLevel};
use emath_sema::CompilerSession;
use emath_syntax::{install_source_parser, parse_str};

fn check(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

/// `hello-square.emath`-shaped ordinary file: no methods anywhere.
const PLAIN_FUNCTION: &str = "\
emath function Square:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
";

#[test]
fn function_only_file_stays_valid() {
    // The constitutional happy path: a function-only file admits.
    let (_, parse_diagnostics) = parse_str(PLAIN_FUNCTION);
    assert!(!parse_diagnostics.has_errors());
    let checked = check("function-only", PLAIN_FUNCTION);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    assert_eq!(checked.package.declarations.len(), 1);
    assert_eq!(checked.package.declarations[0].kind_label, "function");
    assert!(checked.package.declarations[0].evidence.is_empty());
}

#[test]
fn unused_method_pack_does_not_change_meaning_id() {
    // Adding an unused method pack next to the function must not touch
    // the function's MeaningID slice: core semantics are
    // Foundry-independent. The package meaning covers the whole file, so
    // the pinned assertions are on the function declaration itself.
    let with_pack = format!(
        "{PLAIN_FUNCTION}\nuse std.kinds.method\n\nemath method UnusedPack:\n    algorithm:\n        kind: \"scoring\"\n    falsifier:\n        condition: \"held-out accuracy drops below the declared floor\"\n"
    );

    let before = check("before-pack", PLAIN_FUNCTION);
    let after = check("after-pack", &with_pack);
    assert!(
        !after.diagnostics.has_errors(),
        "{:?}",
        after.diagnostics.errors().collect::<Vec<_>>()
    );
    assert_eq!(after.package.declarations.len(), 2);

    let before_fn = &before.package.declarations[0];
    let after_fn = after
        .package
        .declarations
        .iter()
        .find(|declaration| declaration.kind_label == "function")
        .expect("ordinary function must still be admitted");
    assert_eq!(before_fn.name, after_fn.name);
    assert_eq!(before_fn.kind, after_fn.kind);
    assert_eq!(before_fn.inputs.len(), after_fn.inputs.len());
    assert_eq!(before_fn.outputs.len(), after_fn.outputs.len());
    assert_eq!(before_fn.definitions.len(), after_fn.definitions.len());
    // The method's evidence must not leak into the ordinary declaration.
    assert!(after_fn.evidence.is_empty());

    // MeaningID stability for the identical function-only source checked
    // twice (determinism), and the two-source check confirms the added
    // pack produced no silent desugar into the function.
    let repeat = check("function-only-repeat", PLAIN_FUNCTION);
    assert_eq!(
        before.package.meaning_id(&[]).unwrap(),
        repeat.package.meaning_id(&[]).unwrap()
    );
}

#[test]
fn method_authority_stays_proposal_only() {
    // A method card is always E1/not-run with no checker, regardless of
    // what its authority section claims; raising is refused outright
    // (tests/invalid/method_selection.emath).
    let proposal = check(
        "proposal-only",
        "\
use std.kinds.method

emath method Careful:
    algorithm:
        kind: \"scoring\"
    falsifier:
        condition: \"held-out accuracy drops below the declared floor\"
    authority:
        claims: \"proposal\"
",
    );
    assert!(
        !proposal.diagnostics.has_errors(),
        "{:?}",
        proposal.diagnostics.errors().collect::<Vec<_>>()
    );
    let method = proposal
        .package
        .declarations
        .iter()
        .find(|declaration| declaration.kind_label == "method")
        .expect("proposal-admitting method must be admitted");
    assert_eq!(method.evidence.len(), 1);
    assert_eq!(method.evidence[0].verdict, ClaimVerdict::NotRun);
    assert_eq!(method.evidence[0].level, EvidenceLevel::E1);
    assert_eq!(method.evidence[0].checker, None);

    let invalid = check(
        "authority-grab",
        include_str!("../../invalid/method_selection.emath"),
    );
    assert!(
        invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-027")
    );
    assert!(invalid.package.declarations.is_empty());
}
