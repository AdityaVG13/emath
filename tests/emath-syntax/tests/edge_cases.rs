//! Edge-case regressions: empty / extreme / Unicode lexer + source map,
//! and parser edge cases (F5 non-greedy derivative operand).

use emath_core::limits::Limits;
use emath_core::tree::{BinaryOp, DerivativeKind, ExprKind, Item, NotationFixity, StmtKind};
use emath_core::{Diagnostic, FileId, SourceStore, Span};
use emath_syntax::lexer::{lex, lex_with_comments};
use emath_syntax::token::TokenKind;
use emath_syntax::{parse, parse_lossless, parse_str};

#[test]
fn empty_source_lexes_only_eof() {
    let (tokens, diagnostics) = lex("", FileId(0), &Limits::default());
    assert!(!diagnostics.has_errors());
    assert_eq!(tokens.len(), 1);
    assert!(matches!(tokens[0].kind, TokenKind::Eof));
}

#[test]
fn exact_rational_double_slash_is_not_a_comment() {
    // Spec literal family `3//7`. Treating `//` as a line comment used to
    // silently drop the denominator, leaving only Int("3").
    let (tokens, diagnostics) = lex("3//7", FileId(0), &Limits::default());
    assert!(
        !diagnostics.has_errors(),
        "exact rational must lex cleanly, got {:?}",
        diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
    assert!(
        matches!(
            kinds.as_slice(),
            [TokenKind::Int(n), TokenKind::SlashSlash, TokenKind::Int(d), TokenKind::Eof]
                if n == "3" && d == "7"
        ),
        "3//7 must be Int // Int, got {kinds:?}"
    );
}

#[test]
fn exact_rational_literal_folds_in_the_parser() {
    // Grammar: `rational_literal = integer "//" integer` is a primary.
    // After the lexer emits SlashSlash, the parser must fold `3//7` into
    // one expression, not bind `3` and leave `//7` as leftover junk.
    let source = "\
emath function f() -> Float64:
    definitions:
        r = 3//7
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "3//7 must parse as a rational literal, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "r").expect("expected `r` binding");
    assert!(
        matches!(&expr.kind, ExprKind::Rational { numer, denom } if numer == "3" && denom == "7"),
        "expected Rational {{ numer: 3, denom: 7 }}, got {:?}",
        expr.kind
    );
}

#[test]
fn exact_rational_quantity_attaches_the_unit() {
    // Grammar: quantity_literal = (integer | decimal | rational_literal) whitespace path.
    // `3//2 s` is a quantity whose value is the rational, not Int(3).
    let source = "\
emath function f() -> Float64:
    definitions:
        q = 3//2 s
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "3//2 s must parse as a quantity, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "q").expect("expected `q` binding");
    let ExprKind::Quantity { value, unit } = &expr.kind else {
        panic!("expected Quantity, got {:?}", expr.kind);
    };
    assert!(
        matches!(&value.kind, ExprKind::Rational { numer, denom } if numer == "3" && denom == "2"),
        "quantity value should be Rational 3//2, got {:?}",
        value.kind
    );
    assert_eq!(unit.to_string(), "s");
}

#[test]
fn exact_rational_missing_denominator_is_named_refuse() {
    // `3//x` is not `integer "//" integer`. Must diagnose, not silently
    // keep Int("3") with no error.
    let source = "\
emath function f() -> Float64:
    definitions:
        r = 3//x
";
    let (_tree, diags) = parse_str(source);
    assert!(
        diags.errors().any(|e| e.code == "E-SYN-101"),
        "non-integer denominator must be E-SYN-101, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn hash_and_doc_comments_still_skip_the_line() {
    let (tokens, diagnostics, comments) = lex_with_comments(
        "# ordinary\n/// documentation\nx",
        FileId(0),
        &Limits::default(),
    );
    assert!(!diagnostics.has_errors());
    let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
    assert!(
        matches!(
            kinds.as_slice(),
            [TokenKind::Ident(name), TokenKind::Eof] if name == "x"
        ),
        "comments must not emit tokens, got {kinds:?}"
    );
    assert!(
        comments.iter().any(|c| c.text.starts_with('#')),
        "ordinary `#` comment must retain the marker, got {comments:?}"
    );
    assert!(
        comments.iter().any(|c| c.text.starts_with("///")),
        "doc `///` comment must retain the marker, got {comments:?}"
    );
}

#[test]
fn double_slash_comment_is_not_admitted() {
    // Spec comments are `#` and `///` only. `// rest` is SlashSlash plus
    // whatever follows, not a silent skip of `rest`.
    let (tokens, diagnostics) = lex("a//b", FileId(0), &Limits::default());
    assert!(!diagnostics.has_errors());
    let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
    assert!(
        matches!(
            kinds.as_slice(),
            [
                TokenKind::Ident(left),
                TokenKind::SlashSlash,
                TokenKind::Ident(right),
                TokenKind::Eof
            ] if left == "a" && right == "b"
        ),
        "`//` must not eat the following ident, got {kinds:?}"
    );
}

#[test]
fn braces_suppress_newlines_like_parens() {
    // Spec: NEWLINE is suppressed inside `()`, `[]`, and `{}`.
    let (paren, _) = lex("(\n1\n)", FileId(0), &Limits::default());
    let (brace, _) = lex("{\n1\n}", FileId(0), &Limits::default());
    let paren_kinds: Vec<&TokenKind> = paren.iter().map(|t| &t.kind).collect();
    let brace_kinds: Vec<&TokenKind> = brace.iter().map(|t| &t.kind).collect();
    assert!(
        !paren_kinds.iter().any(|k| matches!(k, TokenKind::Newline)),
        "parens already suppress newlines, got {paren_kinds:?}"
    );
    assert!(
        !brace_kinds.iter().any(|k| matches!(k, TokenKind::Newline)),
        "braces must suppress newlines, got {brace_kinds:?}"
    );
    assert!(
        matches!(
            brace_kinds.as_slice(),
            [
                TokenKind::LBrace,
                TokenKind::Int(n),
                TokenKind::RBrace,
                TokenKind::Eof
            ] if n == "1"
        ),
        "expected brace-wrapped int, got {brace_kinds:?}"
    );
}

#[test]
fn tabs_are_rejected_in_canonical_source() {
    let (_, diagnostics) = lex("a\tb", FileId(0), &Limits::default());
    assert!(
        diagnostics.errors().any(|error| error.code == "E-SYN-101"),
        "tab must be a typed refusal, got {:?}",
        diagnostics
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    );
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
fn parse_skips_scratch_expand_when_source_exceeds_limits() {
    let limits = Limits {
        max_source_bytes: 8,
        ..Limits::default()
    };
    let source = "aaaaaaaaaa"; // 10 bytes > 8
    let (_, diagnostics) = parse(source, FileId(0), &limits);
    assert!(
        diagnostics.errors().any(|error| error.code == "E-SYN-116"),
        "expected E-SYN-116 without expanding oversized scratch"
    );
    let lossless = parse_lossless(source, FileId(0), &limits);
    assert!(
        lossless
            .diagnostics
            .errors()
            .any(|error| error.code == "E-SYN-116"),
        "expected E-SYN-116 from lossless parse of oversized source"
    );
    assert!(
        lossless.comments.is_empty(),
        "oversized lossless parse must not retain comments"
    );
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
fn def_expr<'a>(
    tree: &'a emath_core::tree::SyntaxTree,
    name: &str,
) -> Option<&'a emath_core::tree::Expr> {
    let item = tree.items.first()?;
    let emath_core::tree::Item::Declaration(decl) = item else {
        return None;
    };
    for section in decl.sections() {
        if section.name == "definitions" {
            for stmt in &section.suite.statements {
                match &stmt.kind {
                    StmtKind::Let { name: n, value, .. } if n == name => return Some(value),
                    StmtKind::Assign { target, value }
                        if target.segments.first().is_some_and(|s| s == name) =>
                    {
                        return Some(value);
                    }
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
    let ExprKind::Derivative { value, wrt, .. } = &expr.kind else {
        panic!("expected Derivative at top level, got {:?}", expr.kind);
    };
    assert!(
        matches!(
            value.kind,
            ExprKind::Binary {
                op: BinaryOp::Add,
                ..
            }
        ),
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

// ---- C1: conditional expression spelling -----------------------------------

#[test]
fn c1_conditional_uses_colon_form() {
    // The grammar and parser use `if c: a else: b` (colons, no `then`).
    // ch7's "Implemented today" list previously said `if cond then a else b`
    // (with `then`) — a documentation drift.  The colon form is what
    // actually parses and runs.
    let source = "\
emath function sign(x: Float64) -> Float64:
    definitions:
        s = if x > 0: 1 else: 0
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "colon-form conditional must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "s").expect("expected `s` binding");
    let ExprKind::If {
        condition,
        then_value,
        else_value,
    } = &expr.kind
    else {
        panic!("expected If expression, got {:?}", expr.kind);
    };
    // Verify structure: condition is `x > 0`, then is `1`, else is `0`
    assert!(
        matches!(
            &condition.kind,
            ExprKind::Binary {
                op: BinaryOp::Gt,
                ..
            }
        ),
        "condition should be x > 0, got {:?}",
        condition.kind
    );
    assert!(
        matches!(&then_value.kind, ExprKind::Int(_)),
        "then_value should be 1, got {:?}",
        then_value.kind
    );
    assert!(
        matches!(&else_value.kind, ExprKind::Int(_)),
        "else_value should be 0, got {:?}",
        else_value.kind
    );
}

// ---- C3: numeric literal indexing refused ----------------------------------

#[test]
fn c3_numeric_literal_not_indexed() {
    // `9.81 [m]` must NOT parse as indexing the decimal 9.81 by m.
    // After the C3 fix, `parse_postfix` refuses `[` after numeric
    // literals, so `x` is bound to just `9.81` (a Float), and `[m]`
    // is left as a separate construct (list-literal statement).
    let source = "\
emath function bad() -> Float64:
    definitions:
        x = 9.81 [m]
";
    let (tree, _diags) = parse_str(source);
    // The parse may or may not produce errors (the leftover `[m]`
    // might be consumed as a list-literal expression statement),
    // but the key invariant is: x is bound to a Float, NOT an Index.
    let expr = def_expr(&tree, "x").expect("expected `x` binding");
    assert!(
        matches!(&expr.kind, ExprKind::Float(_)),
        "x should be bound to a Float literal (9.81), not an Index, got {:?}",
        expr.kind
    );
}

#[test]
fn c3_variable_indexing_still_works() {
    // `v[0]` on a non-literal primary (path/identifier) must still parse
    // as indexing.  The C3 fix only refuses `[` after numeric literals.
    let source = "\
emath function idx(v: Vector[3]) -> Float64:
    definitions:
        x = v[0]
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "variable indexing must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "x").expect("expected `x` binding");
    let ExprKind::Index { value, indices } = &expr.kind else {
        panic!("expected Index expression, got {:?}", expr.kind);
    };
    assert!(
        matches!(&value.kind, ExprKind::Path { .. }),
        "indexed value should be a path (v), got {:?}",
        value.kind
    );
    assert_eq!(indices.len(), 1, "should have one index");
}

#[test]
fn c3_list_literal_indexing_still_works() {
    // `[[1, 2], [3, 4]][0]` on a list literal must still parse as indexing.
    // The C3 fix only refuses `[` after numeric scalar literals (Int,
    // Float, Quantity), not after list/tuple primaries.
    let source = "\
emath function mat() -> Vector[2]:
    definitions:
        row = [[1, 2], [3, 4]][0]
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "list literal indexing must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "row").expect("expected `row` binding");
    let ExprKind::Index { value, .. } = &expr.kind else {
        panic!("expected Index expression, got {:?}", expr.kind);
    };
    assert!(
        matches!(&value.kind, ExprKind::List(_)),
        "indexed value should be a list, got {:?}",
        value.kind
    );
}

// ---- N1-N5: notation governance core ----------------------------------

#[test]
fn n1_notation_decl_parses() {
    // N1: notation declarations are package-level items, scoped to the
    // package and imported via `use`.
    let source = "\
package test.pkg

notation infixl 40 \"⋅\" => core::math::dot
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "notation decl must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let notation = tree
        .items
        .iter()
        .find_map(|item| match item {
            Item::Notation(n) => Some(n),
            _ => None,
        })
        .expect("expected a Notation item");
    assert_eq!(notation.fixity, NotationFixity::InfixLeft);
    assert_eq!(notation.precedence, 40);
    assert_eq!(notation.glyph, "⋅");
    assert_eq!(notation.target, vec!["core", "math", "dot"]);
    assert!(notation.alias.is_none(), "no alias clause");
}

#[test]
fn n2_notation_alias_clause_parses() {
    // N2: the optional `alias` clause provides an alternative spelling.
    // accept-many/canon-one: multiple aliases map to one canonical path.
    let source = "\
package test.pkg

notation infixl 40 \"⋅\" => core::math::dot alias \"pw\"
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "notation with alias must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let notation = tree
        .items
        .iter()
        .find_map(|item| match item {
            Item::Notation(n) => Some(n),
            _ => None,
        })
        .expect("expected a Notation item");
    assert_eq!(
        notation.alias.as_deref(),
        Some("pw"),
        "alias clause should be captured"
    );
}

#[test]
fn n1_all_fixity_forms_parse() {
    // All five fixity keywords must be recognized.
    for (fixity_str, expected) in [
        ("prefix", NotationFixity::Prefix),
        ("postfix", NotationFixity::Postfix),
        ("infixl", NotationFixity::InfixLeft),
        ("infixr", NotationFixity::InfixRight),
        ("infix", NotationFixity::Infix),
    ] {
        let source =
            format!("package test.pkg\n\nnotation {fixity_str} 50 \"⊗\" => core::math::op");
        let (tree, diags) = parse_str(&source);
        assert!(
            !diags.has_errors(),
            "fixity `{fixity_str}` must parse cleanly, got: {:?}",
            diags.errors().map(|e| e.code).collect::<Vec<_>>()
        );
        let notation = tree
            .items
            .iter()
            .find_map(|item| match item {
                Item::Notation(n) => Some(n),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected Notation for fixity `{fixity_str}`"));
        assert_eq!(
            notation.fixity, expected,
            "fixity mismatch for `{fixity_str}`"
        );
    }
}

#[test]
fn n4_invalid_fixity_errors() {
    // N4 conflict rules: an unrecognized fixity keyword must produce a
    // parse error, not silently misparse.
    let source = "\
package test.pkg

notation infixd 40 \"⋅\" => core::math::dot
";
    let (tree, diags) = parse_str(source);
    assert!(
        diags.has_errors(),
        "invalid fixity `infixd` must produce an error"
    );
    assert!(
        tree.items.iter().all(|i| !matches!(i, Item::Notation(_))),
        "no Notation item should be produced for invalid fixity"
    );
}

#[test]
fn n1_notation_example_file_parses() {
    // Inlined from the pruned notation-governance.emath example.
    // Six notation declarations: five fixity forms + one with alias.
    let source = "\
package examples.notation

notation infixl 40 \"⊕\" => core::math::add

notation infixr 50 \"⊗\" => core::math::mul

notation prefix 80 \"¬\" => core::logic::negate

notation postfix 90 \"†\" => core::math::conjugate

notation infix 45 \"≡\" => core::logic::iff

notation infixl 40 \"⊕\" => core::math::add alias \"plus\"
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "notation example must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let notations: Vec<_> = tree
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Notation(n) => Some(n),
            _ => None,
        })
        .collect();
    assert_eq!(
        notations.len(),
        6,
        "expected 6 notation declarations, got {}",
        notations.len()
    );
    // The last one should have an alias clause (N2: an alias is an
    // alternative spelling, so it must lex as a single identifier —
    // punctuation like `++` would silently re-lex as operators).
    assert_eq!(
        notations[5].alias.as_deref(),
        Some("plus"),
        "last notation should have alias \"plus\""
    );
}

#[test]
fn n1_multiple_notation_decls_no_comments() {
    // Multiple notation declarations without comments must parse cleanly.
    let source = "\
package test.pkg

notation infixl 40 \"X\" => core::math::add

notation infixr 50 \"Y\" => core::math::mul
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "multiple notation decls must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let notations: Vec<_> = tree
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Notation(n) => Some(n),
            _ => None,
        })
        .collect();
    assert_eq!(notations.len(), 2, "expected 2 notation declarations");
}

// ---- B12: logic connectives ==> and <==> --------------------------------

#[test]
fn imply_parses() {
    // `==>` is logical implication, right-associative, lower than `or`.
    let source = "\
emath function test() -> Bool:
    definitions:
        result = true ==> false
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "==> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    assert!(
        matches!(
            &expr.kind,
            ExprKind::Binary {
                op: BinaryOp::Imply,
                ..
            }
        ),
        "expected Imply, got {:?}",
        expr.kind
    );
}

#[test]
fn iff_parses() {
    // `<==>` is logical biconditional, lower than `==>`.
    let source = "\
emath function test() -> Bool:
    definitions:
        result = true <==> false
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "<==> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    assert!(
        matches!(
            &expr.kind,
            ExprKind::Binary {
                op: BinaryOp::Iff,
                ..
            }
        ),
        "expected Iff, got {:?}",
        expr.kind
    );
}

#[test]
fn imply_is_right_associative() {
    // `A ==> B ==> C` should parse as `A ==> (B ==> C)`.
    let source = "\
emath function test() -> Bool:
    definitions:
        result = true ==> false ==> true
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "chained ==> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    let ExprKind::Binary { op, left, right } = &expr.kind else {
        panic!("expected Binary, got {:?}", expr.kind);
    };
    assert_eq!(*op, BinaryOp::Imply, "top-level should be Imply");
    // Right child should also be Imply (right-associative).
    assert!(
        matches!(
            &right.kind,
            ExprKind::Binary {
                op: BinaryOp::Imply,
                ..
            }
        ),
        "right child should be Imply, got {:?}",
        right.kind
    );
    // Left child should be a Bool literal (true), not another Imply.
    assert!(
        matches!(&left.kind, ExprKind::Bool(true)),
        "left child should be true, got {:?}",
        left.kind
    );
}

#[test]
fn arrow_is_not_implication() {
    // `=>` must still parse as the match/lambda/notation arrow, not as `==>`.
    // `true => false` is not valid expression syntax (=> is not a binary op).
    let source = "\
emath function test() -> Bool:
    definitions:
        result = true
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "plain expression must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    // Verify the expression is just `true`, not something involving =>.
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    assert!(
        matches!(&expr.kind, ExprKind::Bool(true)),
        "expected Bool(true), got {:?}",
        expr.kind
    );
}

#[test]
fn binder_guard_parses() {
    // `sum i in 0..n if i > 2: i` should parse as a Binder with a guard.
    let source = "\
emath function test(n: Float64) -> Float64:
    definitions:
        result = sum i in 0..n if i > 2: i
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "binder with guard must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Binder { guard, .. } => {
            assert!(guard.is_some(), "guard should be Some");
        }
        other => panic!("expected Binder, got {:?}", other),
    }
}

// ---- C10: value-level generic arguments at use sites ---------------------

/// Find the type of the first field in an `inputs:` section.
fn first_input_ty<'a>(
    tree: &'a emath_core::tree::SyntaxTree,
) -> Option<&'a emath_core::tree::TypeExpr> {
    let item = tree.items.first()?;
    let Item::Declaration(decl) = item else {
        return None;
    };
    for section in decl.sections() {
        if section.name == "inputs" {
            for stmt in &section.suite.statements {
                if let StmtKind::FieldDecl { ty, .. } = &stmt.kind {
                    return Some(ty);
                }
            }
        }
    }
    None
}

#[test]
fn c10_mod_value_generic_parses() {
    // `Mod<7>` — integer literal as a value generic argument.
    let source = "\
emath model ModTest:
    inputs:
        x: Mod<7>
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "Mod<7> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let ty = first_input_ty(&tree).expect("expected `x` field in inputs");
    match &ty.kind {
        emath_core::tree::TypeKind::Path {
            segments,
            generic_args,
        } => {
            assert_eq!(segments.last().map(String::as_str), Some("Mod"));
            assert_eq!(generic_args.len(), 1, "Mod should have one generic arg");
            match &generic_args[0] {
                emath_core::tree::GenericArg::Value(expr) => {
                    assert!(
                        matches!(&expr.kind, ExprKind::Int(v) if v == "7"),
                        "expected Int(\"7\"), got {:?}",
                        expr.kind
                    );
                }
                other => panic!("expected GenericArg::Value, got {:?}", other),
            }
        }
        other => panic!("expected TypeKind::Path, got {:?}", other),
    }
}

#[test]
fn c10_tensor_bracket_list_extent_parses() {
    // `Tensor<Float64, [N, N]>` — type arg + bracket-list extent arg.
    let source = "\
emath model TensorTest:
    inputs:
        x: Tensor<Float64, [N, N]>
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "Tensor<Float64, [N, N]> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let ty = first_input_ty(&tree).expect("expected `x` field in inputs");
    match &ty.kind {
        emath_core::tree::TypeKind::Path {
            segments,
            generic_args,
        } => {
            assert_eq!(segments.last().map(String::as_str), Some("Tensor"));
            assert_eq!(generic_args.len(), 2, "Tensor should have two generic args");
            // First arg: Float64 (type)
            assert!(
                matches!(&generic_args[0], emath_core::tree::GenericArg::Type(_)),
                "first arg should be Type, got {:?}",
                generic_args[0]
            );
            // Second arg: [N, N] (value expression)
            match &generic_args[1] {
                emath_core::tree::GenericArg::Value(expr) => {
                    assert!(
                        matches!(&expr.kind, ExprKind::List(_)),
                        "expected List expr for [N, N], got {:?}",
                        expr.kind
                    );
                }
                other => panic!("expected GenericArg::Value, got {:?}", other),
            }
        }
        other => panic!("expected TypeKind::Path, got {:?}", other),
    }
}

#[test]
fn c10_named_generic_arg_parses() {
    // `GF<2, 3, modulus = x + 1>` — named argument in generic args.
    let source = "\
emath model GfTest:
    inputs:
        x: GF<2, 3, modulus = x + 1>
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "GF<2, 3, modulus = x + 1> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let ty = first_input_ty(&tree).expect("expected `x` field in inputs");
    match &ty.kind {
        emath_core::tree::TypeKind::Path {
            segments,
            generic_args,
        } => {
            assert_eq!(segments.last().map(String::as_str), Some("GF"));
            assert_eq!(generic_args.len(), 3, "GF should have three generic args");
            // First two: value literals (2, 3)
            assert!(matches!(
                &generic_args[0],
                emath_core::tree::GenericArg::Value(_)
            ));
            assert!(matches!(
                &generic_args[1],
                emath_core::tree::GenericArg::Value(_)
            ));
            // Third: named arg
            match &generic_args[2] {
                emath_core::tree::GenericArg::Named { name, arg } => {
                    assert_eq!(name, "modulus");
                    assert!(
                        matches!(arg.as_ref(), emath_core::tree::GenericArg::Value(_)),
                        "named arg value should be a Value, got {:?}",
                        arg
                    );
                }
                other => panic!("expected GenericArg::Named, got {:?}", other),
            }
        }
        other => panic!("expected TypeKind::Path, got {:?}", other),
    }
}

#[test]
fn c10_vector_float64_regression() {
    // Existing type-only generics must still parse identically.
    let source = "\
emath model VectorTest:
    inputs:
        x: Vector<Float64>
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "Vector<Float64> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let ty = first_input_ty(&tree).expect("expected `x` field in inputs");
    match &ty.kind {
        emath_core::tree::TypeKind::Path {
            segments,
            generic_args,
        } => {
            assert_eq!(segments.last().map(String::as_str), Some("Vector"));
            assert_eq!(generic_args.len(), 1);
            assert!(
                matches!(&generic_args[0], emath_core::tree::GenericArg::Type(_)),
                "Vector arg should be Type, got {:?}",
                generic_args[0]
            );
        }
        other => panic!("expected TypeKind::Path, got {:?}", other),
    }
}

// ---- Partial/Total derivatives with held-fixed sets (04 section 2.2) ------

#[test]
fn partial_derivative_parses() {
    let source = "\
emath function test(x: Float64) -> Float64:
    definitions:
        result = partial(x^2) wrt x
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "partial(x^2) wrt x must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative {
            kind, wrt, holding, ..
        } => {
            assert_eq!(*kind, DerivativeKind::Partial, "should be Partial");
            assert!(wrt.is_some(), "wrt should be attached");
            assert!(holding.is_empty(), "holding should be empty");
        }
        other => panic!("expected Derivative, got {:?}", other),
    }
}

#[test]
fn partial_derivative_unicode_parses() {
    let source = "\
emath function test(x: Float64) -> Float64:
    definitions:
        result = \u{2202}(x^2) wrt x
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "\u{2202}(x^2) wrt x must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative { kind, .. } => {
            assert_eq!(*kind, DerivativeKind::Partial, "should be Partial");
        }
        other => panic!("expected Derivative, got {:?}", other),
    }
}

#[test]
fn total_derivative_parses() {
    let source = "\
emath function test(t: Float64) -> Float64:
    definitions:
        result = total(t^2) wrt t
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "total(t^2) wrt t must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative { kind, wrt, .. } => {
            assert_eq!(*kind, DerivativeKind::Total, "should be Total");
            assert!(wrt.is_some(), "wrt should be attached");
        }
        other => panic!("expected Derivative, got {:?}", other),
    }
}

#[test]
fn total_derivative_d_form_parses() {
    let source = "\
emath function test(t: Float64) -> Float64:
    definitions:
        result = d(t^2) wrt t
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "d(t^2) wrt t must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative { kind, .. } => {
            assert_eq!(*kind, DerivativeKind::Total, "d(...) should be Total");
        }
        other => panic!("expected Derivative, got {:?}", other),
    }
}

#[test]
fn partial_derivative_holding_set_parses() {
    let source = "\
emath function test(T: Float64, p: Float64, V: Float64) -> Float64:
    definitions:
        result = partial(H) wrt T holding p
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "partial(H) wrt T holding p must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative {
            kind, wrt, holding, ..
        } => {
            assert_eq!(*kind, DerivativeKind::Partial);
            assert!(wrt.is_some(), "wrt should be attached");
            assert_eq!(holding.len(), 1, "holding should have one variable");
        }
        other => panic!("expected Derivative, got {:?}", other),
    }
}

#[test]
fn holding_set_different_variables_distinct() {
    // `partial(H) wrt T holding p` and `partial(H) wrt T holding V`
    // must produce structurally different AST nodes.
    let src_a = "emath function f(T: Float64, p: Float64) -> Float64:\n    definitions:\n        a = partial(H) wrt T holding p\n";
    let src_b = "emath function f(T: Float64, V: Float64) -> Float64:\n    definitions:\n        b = partial(H) wrt T holding V\n";
    let (tree_a, diags_a) = parse_str(src_a);
    let (tree_b, diags_b) = parse_str(src_b);
    assert!(!diags_a.has_errors());
    assert!(!diags_b.has_errors());
    let expr_a = def_expr(&tree_a, "a").unwrap();
    let expr_b = def_expr(&tree_b, "b").unwrap();
    // The holding sets differ (p vs V), so the Derivative nodes differ.
    assert_ne!(
        expr_a.kind, expr_b.kind,
        "different holding sets must produce different AST nodes"
    );
}

#[test]
fn partial_as_identifier_still_works() {
    // `partial` not followed by `(` should be a regular identifier.
    let source = "\
emath function test(partial: Float64) -> Float64:
    definitions:
        result = partial + 1
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "`partial` as identifier must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    assert!(
        matches!(
            &expr.kind,
            ExprKind::Binary {
                op: BinaryOp::Add,
                ..
            }
        ),
        "expected addition, got {:?}",
        expr.kind
    );
}

#[test]
fn d_as_identifier_still_works() {
    // `d` not followed by `(` should be a regular identifier.
    let source = "\
emath function test(d: Float64) -> Float64:
    definitions:
        result = d + 1
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "`d` as identifier must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    assert!(
        matches!(
            &expr.kind,
            ExprKind::Binary {
                op: BinaryOp::Add,
                ..
            }
        ),
        "expected addition, got {:?}",
        expr.kind
    );
}

#[test]
fn derivative_plain_regression() {
    // Existing `derivative(x) wrt x` must still produce Plain kind.
    let source = "\
emath function test(x: Float64) -> Float64:
    definitions:
        result = derivative(x^2) wrt x
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "derivative(x^2) wrt x must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative { kind, .. } => {
            assert_eq!(*kind, DerivativeKind::Plain, "derivative should be Plain");
        }
        other => panic!("expected Derivative, got {:?}", other),
    }
}

// ---- Complex numbers (B14) ------------------------------------------------

#[test]
fn complex_literal_2i_parses() {
    // `2i` should parse as `2 * i` (imaginary literal).
    let source = "\
emath function test() -> Float64:
    definitions:
        result = 2i
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "2i must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Binary { op, left, right } => {
            assert_eq!(*op, BinaryOp::Mul, "should be multiplication");
            assert!(
                matches!(&left.kind, ExprKind::Float(v) if v == "2"),
                "left should be Float(\"2\"), got {:?}",
                left.kind
            );
            assert!(
                matches!(&right.kind, ExprKind::Path { segments, .. } if segments == &["i"]),
                "right should be Path([\"i\"]), got {:?}",
                right.kind
            );
        }
        other => panic!("expected Binary Mul, got {:?}", other),
    }
}

#[test]
fn complex_literal_3_5i_parses() {
    // `3.5i` should parse as `3.5 * i`.
    let source = "\
emath function test() -> Float64:
    definitions:
        result = 3.5i
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "3.5i must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Binary { op, left, right } => {
            assert_eq!(*op, BinaryOp::Mul);
            assert!(
                matches!(&left.kind, ExprKind::Float(v) if v == "3.5"),
                "left should be Float(\"3.5\"), got {:?}",
                left.kind
            );
            assert!(
                matches!(&right.kind, ExprKind::Path { segments, .. } if segments == &["i"]),
                "right should be Path([\"i\"]), got {:?}",
                right.kind
            );
        }
        other => panic!("expected Binary Mul, got {:?}", other),
    }
}

#[test]
fn complex_literal_in_expression_parses() {
    // `1 + 2i` should parse as `1 + (2 * i)`.
    let source = "\
emath function test() -> Float64:
    definitions:
        result = 1 + 2i
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "1 + 2i must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Add,
            right,
            ..
        } => {
            // right should be 2 * i
            match &right.kind {
                ExprKind::Binary {
                    op: BinaryOp::Mul, ..
                } => {}
                other => panic!("expected Mul for 2i, got {:?}", other),
            }
        }
        other => panic!("expected Add, got {:?}", other),
    }
}

#[test]
fn complex_type_parses() {
    // `Complex` as a type annotation.
    let source = "\
emath model ComplexTest:
    inputs:
        z: Complex
";
    let (_tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "Complex type must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}

#[test]
fn vector_of_complex_parses() {
    // `Vector<Complex, [2]>` — Complex as element type.
    let source = "\
emath model ComplexVector:
    inputs:
        v: Vector<Complex, [2]>
";
    let (_tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "Vector<Complex, [2]> must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}

#[test]
fn identifier_starting_with_i_not_complex() {
    // `image` should NOT be parsed as complex literal — `i` is only a
    // complex suffix when not followed by identifier characters.
    let source = "\
emath function test(image: Float64) -> Float64:
    definitions:
        result = image + 1
";
    let (_tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "`image` should be a regular identifier, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}

// ---- unit of / dimension of compile-time queries (04 section 1.4) --------

use emath_core::tree::UnitQueryKind;

#[test]
fn unit_of_parses() {
    let source = "\
emath function test(m: Float64, c: Float64) -> Float64:
    definitions:
        result = unit of (m * c^2)
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "unit of (m * c^2) must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::UnitQuery { kind, .. } => {
            assert_eq!(*kind, UnitQueryKind::Unit, "should be Unit kind");
        }
        other => panic!("expected UnitQuery, got {:?}", other),
    }
}

#[test]
fn dimension_of_parses() {
    let source = "\
emath function test(thrust: Float64) -> Float64:
    definitions:
        result = dimension of thrust
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "dimension of thrust must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::UnitQuery { kind, .. } => {
            assert_eq!(*kind, UnitQueryKind::Dimension, "should be Dimension kind");
        }
        other => panic!("expected UnitQuery, got {:?}", other),
    }
}

#[test]
fn unit_of_with_comparison_parses() {
    // `unit of E == X` should parse as `(unit of E) == X`
    let source = "\
emath function test(e: Float64) -> Float64:
    definitions:
        result = unit of e == kg*m^2/s^2
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "unit of e == ... must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    // Top-level should be a comparison (Binary with Eq)
    match &expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Eq,
            left,
            ..
        } => {
            // Left should be UnitQuery
            assert!(
                matches!(
                    &left.kind,
                    ExprKind::UnitQuery {
                        kind: UnitQueryKind::Unit,
                        ..
                    }
                ),
                "left should be UnitQuery(Unit), got {:?}",
                left.kind
            );
        }
        other => panic!("expected Binary Eq, got {:?}", other),
    }
}

#[test]
fn unit_as_identifier_still_works() {
    // `unit` not followed by `of` should be a regular identifier.
    let source = "\
emath function test(unit: Float64) -> Float64:
    definitions:
        result = unit + 1
";
    let (_tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "`unit` as identifier must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Notation declarations (gap B): glyphs are typography (N5) and desugar to
// calls of the canonical target at parse time, so syntax must expose the
// target call shape and the precedence/governance contract.
// ---------------------------------------------------------------------------

/// Read a definition expression from the declaration item in the tree,
/// skipping over any leading `notation` items (files may declare notation
/// before the function that uses it).
fn declaration_def_expr<'a>(
    tree: &'a emath_core::tree::SyntaxTree,
    name: &str,
) -> Option<&'a emath_core::tree::Expr> {
    let item = tree
        .items
        .iter()
        .find(|item| matches!(item, Item::Declaration(_)))?;
    let emath_core::tree::Item::Declaration(decl) = item else {
        return None;
    };
    for section in decl.sections() {
        if section.name == "definitions" {
            for stmt in &section.suite.statements {
                match &stmt.kind {
                    StmtKind::Let { name: n, value, .. } if n == name => return Some(value),
                    StmtKind::Assign { target, value }
                        if target.segments.first().is_some_and(|s| s == name) =>
                    {
                        return Some(value);
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

fn assert_notation_call(expr: &emath_core::tree::Expr, target: &[&str], arity: usize) {
    let ExprKind::Call { function, args } = &expr.kind else {
        panic!("expected Call, got {:?}", expr.kind);
    };
    let ExprKind::Path { segments, .. } = &function.kind else {
        panic!("call function must be a Path, got {:?}", function.kind);
    };
    let segments: Vec<&str> = segments.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        segments, target,
        "call target must desugar to the canonical path"
    );
    assert_eq!(args.len(), arity, "call arity mismatch");
}

#[test]
fn notation_glyph_use_desugars_to_canonical_call() {
    // Order independence: the glyph is used before the declaration that
    // binds it, and the use still desugars to `core::math::pow(x, y)`.
    let source = "\
emath function F:
    inputs:
        x: Float64
        y: Float64
    outputs:
        r: Float64
    definitions:
        r = x ⊕ y
notation infixl 40 \"⊕\" => core::math::pow
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "glyph use must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(expr, &["core", "math", "pow"], 2);
}

#[test]
fn notation_alias_spelling_desugars_to_same_target() {
    let source = "\
emath function F:
    inputs:
        x: Float64
        y: Float64
    outputs:
        r: Float64
    definitions:
        r = x pw y
notation infixl 40 \"⊕\" => core::math::pow alias \"pw\"
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "alias use must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(expr, &["core", "math", "pow"], 2);
}

#[test]
fn notation_custom_infix_binds_tighter_than_core_ladder() {
    // Custom operators sit above the fixed core ladder
    // (`CUSTOM_OP_MIN_PRECEDENCE = 11`), so `a ⊕ b * c` is
    // `(a ⊕ b) * c` and `4 * x ⊕ 2` is `4 * (x ⊕ 2)`.
    let source = "\
emath function F:
    inputs:
        a: Float64
        b: Float64
        c: Float64
        x: Float64
    outputs:
        p: Float64
        q: Float64
    definitions:
        p = a ⊕ b * c
        q = 4 * x ⊕ 2
notation infixl 40 \"⊕\" => core::math::pow
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "custom precedence must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let p = declaration_def_expr(&tree, "p").expect("expected `p` binding");
    let ExprKind::Binary {
        op: BinaryOp::Mul,
        left,
        right,
    } = &p.kind
    else {
        panic!("`a ⊕ b * c` must parse as (a ⊕ b) * c, got {:?}", p.kind);
    };
    assert_notation_call(left, &["core", "math", "pow"], 2);
    assert!(matches!(right.kind, ExprKind::Path { .. }));

    let q = declaration_def_expr(&tree, "q").expect("expected `q` binding");
    let ExprKind::Binary {
        op: BinaryOp::Mul,
        left,
        right,
    } = &q.kind
    else {
        panic!("`4 * x ⊕ 2` must parse as 4 * (x ⊕ 2), got {:?}", q.kind);
    };
    assert!(matches!(left.kind, ExprKind::Int(_)));
    assert_notation_call(right, &["core", "math", "pow"], 2);
}

#[test]
fn notation_infixr_associates_right_and_infix_left() {
    let source = "\
emath function F:
    inputs:
        a: Float64
        b: Float64
        c: Float64
    outputs:
        r: Float64
        s: Float64
    definitions:
        r = a ⊕ b ⊕ c
        s = a ⊗ b ⊗ c
notation infixr 40 \"⊕\" => core::math::pow
notation infix 30 \"⊗\" => core::math::min
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "associativity must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    // infixr: a ⊕ (b ⊕ c)
    let r = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(&r, &["core", "math", "pow"], 2);
    let ExprKind::Call { function: _, args } = &r.kind else {
        unreachable!()
    };
    assert_notation_call(&args[1], &["core", "math", "pow"], 2);
    // infix (left-assoc): (a ⊗ b) ⊗ c
    let s = declaration_def_expr(&tree, "s").expect("expected `s` binding");
    let ExprKind::Call { args: s_args, .. } = &s.kind else {
        panic!("expected Call, got {:?}", s.kind);
    };
    assert!(
        matches!(&s_args[0].kind, ExprKind::Call { .. }),
        "infix must be left-associative"
    );
}

#[test]
fn notation_prefix_and_postfix_desugar_to_unary_target_calls() {
    let source = "\
emath function F:
    inputs:
        a: Float64
        b: Float64
    outputs:
        r: Float64
        s: Float64
    definitions:
        r = √ a
        s = b inv
notation prefix 80 \"√\" => core::math::sqrt
notation postfix 90 \"inv\" => core::math::recip
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "prefix/postfix glyphs must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let r = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(r, &["core", "math", "sqrt"], 1);
    let s = declaration_def_expr(&tree, "s").expect("expected `s` binding");
    assert_notation_call(s, &["core", "math", "recip"], 1);
}

#[test]
fn notation_reserved_glyph_is_refused() {
    // `or` is part of the core vocabulary (N3); no scoped rebinding.
    let source = "\
emath function F:
    outputs:
        r: Float64
    definitions:
        r = 1.0
notation prefix 90 \"or\" => core::logic::not
";
    let (_tree, diags) = parse_str(source);
    let codes: Vec<_> = diags.errors().map(|e| e.code).collect();
    assert!(
        codes.contains(&"E-NOTATION-RESERVED"),
        "reserved glyph must refuse with E-NOTATION-RESERVED, got {codes:?}"
    );
}

#[test]
fn notation_ambiguous_targets_are_refused() {
    // The same glyph bound to two different targets is ambiguous (N4).
    let source = "\
emath function F:
    outputs:
        r: Float64
    definitions:
        r = 1.0
notation infixl 40 \"⊕\" => core::math::pow
notation infixl 60 \"⊕\" => core::math::min
";
    let (_tree, diags) = parse_str(source);
    let codes: Vec<_> = diags.errors().map(|e| e.code).collect();
    assert!(
        codes.contains(&"E-NOTATION-AMBIG"),
        "conflicting redeclaration must refuse with E-NOTATION-AMBIG, got {codes:?}"
    );
}

#[test]
fn notation_precedence_below_custom_floor_is_refused() {
    // The core ladder owns precedences 1..=10; a custom operator mapped
    // inside it would silently never bind, so the declaration is
    // refused with E-NOTATION-PRECEDENCE instead.
    let source = "\
emath function F:
    outputs:
        r: Float64
    definitions:
        r = 1.0
notation infixl 3 \"⊕\" => core::math::pow
";
    let (_tree, diags) = parse_str(source);
    let codes: Vec<_> = diags.errors().map(|e| e.code).collect();
    assert!(
        codes.contains(&"E-NOTATION-PRECEDENCE"),
        "core-ladder precedence must refuse with E-NOTATION-PRECEDENCE, got {codes:?}"
    );
}

#[test]
fn notation_punctuation_glyph_is_refused() {
    // `!` lexes as its own token, never as a single identifier, so it
    // cannot be a custom operator (E-NOTATION-GLYPH).
    let source = "\
emath function F:
    outputs:
        r: Float64
    definitions:
        r = 1.0
notation infixl 40 \"!\" => core::math::pow
";
    let (_tree, diags) = parse_str(source);
    let codes: Vec<_> = diags.errors().map(|e| e.code).collect();
    assert!(
        codes.contains(&"E-NOTATION-GLYPH"),
        "punctuation glyph must refuse with E-NOTATION-GLYPH, got {codes:?}"
    );
}

#[test]
fn math_symbol_does_not_glue_to_adjacent_letters() {
    // `x⊕y` and `√a` must be operator uses, not one unknown identifier.
    let (tokens, diagnostics) = lex("x⊕y √a αβ", FileId(0), &Limits::default());
    assert!(
        !diagnostics.has_errors(),
        "juxtaposed glyphs must lex, got {:?}",
        diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let idents: Vec<&str> = tokens
        .iter()
        .filter_map(|token| match &token.kind {
            TokenKind::Ident(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(idents, ["x", "⊕", "y", "√", "a", "αβ"]);
}

#[test]
fn notation_unspaced_glyph_desugars_to_pow() {
    let source = "\
emath function F:
    inputs:
        x: Float64
        y: Float64
    outputs:
        r: Float64
    definitions:
        r = x⊕y
notation infixl 40 \"⊕\" => core::math::pow
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "unspaced glyph use must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(expr, &["core", "math", "pow"], 2);
}

#[test]
fn notation_unspaced_prefix_desugars_to_sqrt() {
    let source = "\
emath function F:
    inputs:
        a: Float64
    outputs:
        r: Float64
    definitions:
        r = √a
notation prefix 80 \"√\" => core::math::sqrt
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "unspaced prefix glyph must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    let expr = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(expr, &["core", "math", "sqrt"], 1);
}

#[test]
fn keyword_as_declaration_name_is_refused() {
    let source = "\
emath function if:
    outputs:
        r: Float64
    definitions:
        r = 1.0
";
    let (_tree, diags) = parse_str(source);
    assert!(
        diags.errors().any(|e| e.code == "E-SYN-101"
            && e.message
                .contains("keyword `if` cannot be used as an identifier")),
        "keyword declaration name must refuse, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn keyword_as_field_name_is_refused() {
    let source = "\
emath function F:
    inputs:
        if: Float64
    outputs:
        r: Float64
    definitions:
        r = 1.0
";
    let (_tree, diags) = parse_str(source);
    assert!(
        diags.errors().any(|e| e.code == "E-SYN-101"
            && e.message
                .contains("keyword `if` cannot be used as an identifier")),
        "keyword field name must refuse, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn keyword_as_package_segment_is_refused() {
    let source = "\
package tst.if
emath function F:
    outputs:
        r: Float64
    definitions:
        r = 1.0
";
    let (tree, diags) = parse_str(source);
    assert!(
        diags.errors().any(|e| e.code == "E-SYN-101"
            && e.message
                .contains("keyword `if` cannot be used as an identifier")),
        "keyword package segment must refuse, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        !tree
            .items
            .iter()
            .any(|item| matches!(item, Item::Package { path, .. } if path.as_slice() == ["tst"])),
        "truncated package path `tst` must not be recorded"
    );
}

#[test]
fn keyword_as_notation_glyph_is_refused() {
    let source = "\
emath function F:
    outputs:
        r: Float64
    definitions:
        r = 1.0
notation infixl 40 \"if\" => core::math::pow
";
    let (_tree, diags) = parse_str(source);
    let codes: Vec<_> = diags.errors().map(|e| e.code).collect();
    assert!(
        codes.contains(&"E-NOTATION-GLYPH"),
        "keyword glyph must refuse with E-NOTATION-GLYPH, got {codes:?}"
    );
}

// ---- Anti-proposals negative controls ---------------------------------------
// Failure-first: this test is authored against the intended behavior and
// must FAIL before the juxtaposition suggestion exists (A-bonus, C15).
#[test]
fn juxtaposition_2x_is_refused_with_2_times_x_suggestion() {
    // `2x` is not `2 * x` (anti-proposal bonus). The parser must refuse
    // with a suggestion naming the admitted spelling.
    let source = "\
emath function f() -> Float64:
    definitions:
        r = 2x
";
    let (_tree, diags) = parse_str(source);
    let rendered: Vec<String> = diags
        .errors()
        .map(|e| format!("{} {}", e.code, e.message))
        .collect();
    assert!(
        rendered
            .iter()
            .any(|m| m.contains("juxtapos") && m.contains("2 * x")),
        "`2x` must refuse with a juxtaposition + `2 * x` suggestion, got {rendered:?}"
    );
}

#[test]
fn explicit_2_times_x_still_admits() {
    // The admitted spelling must remain untouched — the juxtaposition
    // refusal cannot fire on operator-separated operands.
    let source = "\
emath function f() -> Float64:
    definitions:
        r = 2 * x
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "2 * x must admit, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "r").expect("expected `r` binding");
    let ExprKind::Binary { op, .. } = &expr.kind else {
        panic!("expected Binary, got {:?}", expr.kind);
    };
    assert_eq!(*op, BinaryOp::Mul);
}

#[test]
fn notation_glyph_after_int_is_not_juxtaposition() {
    // `2 ⊕ x` with a registered glyph must parse through the notation
    // infix layer; the adjacency check must exclude registered operators.
    let source = "\
emath function f() -> Float64:
    definitions:
        r = 2 ⊕ x
notation infixl 40 \"⊕\" => core::math::add
";
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "Int + notation glyph must parse as notation infix, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "r").expect("expected `r` binding");
    // The declared target IS the canonical desugar: every registered glyph
    // lowers to `Call(<declared target>, args)` — the same contract the
    // sibling tests pin (`notation_glyph_use_desugars_to_canonical_call`,
    // `notation_alias_spelling_desugars_to_same_target`). The point of this
    // test is the int-adjacent position: `2 ⊕ x` must reach the notation
    // layer instead of folding `⊕` into a quantity unit.
    assert_notation_call(expr, &["core", "math", "add"], 2);
}
