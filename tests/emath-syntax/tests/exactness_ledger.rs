//! Exactness ledger: declared, inferred, constructed, open meaning.

use emath_syntax::{
    ExactnessDimension, ExactnessStatus, exactness_ledger, exactness_ledger_raised, parse_str,
};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

use emath_test_harness::{Probe, boot};

#[test]
fn exactness_ledger_probe() {
    boot();
    let mut probe = Probe::new("Exactness ledger: declared, inferred, constructed, open meaning.");
    probe.case("ledger_counts_are_deterministic", |p| {
    let f0 = p.failures().len();

    let source = "y = x^2 + 4\nexample x = 3\n";
    let once = exactness_ledger(source);
    let twice = exactness_ledger(source);
    p.eq("1", &once, &twice);
    p.demand("2",once.count(ExactnessStatus::Open) >= 1, stringify!(once.count(ExactnessStatus::Open) >= 1));
    if p.failures().len() != f0 { return; }
    p.demand("3",once.count(ExactnessStatus::Inferred) >= 1, stringify!(once.count(ExactnessStatus::Inferred) >= 1));
    if p.failures().len() != f0 { return; }
    let (_, diagnostics) = parse_str(source);
    p.demand("4",!diagnostics.has_errors(), stringify!(!diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }

    });
    probe.case("raise_units_declares_without_rewriting_other_rows", |p| {
    let f0 = p.failures().len();

    let source = "y = x^2 + 4\nexample x = 3\n";
    let before = exactness_ledger(source);
    let after = exactness_ledger_raised(source, &[ExactnessDimension::Unit]);
    let unit_before = before
        .entries
        .iter()
        .find(|entry| entry.dimension == ExactnessDimension::Unit)
        .unwrap();
    let unit_after = after
        .entries
        .iter()
        .find(|entry| entry.dimension == ExactnessDimension::Unit)
        .unwrap();
    p.eq("1", unit_before.status, ExactnessStatus::Open);
    p.eq("2", unit_after.status, ExactnessStatus::Declared);
    p.eq("3", before.count(ExactnessStatus::Inferred), after.count(ExactnessStatus::Inferred));
    let evidence_before = before
        .entries
        .iter()
        .find(|entry| entry.dimension == ExactnessDimension::Evidence)
        .unwrap();
    let evidence_after = after
        .entries
        .iter()
        .find(|entry| entry.dimension == ExactnessDimension::Evidence)
        .unwrap();
    p.eq("4", &evidence_before.status, &evidence_after.status);
    p.eq("5", ExactnessDimension::from_raise_token("units"), Some(ExactnessDimension::Unit));
    p.eq("6", ExactnessDimension::from_raise_token("unit"), Some(ExactnessDimension::Unit));
    p.demand("7", ExactnessDimension::from_raise_token("evidence") == None, format!("expected None, got {:?}", (ExactnessDimension::from_raise_token("evidence"))));
    if p.failures().len() != f0 { return; }
    p.demand("8", ExactnessDimension::from_raise_token("numeric") == None, format!("expected None, got {:?}", (ExactnessDimension::from_raise_token("numeric"))));
    if p.failures().len() != f0 { return; }

    });
    probe.case("claiming_exactness_with_open_hole_is_e_syn_147", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/exactness_ledger.emath");
    p.demand("1",has_error(source, "E-SYN-147"), stringify!(has_error(source, "E-SYN-147")));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
