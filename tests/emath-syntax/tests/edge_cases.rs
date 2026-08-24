//! Edge-case regressions: empty / extreme / Unicode lexer + source map,
//! and parser edge cases (F5 non-greedy derivative operand).

use emath_core::limits::Limits;
use emath_core::tree::{ExprKind, BinaryOp, StmtKind};
use emath_core::{Diagnostic, FileId, SourceStore, Span};
use emath_syntax::lexer::lex;
use emath_syntax::token::TokenKind;
use emath_syntax::parse_str;

#[test]
fn empty_source_lexes_only_eof() {
    let (tokens, diagnostics) = lex("", FileId(0), &Limits::default());
    assert!(!diagnostics.has_errors());
    assert_eq!(tokens.len(), 1);
    assert!(matches!(tokens[0].kind, TokenKind::Eof));
}

#[test]
fn invalid_unicode_escape_keeps_string_and_following_ident() {
    // `\u` without `{` used to `break` the string loop, emit a truncated Str,
    // and retokenize `x"` as Ident + junk. Recovery must stay in the string.
    let (tokens, diagnostics) = lex(r#""\ux" after"#, FileId(0), &Limits::default());
    assert!(
        diagnostics.errors().any(|error| error.code == "E-SYN-109"),
        "expected E-SYN-109 for malformed \\u"
    );
    let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
    assert!(
        kinds.iter().any(|kind| matches!(kind, TokenKind::Str(_))),
        "still emits one Str token: {kinds:?}"
    );
    assert!(
        kinds
            .iter()
            .any(|kind| matches!(kind, TokenKind::Ident(name) if name == "after")),
        "tail after the closing quote must lex as Ident(after), got {kinds:?}"
    );
}

#[test]
fn oversized_unicode_escape_hex_is_diagnosed() {
    // Hex wider than u32 used to be silently skipped (no diagnostic, no char).
    let (tokens, diagnostics) = lex(r#""\u{100000000}""#, FileId(0), &Limits::default());
    assert!(
        diagnostics.errors().any(|error| error.code == "E-SYN-109"),
        "expected E-SYN-109 for overflow hex"
    );
    let value = tokens
        .iter()
        .find_map(|token| match &token.kind {
            TokenKind::Str(value) => Some(value.as_str()),
            _ => None,
        })
        .expect("expected a Str token");
    assert!(
        value.is_empty(),
        "overflow escape must not invent a char, got {value:?}"
    );
}

#[test]
fn empty_unicode_escape_is_diagnosed() {
    let (_, diagnostics) = lex(r#""\u{}""#, FileId(0), &Limits::default());
    assert!(diagnostics.errors().any(|error| error.code == "E-SYN-109"));
}

#[test]
fn line_col_max_offset_does_not_overflow() {
    let mut store = SourceStore::new();
    let id = store.add("t.emath", "");
    let file = store.get(id).expect("file");
    let (line, col) = file.line_col(u32::MAX);
    assert_eq!(line, 1);
    assert_eq!(col, u32::MAX);
}

#[test]
fn token_limit_does_not_emit_unbounded_trailing_dedents() {
    let limits = Limits {
        max_tokens: 4,
        ..Limits::default()
    };
    // Many successively deeper indents after the budget is spent must not
    // append one Dedent per phantom stack frame past max_tokens (+Eof).
    let source = "a\n b\n  c\n   d\n    e\n     f\n      g\n";
    let (tokens, diagnostics) = lex(source, FileId(0), &limits);
    assert!(
        diagnostics.errors().any(|error| error.code == "E-SYN-108"),
        "expected token-limit diagnostic"
    );
    let dedents = tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::Dedent))
        .count();
    assert!(
        tokens.len() <= limits.max_tokens + 1,
        "tokens={} exceeds max_tokens+Eof; dedents={dedents}",
        tokens.len()
    );
}

#[test]
fn caret_aligns_after_multibyte_prefix() {
    let mut store = SourceStore::new();
    // `α` is two UTF-8 bytes; error on the ASCII `x` that follows.
    let id = store.add("t.emath", "αx");
    let file = store.get(id).expect("file");
    let diagnostic = Diagnostic::error(
        "E-TEST",
        "here",
        Span::new(id, 2, 3), // byte offset of `x`
    );
    let rendered = file.render_diagnostic(&diagnostic);
    let caret_line = rendered
        .lines()
        .find(|line| line.contains('^'))
        .expect("caret line");
    // Renderer prefix is two spaces; one more space for the scalar `α`.
    assert_eq!(
        caret_line, "   ^",
        "caret must sit under `x` (1 scalar indent), got {rendered:?}"
    );
}

#[test]
fn source_over_byte_limit_refuses_without_scanning() {
    let limits = Limits {
        max_source_bytes: 8,
        ..Limits::default()
    };
    let source = "aaaaaaaaaa"; // 10 bytes > 8
    let (tokens, diagnostics) = lex(source, FileId(0), &limits);
    assert!(
        diagnostics.errors().any(|error| error.code == "E-SYN-116"),
        "expected E-SYN-116 for oversized source"
    );
    assert_eq!(tokens.len(), 1, "oversized source must emit only Eof");
    assert!(matches!(tokens[0].kind, TokenKind::Eof));
}

#[test]
fn diagnostic_message_newlines_cannot_inject_frames() {
    let mut store = SourceStore::new();
    let id = store.add("t.emath", "x");
    let file = store.get(id).expect("file");
    let mut diagnostic = Diagnostic::error(
        "E-TEST",
        "first\ninjected:1:1: E-FAKE: second",
        Span::new(id, 0, 1),
    );
    diagnostic.help = Some("help\nline".into());
    let rendered = file.render_diagnostic(&diagnostic);
    let header = rendered.lines().next().expect("header line");
    assert!(
        !header.contains('\n') && header.contains("first injected:1:1: E-FAKE: second"),
        "message controls must flatten into one header line, got {rendered:?}"
    );
    assert!(
        rendered.contains("= help: help line"),
        "help controls must flatten, got {rendered:?}"
    );
}

// ---- F5: non-greedy derivative operand -------------------------------------

/// Find the expression bound to `name` inside a declaration's
/// `definitions:` section.  Handles both `Let` and `Assign` statement kinds.
fn def_expr<'a>(tree: &'a emath_core::tree::SyntaxTree, name: &str) -> Option<&'a emath_core::tree::Expr> {
    let item = tree.items.first()?;
    let emath_core::tree::Item::Declaration(decl) = item else {
        return None;
    };
    for section in decl.sections() {
        if section.name == "definitions" {
            for stmt in &section.suite.statements {
                match &stmt.kind {
                    StmtKind::Let { name: n, value, .. } if n == name => return Some(value),
                    StmtKind::Assign { target, value } if target.segments.first().is_some_and(|s| s == name) => return Some(value),
                    _ => {}
                }
            }
        }
    }
    None
}

#[test]
fn f5_derivative_plus_does_not_greedily_consume() {
    // `derivative(v) + v` must parse as `(derivative v) + v`,
    // not `derivative(v + v)`.
    let source = "\
emath function f(v: Float64) -> Float64:
    definitions:
        result = derivative(v) + v
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "must parse cleanly, got errors: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    // Top level should be Binary(Add, Derivative(v), v)
    let ExprKind::Binary { op, left, right } = &expr.kind else {
        panic!("expected Binary at top level, got {:?}", expr.kind);
    };
    assert_eq!(*op, BinaryOp::Add, "top-level operator should be Add");
    assert!(
        matches!(&left.kind, ExprKind::Derivative { .. }),
        "left side should be Derivative, got {:?}",
        left.kind
    );
    assert!(
        matches!(&right.kind, ExprKind::Path { .. }),
        "right side should be the bare identifier v, got {:?}",
        right.kind
    );
}

#[test]
fn f5_derivative_parenthesised_operand_still_works() {
    // `derivative(v + v)` must still parse as derivative of the sum.
    let source = "\
emath function f(v: Float64) -> Float64:
    definitions:
        result = derivative(v + v)
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "must parse cleanly, got errors: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    let ExprKind::Derivative { value, wrt } = &expr.kind else {
        panic!("expected Derivative at top level, got {:?}", expr.kind);
    };
    assert!(
        matches!(value.kind, ExprKind::Binary { op: BinaryOp::Add, .. }),
        "operand should be v + v, got {:?}",
        value.kind
    );
    assert!(wrt.is_none());
}

#[test]
fn f5_derivative_wrt_still_attaches() {
    // `derivative(y) wrt x` must still attach the wrt clause.
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        y = x * x
        dy = derivative(y) wrt x
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "must parse cleanly, got errors: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "dy").expect("expected `dy` binding");
    let ExprKind::Derivative { wrt, .. } = &expr.kind else {
        panic!("expected Derivative, got {:?}", expr.kind);
    };
    assert!(wrt.is_some(), "wrt must be attached");
}
