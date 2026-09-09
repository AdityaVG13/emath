//! `emath freeze` / `why` / `expand` / `assumptions` CLI surface (syntax half).

use emath_syntax::{
    ExactnessStatus, exactness_ledger, expand_scratch, explanation_notes, parse_str,
};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

use emath_test_harness::{Probe, boot};

#[test]
fn exactness_introspection() {
    boot();
    let mut probe = Probe::new("`emath freeze` / `why` / `expand` / `assumptions` CLI surface (syntax half).");
    probe.case("freeze_keeps_open_holes_visible", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/fixtures/language/intro/scratch.emath");
    let expansion = expand_scratch(source);
    p.demand("1",expansion.rewritten(), stringify!(expansion.rewritten()));
    if p.failures().len() != f0 { return; }
    let ledger = exactness_ledger(source);
    p.demand("2",ledger.count(ExactnessStatus::Open) >= 1, stringify!(ledger.count(ExactnessStatus::Open) >= 1));
    if p.failures().len() != f0 { return; }
    let notes = explanation_notes(source);
    p.demand("3",notes
            .iter()
            .any(|note| note.stability == ExactnessStatus::Inferred), stringify!(notes
            .iter()
            .any(|note| note.stability == ExactnessStatus::Inferred)));
    if p.failures().len() != f0 { return; }
    let (_, diagnostics) = parse_str(source);
    p.demand("4",!diagnostics.has_errors(), stringify!(!diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }

    });
    probe.case("freeze_must_not_claim_open_holes", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/exactness_introspection.emath");
    p.demand("1",has_error(source, "E-SYN-147"), stringify!(has_error(source, "E-SYN-147")));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
