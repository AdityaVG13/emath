//! `emath-syntax` genesis grammar tests (migrated from
//! `crates/emath-syntax/src/genesis.rs`).

use emath_core::limits::Limits;
use emath_syntax::genesis::parse_genesis;

use emath_test_harness::{Probe, boot};

#[test]
fn genesis() {
    boot();
    let mut probe = Probe::new("`emath-syntax` genesis grammar tests (migrated from `crates/emath-syntax/src/genesis.rs`).");
    probe.case("unfamiliar_unicode_body_is_preserved_byte_exact", |p| {
    // G0 exit gate: unfamiliar Unicode survives parse byte-equivalent.
    // The normalization policy is byte identity — the parser never
    // normalizes, so NFC "é" (2 bytes) and NFD "é" (3 bytes) are
    // preserved verbatim and stay distinct.

    let body = "\u{29d6}(\u{00e9} \u{22c8} e\u{0301}) \u{229b} \u{03b6} \u{1F702} \u{2B4D}";
    let source = format!("emath custom W:\n  body:\n  {body}\n  answer:\n  return r\n");
    let file = parse_genesis(&source, &Limits::default()).expect("exotic glyphs admitted");
    p.eq("1", file.body_text.as_str(), body);
    p.eq("2", file.body_text.as_bytes(), body.as_bytes());
    p.ne("3", "\u{00e9}", "e\u{0301}");

    });
    probe.case("oversized_hostile_source_is_refused_with_typed_error", |p| {
    // Hostile input is bounded by the source limit with a typed refusal
    // (E-SYN-207), never a panic or an unbounded scan.
    let f0 = p.failures().len();

    let limits = Limits::default();
    let body = "\u{29d6} ".repeat(limits.max_source_bytes);
    let source = format!("emath custom W:\n  body:\n  {body}\n  answer:\n  return r\n");
    let errors = parse_genesis(&source, &limits).expect_err("oversized source refused");
    p.demand("1",errors.iter().any(|error| error.code == "E-SYN-207"), format!(
        "expected E-SYN-207 source-limit refusal, got {errors:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("max_tokens_is_a_token_budget_not_a_line_count", |p| {
    let f0 = p.failures().len();

    let source = "emath custom W:
  body:
  a b c d e f
  answer:
  return r";
    let admitted = parse_genesis(source, &Limits::default());
    p.demand("1",admitted.is_ok(), format!(
        "full-budget parse must admit the fixture; errors: {admitted:?}"
    ));
    if p.failures().len() != f0 { return; }
    // The same file carries 15 tokens; a budget of 8 must cut the scan
    // before `answer:`, so the missing-answer refusal fires. A line
    // count of 5 would keep everything and admit.
    let limits = Limits {
        max_tokens: 8,
        ..Limits::default()
    };
    let refused = parse_genesis(source, &limits);
    p.demand("2",refused.is_err(), format!(
        "token budget must stop the scan before `answer:`, got {refused:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
