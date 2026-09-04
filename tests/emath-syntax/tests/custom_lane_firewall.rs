//! Custom-lane firewall (fixtures, constitutional pins).
//!
//! The custom lane must never silently fall through into strict meaning:
//! alien glyphs stay a byte-exact body and worlds stay *labeled*
//! candidates (portfolio or explicit lock), while the strict lane refuses
//! unknown names with a typed error instead of guessing a world.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::genesis::parse_genesis;

fn check_strict(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    emath_syntax::install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

/// Fixture 04: a custom file whose body uses glyphs no strict
/// builtin knows. The custom lane parses it byte-exactly.
const ALIEN_BODY: &str = "\u{29d6}(\u{00e9} \u{22c8} e\u{0301}) \u{229b} \u{03b6}";

fn genesis_source(body: &str, answer: &str) -> String {
    format!("emath custom W:\n  body:\n  {body}\n  answer:\n  return {answer}\n")
}

#[test]
fn custom_glyphs_stay_labeled_not_strict_meaning() {
    // The custom lane keeps alien glyphs byte-exact; it never invents a
    // Real-typed interpretation for them.
    let file = parse_genesis(
        &genesis_source(ALIEN_BODY, "interpretation_portfolio"),
        &Limits::default(),
    )
    .expect("custom lane admits the alien body");
    assert_eq!(file.body_text, ALIEN_BODY, "byte-exact body preservation");
    assert_eq!(file.world_name, "W");

    // The same glyphs are NOT admissible strict-lane meaning: a strict
    // function using them as identifiers refuses with a typed error.
    let strict = check_strict(
        "strict-glyphs",
        "emath function F:\n    inputs:\n        \u{03b6}: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = \u{29d6}(\u{03b6})\n",
    );
    assert!(
        strict.diagnostics.has_errors(),
        "alien glyphs must not become strict meaning"
    );
}

#[test]
fn strict_unknown_name_never_becomes_a_world() {
    // Strict lane: an unknown callee is a typed refusal (E-TYPE-003), not
    // a silently-guessed custom world.
    let invalid = check_strict(
        "strict-unknown",
        include_str!("../../../tests/invalid/custom_lane_firewall.emath"),
    );
    assert!(
        invalid
            .diagnostics
            .errors()
            .any(|error| error.code == "E-TYPE-003"),
        "{:?}",
        invalid.diagnostics.errors().collect::<Vec<_>>()
    );
    // No guessed world: the ordinary function is not admitted with a
    // custom interpretation attached.
    assert!(
        invalid
            .package
            .declarations
            .iter()
            .all(|declaration| declaration.kind_label != "custom"),
        "a strict refusal must not surface as a custom world"
    );

    // Custom lane: the same unknown callee stays a labeled candidate bag;
    // it is interpreted structurally, never claimed.
    let file = parse_genesis(
        &genesis_source("mystery_op(x)", "interpretation_portfolio"),
        &Limits::default(),
    )
    .expect("custom lane admits unknown names as open body text");
    assert_eq!(file.body_text, "mystery_op(x)");
}

#[test]
fn refused_custom_kind_never_silently_admits() {
    // A bare `emath custom W:` body file is the genesis lane's job. The
    // strict check must refuse it (E-KIND-100) and admit NOTHING — a
    // silent custom->strict fallthrough would put a "custom" declaration
    // into the package.
    let refused = check_strict(
        "bare-custom",
        "emath custom W:\n  body:\n  ⓳(é ⋈ e´)\n  answer:\n  return r\n",
    );
    assert!(
        refused
            .diagnostics
            .errors()
            .any(|error| error.code == "E-KIND-100"),
        "{:?}",
        refused.diagnostics.errors().collect::<Vec<_>>()
    );
    assert!(
        refused.package.declarations.is_empty(),
        "a refused custom declaration must not be silently admitted into strict meaning"
    );
}
