//! `emath-syntax` canonical formatter tests (migrated from
//! `crates/emath-syntax/src/formatter.rs`).

use emath_core::limits::Limits;
use emath_core::FileId;
use emath_syntax::formatter::format;
use emath_syntax::parse_lossless;

fn format_once(text: &str) -> String {
    let parsed = parse_lossless(text, FileId(0), &Limits::default());
    assert!(!parsed.diagnostics.has_errors(), "fixture must parse");
    format(&parsed.tree, &parsed.comments)
}

/// SURF-0013: an Equation statement inside a nested section renders
/// at sibling indent (one level, not two) with a single newline —
/// no blank line right after the equation.
#[test]
fn equation_renders_at_sibling_indent_without_blank_line() {
    let source = "emath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n";
    let once = format_once(source);
    assert!(
        once.contains("        y = x * x"),
        "equation must sit at sibling indent: {once}"
    );
    assert!(
        !once.contains("            y = x * x"),
        "no double indent: {once}"
    );
    assert!(
        !once.contains("y = x * x\n\n"),
        "no blank line immediately after the equation: {once}"
    );
}

/// Golden: formatting is idempotent (`fmt(fmt(s)) == fmt(s)`) and
/// the formatted output parses back cleanly.
#[test]
fn formatting_is_idempotent_and_parse_stable() {
    let source = "emath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n";
    let once = format_once(source);
    assert_eq!(format_once(&once), once, "fmt(fmt(s)) must equal fmt(s)");
    let rebound = parse_lossless(&once, FileId(0), &Limits::default());
    assert!(
        !rebound.diagnostics.has_errors(),
        "formatted output must parse back"
    );
}

/// SURF-0013: every valid corpus file must be byte-canonical under
/// the lossless formatter (`fmt(file) == file`). This pins the exact
/// canonical spellings: `produce rust.library` keeps its dot,
/// expression paths render as `state.scale`, and section generics
/// render `evaluate <y>:` / `example <name>:` (angle, spaced).
#[test]
fn corpus_files_are_lossless_round_trip() {
    for (name, text) in [
        ("square", include_str!("../../valid/square.emath")),
        (
            "affine_scorer",
            include_str!("../../valid/affine_scorer.emath"),
        ),
    ] {
        let parsed = parse_lossless(text, FileId(0), &Limits::default());
        assert!(
            !parsed.diagnostics.has_errors(),
            "{name}: fixture must parse"
        );
        let canonical = format(&parsed.tree, &parsed.comments);
        assert_eq!(
            canonical, text,
            "{name}: corpus file must be byte-canonical (fmt(file) == file)"
        );
    }
}

/// SURF-0013: the canonical render of the corpus round-trips: the
/// formatted output parses back to the identical tree (format-parse
/// fixpoint), so re-formatting never changes the output.
#[test]
fn corpus_canonical_reparse_is_stable() {
    for (name, text) in [
        ("square", include_str!("../../valid/square.emath")),
        (
            "affine_scorer",
            include_str!("../../valid/affine_scorer.emath"),
        ),
    ] {
        let parsed = parse_lossless(text, FileId(0), &Limits::default());
        let canonical = format(&parsed.tree, &parsed.comments);
        let reborn = parse_lossless(&canonical, FileId(0), &Limits::default());
        assert!(
            !reborn.diagnostics.has_errors(),
            "{name}: canonical must parse back"
        );
        assert_eq!(
            format(&reborn.tree, &reborn.comments),
            canonical,
            "{name}: fmt(fmt(x)) == fmt(x)"
        );
    }
}

#[test]
fn numeric_units_surface_round_trips() {
    let source = "\
emath function Timed:
    inputs:
        t: Duration
        rate: Per<Duration>
    outputs:
        y: Float64
    definitions:
        y = t / 1 s * rate * 1 s
    compile:
        target rust
        numeric interval-f64
        precision 53
        error-limit 1e-12
";
    let once = format_once(source);
    assert_eq!(format_once(&once), once, "fmt(fmt(s)) must equal fmt(s)");
    let rebound = parse_lossless(&once, FileId(0), &Limits::default());
    assert!(
        !rebound.diagnostics.has_errors(),
        "formatted numeric/units surface must parse back: {:?}",
        rebound
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
    assert!(once.contains("numeric interval-f64"));
    assert!(once.contains("1 s"));
    assert!(once.contains("Per<Duration>"));
}

/// ELP item attributes round-trip in the canonical `@` spelling:
/// `emath_item = { attribute }, "emath", ...`. The formatter must emit
/// the grammar form (not a Rust-style bracket) and remain idempotent.
#[test]
fn item_attributes_round_trip() {
    let source = "\
@capabilities(experimental-syntax)
@experimental
emath function P:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
";
    let once = format_once(source);
    assert!(
        once.starts_with("@capabilities(experimental-syntax)\n@experimental\nemath function P:"),
        "attributes must render before the head in `@` form: {once}"
    );
    assert!(
        !once.contains('['),
        "attributes must not render as Rust-style brackets: {once}"
    );
    assert_eq!(format_once(&once), once, "fmt(fmt(s)) must equal fmt(s)");
    let rebound = parse_lossless(&once, FileId(0), &Limits::default());
    assert!(
        !rebound.diagnostics.has_errors(),
        "formatted attributes must parse back: {:?}",
        rebound
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
}

/// Notation declarations round-trip in the canonical spelling
/// `notation <fixity> <prec> "<glyph>" => <path> [alias "<str>"]`,
/// separated from sibling items by one blank line, and the reformatted
/// output stays idempotent and parse-stable.
#[test]
fn notation_items_round_trip() {
    let source = "\
package tst.ex2

notation infixl 40 \"⊕\" => core::math::pow alias \"pw\"

notation prefix 80 \"√\" => core::math::sqrt

notation postfix 90 \"inv\" => core::math::recip

emath function F:
    outputs:
        y: Float64
    definitions:
        y = 1.0
";
    let once = format_once(source);
    assert_eq!(format_once(&once), once, "fmt(fmt(s)) must equal fmt(s)");
    assert!(once.contains("notation infixl 40 \"⊕\" => core::math::pow alias \"pw\""));
    assert!(once.contains("notation prefix 80 \"√\" => core::math::sqrt"));
    assert!(once.contains("notation postfix 90 \"inv\" => core::math::recip"));
    let rebound = parse_lossless(&once, FileId(0), &Limits::default());
    assert!(
        !rebound.diagnostics.has_errors(),
        "formatted notation must parse back: {:?}",
        rebound
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
}

/// Quoted string arguments keep their quotes through the round trip so
/// identifier args and string args never merge on reformat.
#[test]
fn attribute_string_arguments_keep_quotes() {
    let source = "\
@capabilities(\"experimental-syntax\", nightly)
emath function P:
    outputs:
        y: Float64
    definitions:
        y = 1.0
";
    let once = format_once(source);
    assert!(
        once.contains("@capabilities(\"experimental-syntax\", nightly)"),
        "quoted form must round-trip: {once}"
    );
    assert_eq!(format_once(&once), once, "fmt(fmt(s)) must equal fmt(s)");
}
