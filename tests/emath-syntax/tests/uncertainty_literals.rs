//! `emath-r3-uncertainty-literals-jzej`: measurement literal forms (spec 04
//! section 1.5).
//!
//! Two written forms get literal status:
//! - explicit: `1.50 ± 0.02 m` (X6: `±` in core IS the measurement literal;
//!   algebraic plus-minus lives only in the opt-in algebra pack);
//! - parenthetical (CODATA): `0.5012(3)` = 0.5012 ± 0.0003 — digits attach to
//!   the last mantissa digits, immediately, no space, digits only.
//!
//! Attached-only rule: `f(2)` stays a call; `1.50 (2)` (space) never lexes as
//! an uncertainty. Provenance defaults to `Unstated` and prints loudly.
//!
//! Lowering seam (design notes): no IR enum change — `Measured<T>`,
//! `DistributionKind`, and `Provenance::Unstated` are on HEAD; the literal
//! lowers through the `core::measure::Measured` constructor path.

use emath_core::limits::Limits;
use emath_core::{FileId};
use emath_syntax::lexer::lex;
use emath_syntax::token::TokenKind;

fn tokens_of(source: &str) -> Vec<TokenKind> {
    let (tokens, _) = lex(source, FileId(0), &Limits::default());
    tokens.into_iter().map(|token| token.kind).collect()
}

fn codes_of(source: &str) -> Vec<String> {
    let (_, diagnostics) = lex(source, FileId(0), &Limits::default());
    diagnostics
        .errors()
        .map(|error| error.code.to_string())
        .collect()
}

#[test]
fn explicit_plus_minus_literal_tokenizes() {
    // `1.50 ± 0.02` must lex as Float `±` Float, no diagnostics. At HEAD the
    // `±` glyph routes into the non-ASCII ident path and the sequence is not
    // a measurement literal — this pin fails until the lexer lands.
    let kinds = tokens_of("1.50 ± 0.02");
    assert!(
        matches!(
            kinds.as_slice(),
            [TokenKind::Float(a), TokenKind::PlusMinus, TokenKind::Float(b), TokenKind::Eof]
                if a == "1.50" && b == "0.02"
        ),
        "`1.50 ± 0.02` must be Float ± Float, got {kinds:?}"
    );
    assert!(
        codes_of("1.50 ± 0.02").is_empty(),
        "measurement literal must lex cleanly"
    );
}

#[test]
fn parenthetical_uncertainty_tokenizes_when_attached() {
    // `0.5012(3)`: one attached uncertainty token; the uncertainty digits are
    // preserved raw (decimal-place scaling is admission's job).
    let kinds = tokens_of("0.5012(3)");
    assert!(
        matches!(
            kinds.as_slice(),
            [TokenKind::FloatUncertainty { number, digits }, TokenKind::Eof]
                if number == "0.5012" && digits == "3"
        ),
        "`0.5012(3)` must be one attached uncertainty literal, got {kinds:?}"
    );
}

#[test]
fn codata_uncertainty_tokenizes_with_exponent() {
    // CODATA G: `6.67430(15)e-11` — uncertainty digits + exponent in one
    // literal token; admission scales ±0.0015 by e-11.
    let kinds = tokens_of("6.67430(15)e-11");
    assert!(
        matches!(
            kinds.as_slice(),
            [TokenKind::FloatUncertainty { number, digits }, TokenKind::Eof]
                if number == "6.67430e-11" && digits == "15"
        ),
        "CODATA form must keep exponent in the number spelling, got {kinds:?}"
    );
}

#[test]
fn distribution_tag_tokenizes() {
    // `~ normal` distribution tag: bare `~` becomes Tilde followed by the
    // distribution name (HEAD refuses bare `~` with E-SYN-101).
    let kinds = tokens_of("0.62 ± 0.01 ~ lognormal");
    assert!(
        matches!(
            kinds.as_slice(),
            [
                TokenKind::Float(_),
                TokenKind::PlusMinus,
                TokenKind::Float(_),
                TokenKind::Tilde,
                TokenKind::Ident(name),
                TokenKind::Eof
            ] if name == "lognormal"
        ),
        "distribution tag must be `~ name`, got {kinds:?}"
    );
}

#[test]
fn call_suffix_stays_a_call() {
    // Regression guard (already true at HEAD): `f(2)` never attaches as an
    // uncertainty — the leading token is an Ident, not a number.
    let kinds = tokens_of("f(2)");
    assert!(matches!(
        kinds.as_slice(),
        [TokenKind::Ident(f), TokenKind::LParen, TokenKind::Int(n), TokenKind::RParen, TokenKind::Eof]
            if f == "f" && n == "2"
    ));
}

#[test]
fn space_before_parenthesis_never_attaches_uncertainty() {
    // `1.50 (2)` (space): the lexer must NOT attach; parser-level refusal is
    // Pinned by tests/invalid/uncertainty_parenthetical_spacing.emath.
    let kinds = tokens_of("1.50 (2)");
    assert!(matches!(
        kinds.as_slice(),
        [TokenKind::Float(_), TokenKind::LParen, TokenKind::Int(_), TokenKind::RParen, TokenKind::Eof]
    ));
}

#[test]
fn parenthetical_uncertainty_requires_digits_only() {
    // `1.5(x)` is not an uncertainty (non-digit inside) — the `(` lexes as a
    // group opener, leaving the attached-form refusal to the parser lane.
    let kinds = tokens_of("1.5(x)");
    assert!(matches!(
        kinds.as_slice(),
        [TokenKind::Float(_), TokenKind::LParen, TokenKind::Ident(_), TokenKind::RParen, TokenKind::Eof]
    ));
    // Empty parenthetical `1.5()` is likewise not an uncertainty: zero
    // uncertainty digits carry no measurement content.
    let kinds = tokens_of("1.5()");
    assert!(matches!(
        kinds.as_slice(),
        [TokenKind::Float(_), TokenKind::LParen, TokenKind::RParen, TokenKind::Eof]
    ));
}

// ---- slice 2: parse-level literal fold (spec 04 section 1.5) --------------

use emath_core::tree::{ExprKind, StmtKind};
use emath_syntax::parse_str;

fn def_expr_of(source: &str) -> ExprKind {
    let (tree, diags) = parse_str(source);
    assert!(!diags.has_errors(), "{diags:?}");
    let Some(emath_core::tree::Item::Declaration(decl)) = tree.items.first() else {
        panic!("declaration expected");
    };
    let Some(stmt) = decl.body.iter().find(|stmt| {
        matches!(&stmt.kind, StmtKind::Section(s) if s.name == "definitions")
    }) else {
        panic!("definitions section expected");
    };
    let StmtKind::Section(definitions) = &stmt.kind else {
        unreachable!()
    };
    match &definitions.suite.statements[0].kind {
        StmtKind::Assign { value, .. } => value.kind.clone(),
        other => panic!("assignment expected, got {other:?}"),
    }
}

#[test]
fn explicit_plus_minus_folds_to_measured_value() {
    // `1.50 ± 0.02` folds into one Measured literal carrying both spellings;
    // provenance is Unstated by default (admission prints it loudly).
    let expr = def_expr_of(
        "emath function f:\n    definitions:\n        m = 1.50 ± 0.02\n",
    );
    assert!(
        matches!(
            &expr,
            ExprKind::Measured { value, uncertainty, uncertainty_digits, distribution }
                if value == "1.50" && uncertainty == "0.02" && uncertainty_digits.is_empty() && distribution.is_none()
        ),
        "explicit ± form must fold to Measured, got {expr:?}"
    );
}

#[test]
fn parenthetical_uncertainty_preserves_raw_digits() {
    // `0.5012(3)`: value keeps the mantissa, digits stay raw — scaling to a
    // decimal uncertainty is admission's job (0.5012 ± 0.0003).
    let expr = def_expr_of(
        "emath function f:\n    definitions:\n        m = 0.5012(3)\n",
    );
    assert!(
        matches!(
            &expr,
            ExprKind::Measured { value, uncertainty_digits, .. }
                if value == "0.5012" && uncertainty_digits == "3"
        ),
        "parenthetical form must fold to Measured with raw digits, got {expr:?}"
    );
}

#[test]
fn codata_uncertainty_folds_exponent_into_value() {
    // CODATA G: `6.67430(15)e-11` — exponent stays in the value spelling;
    // admission scales ±digits by 10^exp (±0.00015e-11 = 1.5e-15).
    let expr = def_expr_of(
        "emath function f:\n    definitions:\n        g = 6.67430(15)e-11\n",
    );
    assert!(
        matches!(
            &expr,
            ExprKind::Measured { value, uncertainty_digits, .. }
                if value == "6.67430e-11" && uncertainty_digits == "15"
        ),
        "CODATA form must keep the exponent in the value, got {expr:?}"
    );
}

#[test]
fn distribution_tag_folds() {
    // `0.62 ± 0.01 ~ lognormal`: the tag rides the same Measured literal.
    let expr = def_expr_of(
        "emath function f:\n    definitions:\n        m = 0.62 ± 0.01 ~ lognormal\n",
    );
    assert!(
        matches!(
            &expr,
            ExprKind::Measured { distribution: Some(name), .. } if name == "lognormal"
        ),
        "distribution tag must fold into the literal, got {expr:?}"
    );
}

#[test]
fn call_suffix_still_parses_as_call() {
    // Regression at parse level: `f(2)` is a call; a FloatUncertainty only
    // exists as its own token, so no call-shaped source can fold to Measured.
    let expr = def_expr_of(
        "emath function f:\n    inputs:\n        x: Float64\n    definitions:\n        y = f(2)\n",
    );
    assert!(
        matches!(&expr, ExprKind::Call { .. }),
        "f(2) must remain a call, got {expr:?}"
    );
}

// ---- slice 2: admission-level pins (E-MEAS codes) -------------------------

use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check_source(source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned("r3-admission", source)
}

fn source_with(def: &str) -> String {
    format!("emath function f:\n    definitions:\n        {def}\n")
}

#[test]
fn codata_uncertainty_scales_to_expected_exponent() {
    // CODATA G: 15 digits on 6.67430e-11 → ±15 × 10^(−11−5) = 1.5e-15.
    // The receipt carries the scaled uncertainty (mutation target: the
    // 10^(exp−frac) exponent — off-by-one kills this pin).
    let checked = check_source(&source_with("g = 6.67430(15)e-11\n"));
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let receipt = checked
        .diagnostics
        .items()
        .iter()
        .filter(|diagnostic| diagnostic.code == "E-MEAS-003")
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        receipt.contains("1.5e-15"),
        "E-MEAS-003 receipt must carry the scaled uncertainty 1.5e-15, got: {receipt}"
    );
}

#[test]
fn unknown_distribution_tag_refuses() {
    // `~ gaussian` is not in the tag vocabulary; refusing typed (never a
    // silent default to normal — a wrong distribution is a wrong claim).
    let checked = check_source(&source_with("m = 0.62 ± 0.01 ~ gaussian\n"));
    assert!(
        checked
            .diagnostics
            .errors()
            .any(|error| error.code == "E-MEAS-002"),
        "unknown distribution tag must refuse E-MEAS-002, got {:?}",
        checked.diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}
