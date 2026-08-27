//! Inspectable desugaring across progressive-exactness levels.

use emath_core::FileId;
use emath_core::limits::Limits;
use emath_core::tree::Item;
use emath_syntax::formatter::format;
use emath_syntax::{expand_scratch, parse_lossless, parse_str};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

#[test]
fn l0_expansion_is_visible_and_round_trips() {
    let source = "2+2\n";
    let expansion = expand_scratch(source);
    assert!(expansion.rewritten);
    assert!(
        expansion.expanded.contains("emath function Scratch:"),
        "{}",
        expansion.expanded
    );
    assert!(
        expansion
            .diagnostics
            .items()
            .iter()
            .any(|item| item.code == "N-SCRATCH-001"),
        "desugar must not be silent"
    );
    let parsed = parse_lossless(&expansion.expanded, FileId(0), &Limits::default());
    assert!(!parsed.diagnostics.has_errors());
    let once = format(&parsed.tree, &parsed.comments);
    let twice = format(
        &parse_lossless(&once, FileId(0), &Limits::default()).tree,
        &[],
    );
    assert_eq!(twice, once, "fmt(fmt(expanded)) must equal fmt(expanded)");
}

#[test]
fn l0_and_l3_share_declaration_center() {
    let l0 = parse_str("2+2\n").0;
    let l3 = parse_str("emath function Scratch:\n    definitions:\n        result = 2+2\n").0;
    let Item::Declaration(a) = &l0.items[0] else {
        panic!("l0");
    };
    let Item::Declaration(b) = &l3.items[0] else {
        panic!("l3");
    };
    assert_eq!(a.name, b.name);
    assert_eq!(a.body.len(), b.body.len());
}

#[test]
fn inspectable_example_file_expands() {
    let source = "2 + 2\n";
    let expansion = expand_scratch(source);
    assert!(expansion.rewritten);
    assert!(
        expansion.expanded.contains("result = 2 + 2"),
        "{}",
        expansion.expanded
    );
    let (_, diagnostics) = parse_str(source);
    assert!(!diagnostics.has_errors());
}

#[test]
fn hidden_desugar_is_e_syn_144() {
    let source = include_str!("../../../tests/invalid/v9_06_2rdq_4.emath");
    assert!(
        has_error(source, "E-SYN-144"),
        "hidden desugar must refuse with E-SYN-144"
    );
}
