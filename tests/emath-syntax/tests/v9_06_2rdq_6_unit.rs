//! `emath freeze` / `why` / `expand` / `assumptions` CLI surface (syntax half).

use emath_syntax::{
    ExactnessStatus, exactness_ledger, expand_scratch, explanation_notes, parse_str,
};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

#[test]
fn freeze_keeps_open_holes_visible() {
    let source = include_str!("../../../language/examples/intro/scratch.emath");
    let expansion = expand_scratch(source);
    assert!(expansion.rewritten);
    let ledger = exactness_ledger(source);
    assert!(ledger.count(ExactnessStatus::Open) >= 1);
    let notes = explanation_notes(source);
    assert!(notes.iter().any(|note| note.stability == "inferred"));
    let (_, diagnostics) = parse_str(source);
    assert!(!diagnostics.has_errors());
}

#[test]
fn freeze_must_not_claim_open_holes() {
    let source = include_str!("../../../tests/invalid/v9_06_2rdq_6.emath");
    assert!(has_error(source, "E-SYN-147"));
}
