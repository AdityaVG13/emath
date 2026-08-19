//! `emath-syntax` genesis grammar tests (migrated from
//! `crates/emath-syntax/src/genesis.rs`).

use emath_core::limits::Limits;
use emath_syntax::genesis::parse_genesis;

/// G0 exit gate: unfamiliar Unicode survives parse byte-equivalent.
/// The normalization policy is byte identity — the parser never
/// normalizes, so NFC "é" (2 bytes) and NFD "é" (3 bytes) are
/// preserved verbatim and stay distinct.
#[test]
fn unfamiliar_unicode_body_is_preserved_byte_exact() {
    let body = "\u{29d6}(\u{00e9} \u{22c8} e\u{0301}) \u{229b} \u{03b6} \u{1F702} \u{2B4D}";
    let source = format!("emath custom W:\n  body:\n  {body}\n  answer:\n  return r\n");
    let file = parse_genesis(&source, &Limits::default()).expect("exotic glyphs admitted");
    assert_eq!(file.body_text, body, "body must be byte-identical");
    assert_eq!(
        file.body_text.as_bytes(),
        body.as_bytes(),
        "no normalization: NFC and NFD sequences keep their own bytes"
    );
    assert_ne!(
        "\u{00e9}", "e\u{0301}",
        "policy premise: forms differ bytewise"
    );
}

/// Hostile input is bounded by the source limit with a typed refusal
/// (E-SYN-207), never a panic or an unbounded scan.
#[test]
fn oversized_hostile_source_is_refused_with_typed_error() {
    let limits = Limits::default();
    let body = "\u{29d6} ".repeat(limits.max_source_bytes);
    let source = format!("emath custom W:\n  body:\n  {body}\n  answer:\n  return r\n");
    let errors = parse_genesis(&source, &limits).expect_err("oversized source refused");
    assert!(
        errors.iter().any(|error| error.code == "E-SYN-207"),
        "expected E-SYN-207 source-limit refusal, got {errors:?}"
    );
}

#[test]
fn max_tokens_is_a_token_budget_not_a_line_count() {
    let source = "emath custom W:
  body:
  a b c d e f
  answer:
  return r";
    let admitted = parse_genesis(source, &Limits::default());
    assert!(
        admitted.is_ok(),
        "full-budget parse must admit the fixture; errors: {admitted:?}"
    );
    // The same file carries 15 tokens; a budget of 8 must cut the scan
    // before `answer:`, so the missing-answer refusal fires. A line
    // count of 5 would keep everything and admit.
    let limits = Limits {
        max_tokens: 8,
        ..Limits::default()
    };
    let refused = parse_genesis(source, &limits);
    assert!(
        refused.is_err(),
        "token budget must stop the scan before `answer:`, got {refused:?}"
    );
}
