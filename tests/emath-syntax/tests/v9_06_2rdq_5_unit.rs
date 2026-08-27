//! Exactness ledger: declared, inferred, constructed, open meaning.

use emath_syntax::{
    ExactnessDimension, ExactnessStatus, exactness_ledger, exactness_ledger_raised, parse_str,
};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

#[test]
fn ledger_counts_are_deterministic() {
    let source = "y = x^2 + 4\nexample x = 3\n";
    let once = exactness_ledger(source);
    let twice = exactness_ledger(source);
    assert_eq!(once, twice);
    assert!(once.count(ExactnessStatus::Open) >= 1);
    assert!(once.count(ExactnessStatus::Inferred) >= 1);
    let (_, diagnostics) = parse_str(source);
    assert!(!diagnostics.has_errors());
}

#[test]
fn raise_units_declares_without_rewriting_other_rows() {
    let source = "y = x^2 + 4\nexample x = 3\n";
    let before = exactness_ledger(source);
    let after = exactness_ledger_raised(source, &["units"]);
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
    assert_eq!(unit_before.status, ExactnessStatus::Open);
    assert_eq!(unit_after.status, ExactnessStatus::Declared);
    assert_eq!(
        before.count(ExactnessStatus::Inferred),
        after.count(ExactnessStatus::Inferred)
    );
}

#[test]
fn claiming_exactness_with_open_hole_is_e_syn_147() {
    let source = include_str!("../../../tests/invalid/v9_06_2rdq_5.emath");
    assert!(has_error(source, "E-SYN-147"));
}
