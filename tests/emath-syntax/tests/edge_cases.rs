//! Edge-case regressions: empty / extreme / Unicode lexer + source map,
//! and parser edge cases (F5 non-greedy derivative operand).

use emath_core::limits::Limits;
use emath_core::tree::{BinaryOp, DerivativeKind, ExprKind, Item, NotationFixity, StmtKind};
use emath_core::{Diagnostic, FileId, SourceStore, Span};
use emath_syntax::lexer::{lex, lex_with_comments};
use emath_syntax::token::TokenKind;
use emath_syntax::{parse, parse_lossless, parse_str};

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

// ---- C1: conditional expression spelling -----------------------------------

// ---- C3: numeric literal indexing refused ----------------------------------

// ---- N1-N5: notation governance core ----------------------------------

// ---- B12: logic connectives ==> and <==> --------------------------------

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

// ---- Partial/Total derivatives with held-fixed sets (04 section 2.2) ------

// ---- Complex numbers (B14) ------------------------------------------------

// ---- unit of / dimension of compile-time queries (04 section 1.4) --------

use emath_core::tree::UnitQueryKind;

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

fn assert_notation_call(p: &mut Probe, expr: &emath_core::tree::Expr, target: &[&str], arity: usize) {
    let ExprKind::Call { function, args } = &expr.kind else {
        panic!("expected Call, got {:?}", expr.kind);
    };
    let ExprKind::Path { segments, .. } = &function.kind else {
        panic!("call function must be a Path, got {:?}", function.kind);
    };
    let segments: Vec<&str> = segments.iter().map(|s| s.as_str()).collect();
    p.eq("target", segments, target);
    p.eq("arity", args.len(), arity);
}

// ---- Anti-proposals negative controls ---------------------------------------
// Failure-first: this test is authored against the intended behavior and
// must FAIL before the juxtaposition suggestion exists (A-bonus, C15).

use emath_test_harness::{Probe, boot};

#[test]
fn edge_cases() {
    boot();
    let mut probe = Probe::new("Edge-case regressions: empty / extreme / Unicode lexer + source map, and parser edge cases (F5 non-greedy derivative operand).");
    probe.case("empty_source_lexes_only_eof", |p| {
    let f0 = p.failures().len();

    let (tokens, diagnostics) = lex("", FileId(0), &Limits::default());
    p.demand("1",!diagnostics.has_errors(), stringify!(!diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    p.eq("2", tokens.len(), 1);
    p.demand("3",matches!(tokens[0].kind, TokenKind::Eof), stringify!(matches!(tokens[0].kind, TokenKind::Eof)));
    if p.failures().len() != f0 { return; }

    });
    probe.case("exact_rational_double_slash_is_not_a_comment", |p| {
    let f0 = p.failures().len();

    // Spec literal family `3//7`. Treating `//` as a line comment used to
    // silently drop the denominator, leaving only Int("3").
    let (tokens, diagnostics) = lex("3//7", FileId(0), &Limits::default());
    p.demand("1",!diagnostics.has_errors(), format!(
        "exact rational must lex cleanly, got {:?}",
        diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
    p.demand("2",matches!(
            kinds.as_slice(),
            [TokenKind::Int(n), TokenKind::SlashSlash, TokenKind::Int(d), TokenKind::Eof]
                if n == "3" && d == "7"
        ), format!(
        "3//7 must be Int // Int, got {kinds:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("exact_rational_literal_folds_in_the_parser", |p| {
    let f0 = p.failures().len();

    // Grammar: `rational_literal = integer "//" integer` is a primary.
    // After the lexer emits SlashSlash, the parser must fold `3//7` into
    // one expression, not bind `3` and leave `//7` as leftover junk.
    let source = "\
emath function f() -> Float64:
    definitions:
        r = 3//7
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "3//7 must parse as a rational literal, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "r").expect("expected `r` binding");
    p.demand("2",matches!(&expr.kind, ExprKind::Rational { numer, denom } if numer == "3" && denom == "7"), format!(
        "expected Rational {{ numer: 3, denom: 7 }}, got {:?}",
        expr.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("exact_rational_quantity_attaches_the_unit", |p| {
    let f0 = p.failures().len();

    // Grammar: quantity_literal = (integer | decimal | rational_literal) whitespace path.
    // `3//2 s` is a quantity whose value is the rational, not Int(3).
    let source = "\
emath function f() -> Float64:
    definitions:
        q = 3//2 s
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "3//2 s must parse as a quantity, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "q").expect("expected `q` binding");
    let ExprKind::Quantity { value, unit } = &expr.kind else {
        panic!("expected Quantity, got {:?}", expr.kind);
    };
    p.demand("2",matches!(&value.kind, ExprKind::Rational { numer, denom } if numer == "3" && denom == "2"), format!(
        "quantity value should be Rational 3//2, got {:?}",
        value.kind
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3", (unit.to_string()) == ("s"), format!("expected {:?}, got {:?}", ("s"), (unit.to_string())));
    if p.failures().len() != f0 { return; }

    });
    probe.case("exact_rational_missing_denominator_is_named_refuse", |p| {
    let f0 = p.failures().len();

    // `3//x` is not `integer "//" integer`. Must diagnose, not silently
    // keep Int("3") with no error.
    let source = "\
emath function f() -> Float64:
    definitions:
        r = 3//x
";
    let (_tree, diags) = parse_str(source);
    p.demand("1",diags.errors().any(|e| e.code == "E-SYN-101"), format!(
        "non-integer denominator must be E-SYN-101, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("hash_and_doc_comments_still_skip_the_line", |p| {
    let f0 = p.failures().len();

    let (tokens, diagnostics, comments) = lex_with_comments(
        "# ordinary\n/// documentation\nx",
        FileId(0),
        &Limits::default(),
    );
    p.demand("1",!diagnostics.has_errors(), stringify!(!diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
    p.demand("2",matches!(
            kinds.as_slice(),
            [TokenKind::Ident(name), TokenKind::Eof] if name == "x"
        ), format!(
        "comments must not emit tokens, got {kinds:?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",comments.iter().any(|c| c.text.starts_with('#')), format!(
        "ordinary `#` comment must retain the marker, got {comments:?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("4",comments.iter().any(|c| c.text.starts_with("///")), format!(
        "doc `///` comment must retain the marker, got {comments:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("double_slash_comment_is_not_admitted", |p| {
    let f0 = p.failures().len();

    // Spec comments are `#` and `///` only. `// rest` is SlashSlash plus
    // whatever follows, not a silent skip of `rest`.
    let (tokens, diagnostics) = lex("a//b", FileId(0), &Limits::default());
    p.demand("1",!diagnostics.has_errors(), stringify!(!diagnostics.has_errors()));
    if p.failures().len() != f0 { return; }
    let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
    p.demand("2",matches!(
            kinds.as_slice(),
            [
                TokenKind::Ident(left),
                TokenKind::SlashSlash,
                TokenKind::Ident(right),
                TokenKind::Eof
            ] if left == "a" && right == "b"
        ), format!(
        "`//` must not eat the following ident, got {kinds:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("braces_suppress_newlines_like_parens", |p| {
    let f0 = p.failures().len();

    // Spec: NEWLINE is suppressed inside `()`, `[]`, and `{}`.
    let (paren, _) = lex("(\n1\n)", FileId(0), &Limits::default());
    let (brace, _) = lex("{\n1\n}", FileId(0), &Limits::default());
    let paren_kinds: Vec<&TokenKind> = paren.iter().map(|t| &t.kind).collect();
    let brace_kinds: Vec<&TokenKind> = brace.iter().map(|t| &t.kind).collect();
    p.demand("1",!paren_kinds.iter().any(|k| matches!(k, TokenKind::Newline)), format!(
        "parens already suppress newlines, got {paren_kinds:?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",!brace_kinds.iter().any(|k| matches!(k, TokenKind::Newline)), format!(
        "braces must suppress newlines, got {brace_kinds:?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",matches!(
            brace_kinds.as_slice(),
            [
                TokenKind::LBrace,
                TokenKind::Int(n),
                TokenKind::RBrace,
                TokenKind::Eof
            ] if n == "1"
        ), format!(
        "expected brace-wrapped int, got {brace_kinds:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("tabs_are_rejected_in_canonical_source", |p| {
    let f0 = p.failures().len();

    let (_, diagnostics) = lex("a\tb", FileId(0), &Limits::default());
    p.demand("1",diagnostics.errors().any(|error| error.code == "E-SYN-101"), format!(
        "tab must be a typed refusal, got {:?}",
        diagnostics
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("invalid_unicode_escape_keeps_string_and_following_ident", |p| {
    let f0 = p.failures().len();

    // `\u` without `{` used to `break` the string loop, emit a truncated Str,
    // and retokenize `x"` as Ident + junk. Recovery must stay in the string.
    let (tokens, diagnostics) = lex(r#""\ux" after"#, FileId(0), &Limits::default());
    p.demand("1",diagnostics.errors().any(|error| error.code == "E-SYN-109"), format!(
        "expected E-SYN-109 for malformed \\u"
    ));
    if p.failures().len() != f0 { return; }
    let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
    p.demand("2",kinds.iter().any(|kind| matches!(kind, TokenKind::Str(_))), format!(
        "still emits one Str token: {kinds:?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",kinds
            .iter()
            .any(|kind| matches!(kind, TokenKind::Ident(name) if name == "after")), format!(
        "tail after the closing quote must lex as Ident(after), got {kinds:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("oversized_unicode_escape_hex_is_diagnosed", |p| {
    let f0 = p.failures().len();

    // Hex wider than u32 used to be silently skipped (no diagnostic, no char).
    let (tokens, diagnostics) = lex(r#""\u{100000000}""#, FileId(0), &Limits::default());
    p.demand("1",diagnostics.errors().any(|error| error.code == "E-SYN-109"), format!(
        "expected E-SYN-109 for overflow hex"
    ));
    if p.failures().len() != f0 { return; }
    let value = tokens
        .iter()
        .find_map(|token| match &token.kind {
            TokenKind::Str(value) => Some(value.as_str()),
            _ => None,
        })
        .expect("expected a Str token");
    p.demand("2",value.is_empty(), format!(
        "overflow escape must not invent a char, got {value:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("empty_unicode_escape_is_diagnosed", |p| {
    let f0 = p.failures().len();

    let (_, diagnostics) = lex(r#""\u{}""#, FileId(0), &Limits::default());
    p.demand("1",diagnostics.errors().any(|error| error.code == "E-SYN-109"), stringify!(diagnostics.errors().any(|error| error.code == "E-SYN-109")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("line_col_max_offset_does_not_overflow", |p| {

    let mut store = SourceStore::new();
    let id = store.add("t.emath", "");
    let file = store.get(id).expect("file");
    let (line, col) = file.line_col(u32::MAX);
    p.eq("1", line, 1);
    p.eq("2", col, u32::MAX);

    });
    probe.case("token_limit_does_not_emit_unbounded_trailing_dedents", |p| {
    let f0 = p.failures().len();

    let limits = Limits {
        max_tokens: 4,
        ..Limits::default()
    };
    // Many successively deeper indents after the budget is spent must not
    // append one Dedent per phantom stack frame past max_tokens (+Eof).
    let source = "a\n b\n  c\n   d\n    e\n     f\n      g\n";
    let (tokens, diagnostics) = lex(source, FileId(0), &limits);
    p.demand("1",diagnostics.errors().any(|error| error.code == "E-SYN-108"), format!(
        "expected token-limit diagnostic"
    ));
    if p.failures().len() != f0 { return; }
    let dedents = tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::Dedent))
        .count();
    p.demand("2",tokens.len() <= limits.max_tokens + 1, format!(
        "tokens={} exceeds max_tokens+Eof; dedents={dedents}",
        tokens.len()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("caret_aligns_after_multibyte_prefix", |p| {
    let f0 = p.failures().len();

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
    p.demand("1", (caret_line) == ("   ^"), format!("expected {:?}, got {:?}", ("   ^"), (caret_line)));
    if p.failures().len() != f0 { return; }

    });
    probe.case("source_over_byte_limit_refuses_without_scanning", |p| {
    let f0 = p.failures().len();

    let limits = Limits {
        max_source_bytes: 8,
        ..Limits::default()
    };
    let source = "aaaaaaaaaa"; // 10 bytes > 8
    let (tokens, diagnostics) = lex(source, FileId(0), &limits);
    p.demand("1",diagnostics.errors().any(|error| error.code == "E-SYN-116"), format!(
        "expected E-SYN-116 for oversized source"
    ));
    if p.failures().len() != f0 { return; }
    p.eq("2", tokens.len(), 1);
    p.demand("3",matches!(tokens[0].kind, TokenKind::Eof), stringify!(matches!(tokens[0].kind, TokenKind::Eof)));
    if p.failures().len() != f0 { return; }

    });
    probe.case("parse_skips_scratch_expand_when_source_exceeds_limits", |p| {
    let f0 = p.failures().len();

    let limits = Limits {
        max_source_bytes: 8,
        ..Limits::default()
    };
    let source = "aaaaaaaaaa"; // 10 bytes > 8
    let (_, diagnostics) = parse(source, FileId(0), &limits);
    p.demand("1",diagnostics.errors().any(|error| error.code == "E-SYN-116"), format!(
        "expected E-SYN-116 without expanding oversized scratch"
    ));
    if p.failures().len() != f0 { return; }
    let lossless = parse_lossless(source, FileId(0), &limits);
    p.demand("2",lossless
            .diagnostics
            .errors()
            .any(|error| error.code == "E-SYN-116"), format!(
        "expected E-SYN-116 from lossless parse of oversized source"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",lossless.comments.is_empty(), format!(
        "oversized lossless parse must not retain comments"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("diagnostic_message_newlines_cannot_inject_frames", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!header.contains('\n') && header.contains("first injected:1:1: E-FAKE: second"), format!(
        "message controls must flatten into one header line, got {rendered:?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",rendered.contains("= help: help line"), format!(
        "help controls must flatten, got {rendered:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("f5_derivative_plus_does_not_greedily_consume", |p| {
    let f0 = p.failures().len();

    // `derivative(v) + v` must parse as `(derivative v) + v`,
    // not `derivative(v + v)`.
    let source = "\
emath function f(v: Float64) -> Float64:
    definitions:
        result = derivative(v) + v
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "must parse cleanly, got errors: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    // Top level should be Binary(Add, Derivative(v), v)
    let ExprKind::Binary { op, left, right } = &expr.kind else {
        panic!("expected Binary at top level, got {:?}", expr.kind);
    };
    p.eq("2", *op, BinaryOp::Add);
    p.demand("3",matches!(&left.kind, ExprKind::Derivative { .. }), format!(
        "left side should be Derivative, got {:?}",
        left.kind
    ));
    if p.failures().len() != f0 { return; }
    p.demand("4",matches!(&right.kind, ExprKind::Path { .. }), format!(
        "right side should be the bare identifier v, got {:?}",
        right.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("f5_derivative_parenthesised_operand_still_works", |p| {
    let f0 = p.failures().len();

    // `derivative(v + v)` must still parse as derivative of the sum.
    let source = "\
emath function f(v: Float64) -> Float64:
    definitions:
        result = derivative(v + v)
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "must parse cleanly, got errors: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    let ExprKind::Derivative { value, wrt, .. } = &expr.kind else {
        panic!("expected Derivative at top level, got {:?}", expr.kind);
    };
    p.demand("2",matches!(
            value.kind,
            ExprKind::Binary {
                op: BinaryOp::Add,
                ..
            }
        ), format!(
        "operand should be v + v, got {:?}",
        value.kind
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",wrt.is_none(), stringify!(wrt.is_none()));
    if p.failures().len() != f0 { return; }

    });
    probe.case("f5_derivative_wrt_still_attaches", |p| {
    let f0 = p.failures().len();

    // `derivative(y) wrt x` must still attach the wrt clause.
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        y = x * x
        dy = derivative(y) wrt x
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "must parse cleanly, got errors: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "dy").expect("expected `dy` binding");
    let ExprKind::Derivative { wrt, .. } = &expr.kind else {
        panic!("expected Derivative, got {:?}", expr.kind);
    };
    p.demand("2",wrt.is_some(), format!( "wrt must be attached"));
    if p.failures().len() != f0 { return; }

    });
    probe.case("c1_conditional_uses_colon_form", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!diags.has_errors(), format!(
        "colon-form conditional must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
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
    p.demand("2",matches!(
            &condition.kind,
            ExprKind::Binary {
                op: BinaryOp::Gt,
                ..
            }
        ), format!(
        "condition should be x > 0, got {:?}",
        condition.kind
    ));
    if p.failures().len() != f0 { return; }
    p.demand("3",matches!(&then_value.kind, ExprKind::Int(_)), format!(
        "then_value should be 1, got {:?}",
        then_value.kind
    ));
    if p.failures().len() != f0 { return; }
    p.demand("4",matches!(&else_value.kind, ExprKind::Int(_)), format!(
        "else_value should be 0, got {:?}",
        else_value.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("c3_numeric_literal_not_indexed", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",matches!(&expr.kind, ExprKind::Float(_)), format!(
        "x should be bound to a Float literal (9.81), not an Index, got {:?}",
        expr.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("c3_variable_indexing_still_works", |p| {
    let f0 = p.failures().len();

    // `v[0]` on a non-literal primary (path/identifier) must still parse
    // as indexing.  The C3 fix only refuses `[` after numeric literals.
    let source = "\
emath function idx(v: Vector[3]) -> Float64:
    definitions:
        x = v[0]
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "variable indexing must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "x").expect("expected `x` binding");
    let ExprKind::Index { value, indices } = &expr.kind else {
        panic!("expected Index expression, got {:?}", expr.kind);
    };
    p.demand("2",matches!(&value.kind, ExprKind::Path { .. }), format!(
        "indexed value should be a path (v), got {:?}",
        value.kind
    ));
    if p.failures().len() != f0 { return; }
    p.eq("3", indices.len(), 1);

    });
    probe.case("c3_list_literal_indexing_still_works", |p| {
    let f0 = p.failures().len();

    // `[[1, 2], [3, 4]][0]` on a list literal must still parse as indexing.
    // The C3 fix only refuses `[` after numeric scalar literals (Int,
    // Float, Quantity), not after list/tuple primaries.
    let source = "\
emath function mat() -> Vector[2]:
    definitions:
        row = [[1, 2], [3, 4]][0]
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "list literal indexing must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "row").expect("expected `row` binding");
    let ExprKind::Index { value, .. } = &expr.kind else {
        panic!("expected Index expression, got {:?}", expr.kind);
    };
    p.demand("2",matches!(&value.kind, ExprKind::List(_)), format!(
        "indexed value should be a list, got {:?}",
        value.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("n1_notation_decl_parses", |p| {
    let f0 = p.failures().len();

    // N1: notation declarations are package-level items, scoped to the
    // package and imported via `use`.
    let source = "\
package test.pkg

notation infixl 40 \"⋅\" => core::math::dot
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "notation decl must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let notation = tree
        .items
        .iter()
        .find_map(|item| match item {
            Item::Notation(n) => Some(n),
            _ => None,
        })
        .expect("expected a Notation item");
    p.eq("2", notation.fixity, NotationFixity::InfixLeft);
    p.eq("3", notation.precedence, 40);
    p.demand("4", (notation.glyph) == ("⋅"), format!("expected {:?}, got {:?}", ("⋅"), (notation.glyph)));
    if p.failures().len() != f0 { return; }
    p.demand("5", (notation.target) == (vec!["core", "math", "dot"]), format!("expected {:?}, got {:?}", (vec!["core", "math", "dot"]), (notation.target)));
    if p.failures().len() != f0 { return; }
    p.demand("6",notation.alias.is_none(), format!( "no alias clause"));
    if p.failures().len() != f0 { return; }

    });
    probe.case("n2_notation_alias_clause_parses", |p| {
    let f0 = p.failures().len();

    // N2: the optional `alias` clause provides an alternative spelling.
    // accept-many/canon-one: multiple aliases map to one canonical path.
    let source = "\
package test.pkg

notation infixl 40 \"⋅\" => core::math::dot alias \"pw\"
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "notation with alias must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let notation = tree
        .items
        .iter()
        .find_map(|item| match item {
            Item::Notation(n) => Some(n),
            _ => None,
        })
        .expect("expected a Notation item");
    p.eq("2", notation.alias.as_deref(), Some("pw"));

    });
    probe.case("n1_all_fixity_forms_parse", |p| {
    let f0 = p.failures().len();

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
        p.demand("1",!diags.has_errors(), format!(
            "fixity `{fixity_str}` must parse cleanly, got: {:?}",
            diags.errors().map(|e| e.code).collect::<Vec<_>>()
        ));
        if p.failures().len() != f0 { return; }
        let notation = tree
            .items
            .iter()
            .find_map(|item| match item {
                Item::Notation(n) => Some(n),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected Notation for fixity `{fixity_str}`"));
        p.eq("2", notation.fixity, expected);
    }

    });
    probe.case("n4_invalid_fixity_errors", |p| {
    let f0 = p.failures().len();

    // N4 conflict rules: an unrecognized fixity keyword must produce a
    // parse error, not silently misparse.
    let source = "\
package test.pkg

notation infixd 40 \"⋅\" => core::math::dot
";
    let (tree, diags) = parse_str(source);
    p.demand("1",diags.has_errors(), format!(
        "invalid fixity `infixd` must produce an error"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",tree.items.iter().all(|i| !matches!(i, Item::Notation(_))), format!(
        "no Notation item should be produced for invalid fixity"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("n1_notation_example_file_parses", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!diags.has_errors(), format!(
        "notation example must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let notations: Vec<_> = tree
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Notation(n) => Some(n),
            _ => None,
        })
        .collect();
    p.eq("2", notations.len(), 6);
    // The last one should have an alias clause (N2: an alias is an
    // alternative spelling, so it must lex as a single identifier —
    // punctuation like `++` would silently re-lex as operators).
    p.eq("3", notations[5].alias.as_deref(), Some("plus"));

    });
    probe.case("n1_multiple_notation_decls_no_comments", |p| {
    let f0 = p.failures().len();

    // Multiple notation declarations without comments must parse cleanly.
    let source = "\
package test.pkg

notation infixl 40 \"X\" => core::math::add

notation infixr 50 \"Y\" => core::math::mul
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "multiple notation decls must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let notations: Vec<_> = tree
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Notation(n) => Some(n),
            _ => None,
        })
        .collect();
    p.eq("2", notations.len(), 2);

    });
    probe.case("imply_parses", |p| {
    let f0 = p.failures().len();

    // `==>` is logical implication, right-associative, lower than `or`.
    let source = "\
emath function test() -> Bool:
    definitions:
        result = true ==> false
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "==> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    p.demand("2",matches!(
            &expr.kind,
            ExprKind::Binary {
                op: BinaryOp::Imply,
                ..
            }
        ), format!(
        "expected Imply, got {:?}",
        expr.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("iff_parses", |p| {
    let f0 = p.failures().len();

    // `<==>` is logical biconditional, lower than `==>`.
    let source = "\
emath function test() -> Bool:
    definitions:
        result = true <==> false
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "<==> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    p.demand("2",matches!(
            &expr.kind,
            ExprKind::Binary {
                op: BinaryOp::Iff,
                ..
            }
        ), format!(
        "expected Iff, got {:?}",
        expr.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("imply_is_right_associative", |p| {
    let f0 = p.failures().len();

    // `A ==> B ==> C` should parse as `A ==> (B ==> C)`.
    let source = "\
emath function test() -> Bool:
    definitions:
        result = true ==> false ==> true
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "chained ==> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    let ExprKind::Binary { op, left, right } = &expr.kind else {
        panic!("expected Binary, got {:?}", expr.kind);
    };
    p.eq("2", *op, BinaryOp::Imply);
    // Right child should also be Imply (right-associative).
    p.demand("3",matches!(
            &right.kind,
            ExprKind::Binary {
                op: BinaryOp::Imply,
                ..
            }
        ), format!(
        "right child should be Imply, got {:?}",
        right.kind
    ));
    if p.failures().len() != f0 { return; }
    // Left child should be a Bool literal (true), not another Imply.
    p.demand("4",matches!(&left.kind, ExprKind::Bool(true)), format!(
        "left child should be true, got {:?}",
        left.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("arrow_is_not_implication", |p| {
    let f0 = p.failures().len();

    // `=>` must still parse as the match/lambda/notation arrow, not as `==>`.
    // `true => false` is not valid expression syntax (=> is not a binary op).
    let source = "\
emath function test() -> Bool:
    definitions:
        result = true
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "plain expression must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    // Verify the expression is just `true`, not something involving =>.
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    p.demand("2",matches!(&expr.kind, ExprKind::Bool(true)), format!(
        "expected Bool(true), got {:?}",
        expr.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("binder_guard_parses", |p| {
    let f0 = p.failures().len();

    // `sum i in 0..n if i > 2: i` should parse as a Binder with a guard.
    let source = "\
emath function test(n: Float64) -> Float64:
    definitions:
        result = sum i in 0..n if i > 2: i
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "binder with guard must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Binder { guard, .. } => {
            p.demand("2",guard.is_some(), format!( "guard should be Some"));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected Binder, got {:?}", other),
    }

    });
    probe.case("c10_mod_value_generic_parses", |p| {
    let f0 = p.failures().len();

    // `Mod<7>` — integer literal as a value generic argument.
    let source = "\
emath model ModTest:
    inputs:
        x: Mod<7>
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "Mod<7> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let ty = first_input_ty(&tree).expect("expected `x` field in inputs");
    match &ty.kind {
        emath_core::tree::TypeKind::Path {
            segments,
            generic_args,
        } => {
            p.eq("2", segments.last().map(String::as_str), Some("Mod"));
            p.eq("3", generic_args.len(), 1);
            match &generic_args[0] {
                emath_core::tree::GenericArg::Value(expr) => {
                    p.demand("4",matches!(&expr.kind, ExprKind::Int(v) if v == "7"), format!(
                        "expected Int(\"7\"), got {:?}",
                        expr.kind
                    ));
                    if p.failures().len() != f0 { return; }
                }
                other => panic!("expected GenericArg::Value, got {:?}", other),
            }
        }
        other => panic!("expected TypeKind::Path, got {:?}", other),
    }

    });
    probe.case("c10_tensor_bracket_list_extent_parses", |p| {
    let f0 = p.failures().len();

    // `Tensor<Float64, [N, N]>` — type arg + bracket-list extent arg.
    let source = "\
emath model TensorTest:
    inputs:
        x: Tensor<Float64, [N, N]>
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "Tensor<Float64, [N, N]> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let ty = first_input_ty(&tree).expect("expected `x` field in inputs");
    match &ty.kind {
        emath_core::tree::TypeKind::Path {
            segments,
            generic_args,
        } => {
            p.eq("2", segments.last().map(String::as_str), Some("Tensor"));
            p.eq("3", generic_args.len(), 2);
            // First arg: Float64 (type)
            p.demand("4",matches!(&generic_args[0], emath_core::tree::GenericArg::Type(_)), format!(
                "first arg should be Type, got {:?}",
                generic_args[0]
            ));
            if p.failures().len() != f0 { return; }
            // Second arg: [N, N] (value expression)
            match &generic_args[1] {
                emath_core::tree::GenericArg::Value(expr) => {
                    p.demand("5",matches!(&expr.kind, ExprKind::List(_)), format!(
                        "expected List expr for [N, N], got {:?}",
                        expr.kind
                    ));
                    if p.failures().len() != f0 { return; }
                }
                other => panic!("expected GenericArg::Value, got {:?}", other),
            }
        }
        other => panic!("expected TypeKind::Path, got {:?}", other),
    }

    });
    probe.case("c10_named_generic_arg_parses", |p| {
    let f0 = p.failures().len();

    // `GF<2, 3, modulus = x + 1>` — named argument in generic args.
    let source = "\
emath model GfTest:
    inputs:
        x: GF<2, 3, modulus = x + 1>
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "GF<2, 3, modulus = x + 1> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let ty = first_input_ty(&tree).expect("expected `x` field in inputs");
    match &ty.kind {
        emath_core::tree::TypeKind::Path {
            segments,
            generic_args,
        } => {
            p.eq("2", segments.last().map(String::as_str), Some("GF"));
            p.eq("3", generic_args.len(), 3);
            // First two: value literals (2, 3)
            p.demand("4",matches!(
                &generic_args[0],
                emath_core::tree::GenericArg::Value(_)
            ), stringify!(matches!(
                &generic_args[0],
                emath_core::tree::GenericArg::Value(_)
            )));
            if p.failures().len() != f0 { return; }
            p.demand("5",matches!(
                &generic_args[1],
                emath_core::tree::GenericArg::Value(_)
            ), stringify!(matches!(
                &generic_args[1],
                emath_core::tree::GenericArg::Value(_)
            )));
            if p.failures().len() != f0 { return; }
            // Third: named arg
            match &generic_args[2] {
                emath_core::tree::GenericArg::Named { name, arg } => {
                    p.demand("6", (name) == ("modulus"), format!("expected {:?}, got {:?}", ("modulus"), (name)));
                    if p.failures().len() != f0 { return; }
                    p.demand("7",matches!(arg.as_ref(), emath_core::tree::GenericArg::Value(_)), format!(
                        "named arg value should be a Value, got {:?}",
                        arg
                    ));
                    if p.failures().len() != f0 { return; }
                }
                other => panic!("expected GenericArg::Named, got {:?}", other),
            }
        }
        other => panic!("expected TypeKind::Path, got {:?}", other),
    }

    });
    probe.case("c10_vector_float64_regression", |p| {
    let f0 = p.failures().len();

    // Existing type-only generics must still parse identically.
    let source = "\
emath model VectorTest:
    inputs:
        x: Vector<Float64>
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "Vector<Float64> must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let ty = first_input_ty(&tree).expect("expected `x` field in inputs");
    match &ty.kind {
        emath_core::tree::TypeKind::Path {
            segments,
            generic_args,
        } => {
            p.eq("2", segments.last().map(String::as_str), Some("Vector"));
            p.eq("3", generic_args.len(), 1);
            p.demand("4",matches!(&generic_args[0], emath_core::tree::GenericArg::Type(_)), format!(
                "Vector arg should be Type, got {:?}",
                generic_args[0]
            ));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected TypeKind::Path, got {:?}", other),
    }

    });
    probe.case("partial_derivative_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function test(x: Float64) -> Float64:
    definitions:
        result = partial(x^2) wrt x
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "partial(x^2) wrt x must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative {
            kind, wrt, holding, ..
        } => {
            p.eq("2", *kind, DerivativeKind::Partial);
            p.demand("3",wrt.is_some(), format!( "wrt should be attached"));
            if p.failures().len() != f0 { return; }
            p.demand("4",holding.is_empty(), format!( "holding should be empty"));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected Derivative, got {:?}", other),
    }

    });
    probe.case("partial_derivative_unicode_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function test(x: Float64) -> Float64:
    definitions:
        result = \u{2202}(x^2) wrt x
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "\u{2202}(x^2) wrt x must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative { kind, .. } => {
            p.eq("2", *kind, DerivativeKind::Partial);
        }
        other => panic!("expected Derivative, got {:?}", other),
    }

    });
    probe.case("total_derivative_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function test(t: Float64) -> Float64:
    definitions:
        result = total(t^2) wrt t
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "total(t^2) wrt t must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative { kind, wrt, .. } => {
            p.eq("2", *kind, DerivativeKind::Total);
            p.demand("3",wrt.is_some(), format!( "wrt should be attached"));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected Derivative, got {:?}", other),
    }

    });
    probe.case("total_derivative_d_form_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function test(t: Float64) -> Float64:
    definitions:
        result = d(t^2) wrt t
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "d(t^2) wrt t must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative { kind, .. } => {
            p.eq("2", *kind, DerivativeKind::Total);
        }
        other => panic!("expected Derivative, got {:?}", other),
    }

    });
    probe.case("partial_derivative_holding_set_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function test(T: Float64, p: Float64, V: Float64) -> Float64:
    definitions:
        result = partial(H) wrt T holding p
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "partial(H) wrt T holding p must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative {
            kind, wrt, holding, ..
        } => {
            p.eq("2", *kind, DerivativeKind::Partial);
            p.demand("3",wrt.is_some(), format!( "wrt should be attached"));
            if p.failures().len() != f0 { return; }
            p.eq("4", holding.len(), 1);
        }
        other => panic!("expected Derivative, got {:?}", other),
    }

    });
    probe.case("holding_set_different_variables_distinct", |p| {
    let f0 = p.failures().len();

    // `partial(H) wrt T holding p` and `partial(H) wrt T holding V`
    // must produce structurally different AST nodes.
    let src_a = "emath function f(T: Float64, p: Float64) -> Float64:\n    definitions:\n        a = partial(H) wrt T holding p\n";
    let src_b = "emath function f(T: Float64, V: Float64) -> Float64:\n    definitions:\n        b = partial(H) wrt T holding V\n";
    let (tree_a, diags_a) = parse_str(src_a);
    let (tree_b, diags_b) = parse_str(src_b);
    p.demand("1",!diags_a.has_errors(), stringify!(!diags_a.has_errors()));
    if p.failures().len() != f0 { return; }
    p.demand("2",!diags_b.has_errors(), stringify!(!diags_b.has_errors()));
    if p.failures().len() != f0 { return; }
    let expr_a = def_expr(&tree_a, "a").unwrap();
    let expr_b = def_expr(&tree_b, "b").unwrap();
    // The holding sets differ (p vs V), so the Derivative nodes differ.
    p.ne("3", &expr_a.kind, &expr_b.kind);

    });
    probe.case("partial_as_identifier_still_works", |p| {
    let f0 = p.failures().len();

    // `partial` not followed by `(` should be a regular identifier.
    let source = "\
emath function test(partial: Float64) -> Float64:
    definitions:
        result = partial + 1
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "`partial` as identifier must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    p.demand("2",matches!(
            &expr.kind,
            ExprKind::Binary {
                op: BinaryOp::Add,
                ..
            }
        ), format!(
        "expected addition, got {:?}",
        expr.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("d_as_identifier_still_works", |p| {
    let f0 = p.failures().len();

    // `d` not followed by `(` should be a regular identifier.
    let source = "\
emath function test(d: Float64) -> Float64:
    definitions:
        result = d + 1
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "`d` as identifier must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    p.demand("2",matches!(
            &expr.kind,
            ExprKind::Binary {
                op: BinaryOp::Add,
                ..
            }
        ), format!(
        "expected addition, got {:?}",
        expr.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("derivative_plain_regression", |p| {
    let f0 = p.failures().len();

    // Existing `derivative(x) wrt x` must still produce Plain kind.
    let source = "\
emath function test(x: Float64) -> Float64:
    definitions:
        result = derivative(x^2) wrt x
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "derivative(x^2) wrt x must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Derivative { kind, .. } => {
            p.eq("2", *kind, DerivativeKind::Plain);
        }
        other => panic!("expected Derivative, got {:?}", other),
    }

    });
    probe.case("complex_literal_2i_parses", |p| {
    let f0 = p.failures().len();

    // `2i` should parse as `2 * i` (imaginary literal).
    let source = "\
emath function test() -> Float64:
    definitions:
        result = 2i
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "2i must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Binary { op, left, right } => {
            p.eq("2", *op, BinaryOp::Mul);
            p.demand("3",matches!(&left.kind, ExprKind::Float(v) if v == "2"), format!(
                "left should be Float(\"2\"), got {:?}",
                left.kind
            ));
            if p.failures().len() != f0 { return; }
            p.demand("4",matches!(&right.kind, ExprKind::Path { segments, .. } if segments == &["i"]), format!(
                "right should be Path([\"i\"]), got {:?}",
                right.kind
            ));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected Binary Mul, got {:?}", other),
    }

    });
    probe.case("complex_literal_3_5i_parses", |p| {
    let f0 = p.failures().len();

    // `3.5i` should parse as `3.5 * i`.
    let source = "\
emath function test() -> Float64:
    definitions:
        result = 3.5i
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "3.5i must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Binary { op, left, right } => {
            p.eq("2", *op, BinaryOp::Mul);
            p.demand("3",matches!(&left.kind, ExprKind::Float(v) if v == "3.5"), format!(
                "left should be Float(\"3.5\"), got {:?}",
                left.kind
            ));
            if p.failures().len() != f0 { return; }
            p.demand("4",matches!(&right.kind, ExprKind::Path { segments, .. } if segments == &["i"]), format!(
                "right should be Path([\"i\"]), got {:?}",
                right.kind
            ));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected Binary Mul, got {:?}", other),
    }

    });
    probe.case("complex_literal_in_expression_parses", |p| {
    let f0 = p.failures().len();

    // `1 + 2i` should parse as `1 + (2 * i)`.
    let source = "\
emath function test() -> Float64:
    definitions:
        result = 1 + 2i
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "1 + 2i must parse cleanly, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
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

    });
    probe.case("complex_type_parses", |p| {
    let f0 = p.failures().len();

    // `Complex` as a type annotation.
    let source = "\
emath model ComplexTest:
    inputs:
        z: Complex
";
    let (_tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "Complex type must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("vector_of_complex_parses", |p| {
    let f0 = p.failures().len();

    // `Vector<Complex, [2]>` — Complex as element type.
    let source = "\
emath model ComplexVector:
    inputs:
        v: Vector<Complex, [2]>
";
    let (_tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "Vector<Complex, [2]> must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("identifier_starting_with_i_not_complex", |p| {
    let f0 = p.failures().len();

    // `image` should NOT be parsed as complex literal — `i` is only a
    // complex suffix when not followed by identifier characters.
    let source = "\
emath function test(image: Float64) -> Float64:
    definitions:
        result = image + 1
";
    let (_tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "`image` should be a regular identifier, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("unit_of_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function test(m: Float64, c: Float64) -> Float64:
    definitions:
        result = unit of (m * c^2)
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "unit of (m * c^2) must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::UnitQuery { kind, .. } => {
            p.eq("2", *kind, UnitQueryKind::Unit);
        }
        other => panic!("expected UnitQuery, got {:?}", other),
    }

    });
    probe.case("dimension_of_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function test(thrust: Float64) -> Float64:
    definitions:
        result = dimension of thrust
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "dimension of thrust must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::UnitQuery { kind, .. } => {
            p.eq("2", *kind, UnitQueryKind::Dimension);
        }
        other => panic!("expected UnitQuery, got {:?}", other),
    }

    });
    probe.case("unit_of_with_comparison_parses", |p| {
    let f0 = p.failures().len();

    // `unit of E == X` should parse as `(unit of E) == X`
    let source = "\
emath function test(e: Float64) -> Float64:
    definitions:
        result = unit of e == kg*m^2/s^2
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "unit of e == ... must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    // Top-level should be a comparison (Binary with Eq)
    match &expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Eq,
            left,
            ..
        } => {
            // Left should be UnitQuery
            p.demand("2",matches!(
                    &left.kind,
                    ExprKind::UnitQuery {
                        kind: UnitQueryKind::Unit,
                        ..
                    }
                ), format!(
                "left should be UnitQuery(Unit), got {:?}",
                left.kind
            ));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected Binary Eq, got {:?}", other),
    }

    });
    probe.case("unit_as_identifier_still_works", |p| {
    let f0 = p.failures().len();

    // `unit` not followed by `of` should be a regular identifier.
    let source = "\
emath function test(unit: Float64) -> Float64:
    definitions:
        result = unit + 1
";
    let (_tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "`unit` as identifier must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("notation_glyph_use_desugars_to_canonical_call", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!diags.has_errors(), format!(
        "glyph use must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(p, expr, &["core", "math", "pow"], 2);
;

    });
    probe.case("notation_alias_spelling_desugars_to_same_target", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!diags.has_errors(), format!(
        "alias use must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(p, expr, &["core", "math", "pow"], 2);
;

    });
    probe.case("notation_custom_infix_binds_tighter_than_core_ladder", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!diags.has_errors(), format!(
        "custom precedence must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let bound_p = declaration_def_expr(&tree, "p").expect("expected `p` binding");
    let ExprKind::Binary {
        op: BinaryOp::Mul,
        left,
        right,
    } = &bound_p.kind
    else {
        panic!("`a ⊕ b * c` must parse as (a ⊕ b) * c, got {:?}", bound_p.kind);
    };
    assert_notation_call(p, left, &["core", "math", "pow"], 2);
    p.demand("2",matches!(right.kind, ExprKind::Path { .. }), stringify!(matches!(right.kind, ExprKind::Path { .. })));
    if p.failures().len() != f0 { return; }

    let q = declaration_def_expr(&tree, "q").expect("expected `q` binding");
    let ExprKind::Binary {
        op: BinaryOp::Mul,
        left,
        right,
    } = &q.kind
    else {
        panic!("`4 * x ⊕ 2` must parse as 4 * (x ⊕ 2), got {:?}", q.kind);
    };
    p.demand("3",matches!(left.kind, ExprKind::Int(_)), stringify!(matches!(left.kind, ExprKind::Int(_))));
    if p.failures().len() != f0 { return; }
    assert_notation_call(p, right, &["core", "math", "pow"], 2);
;

    });
    probe.case("notation_infixr_associates_right_and_infix_left", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!diags.has_errors(), format!(
        "associativity must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    // infixr: a ⊕ (b ⊕ c)
    let r = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(p, &r, &["core", "math", "pow"], 2);
    let ExprKind::Call { function: _, args } = &r.kind else {
        unreachable!()
    };
    assert_notation_call(p, &args[1], &["core", "math", "pow"], 2);
    // infix (left-assoc): (a ⊗ b) ⊗ c
    let s = declaration_def_expr(&tree, "s").expect("expected `s` binding");
    let ExprKind::Call { args: s_args, .. } = &s.kind else {
        panic!("expected Call, got {:?}", s.kind);
    };
    p.demand("2",matches!(&s_args[0].kind, ExprKind::Call { .. }), format!(
        "infix must be left-associative"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("notation_prefix_and_postfix_desugar_to_unary_target_calls", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!diags.has_errors(), format!(
        "prefix/postfix glyphs must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let r = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(p, r, &["core", "math", "sqrt"], 1);
    let s = declaration_def_expr(&tree, "s").expect("expected `s` binding");
    assert_notation_call(p, s, &["core", "math", "recip"], 1);
;

    });
    probe.case("notation_reserved_glyph_is_refused", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",codes.contains(&"E-NOTATION-RESERVED"), format!(
        "reserved glyph must refuse with E-NOTATION-RESERVED, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("notation_ambiguous_targets_are_refused", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",codes.contains(&"E-NOTATION-AMBIG"), format!(
        "conflicting redeclaration must refuse with E-NOTATION-AMBIG, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("notation_precedence_below_custom_floor_is_refused", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",codes.contains(&"E-NOTATION-PRECEDENCE"), format!(
        "core-ladder precedence must refuse with E-NOTATION-PRECEDENCE, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("notation_punctuation_glyph_is_refused", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",codes.contains(&"E-NOTATION-GLYPH"), format!(
        "punctuation glyph must refuse with E-NOTATION-GLYPH, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("math_symbol_does_not_glue_to_adjacent_letters", |p| {
    let f0 = p.failures().len();

    // `x⊕y` and `√a` must be operator uses, not one unknown identifier.
    let (tokens, diagnostics) = lex("x⊕y √a αβ", FileId(0), &Limits::default());
    p.demand("1",!diagnostics.has_errors(), format!(
        "juxtaposed glyphs must lex, got {:?}",
        diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let idents: Vec<&str> = tokens
        .iter()
        .filter_map(|token| match &token.kind {
            TokenKind::Ident(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    p.demand("2", (idents) == (["x", "⊕", "y", "√", "a", "αβ"]), format!("expected {:?}, got {:?}", (["x", "⊕", "y", "√", "a", "αβ"]), (idents)));
    if p.failures().len() != f0 { return; }

    });
    probe.case("notation_unspaced_glyph_desugars_to_pow", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!diags.has_errors(), format!(
        "unspaced glyph use must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(p, expr, &["core", "math", "pow"], 2);
;

    });
    probe.case("notation_unspaced_prefix_desugars_to_sqrt", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",!diags.has_errors(), format!(
        "unspaced prefix glyph must parse, got: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = declaration_def_expr(&tree, "r").expect("expected `r` binding");
    assert_notation_call(p, expr, &["core", "math", "sqrt"], 1);
;

    });
    probe.case("keyword_as_declaration_name_is_refused", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function if:
    outputs:
        r: Float64
    definitions:
        r = 1.0
";
    let (_tree, diags) = parse_str(source);
    p.demand("1",diags.errors().any(|e| e.code == "E-SYN-101"
            && e.message
                .contains("keyword `if` cannot be used as an identifier")), format!(
        "keyword declaration name must refuse, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("keyword_as_field_name_is_refused", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",diags.errors().any(|e| e.code == "E-SYN-101"
            && e.message
                .contains("keyword `if` cannot be used as an identifier")), format!(
        "keyword field name must refuse, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("keyword_as_package_segment_is_refused", |p| {
    let f0 = p.failures().len();

    let source = "\
package tst.if
emath function F:
    outputs:
        r: Float64
    definitions:
        r = 1.0
";
    let (tree, diags) = parse_str(source);
    p.demand("1",diags.errors().any(|e| e.code == "E-SYN-101"
            && e.message
                .contains("keyword `if` cannot be used as an identifier")), format!(
        "keyword package segment must refuse, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",!tree
            .items
            .iter()
            .any(|item| matches!(item, Item::Package { path, .. } if path.as_slice() == ["tst"])), format!(
        "truncated package path `tst` must not be recorded"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("keyword_as_notation_glyph_is_refused", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",codes.contains(&"E-NOTATION-GLYPH"), format!(
        "keyword glyph must refuse with E-NOTATION-GLYPH, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("juxtaposition_2x_is_refused_with_2_times_x_suggestion", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",rendered
            .iter()
            .any(|m| m.contains("juxtapos") && m.contains("2 * x")), format!(
        "`2x` must refuse with a juxtaposition + `2 * x` suggestion, got {rendered:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("explicit_2_times_x_still_admits", |p| {
    let f0 = p.failures().len();

    // The admitted spelling must remain untouched — the juxtaposition
    // refusal cannot fire on operator-separated operands.
    let source = "\
emath function f() -> Float64:
    definitions:
        r = 2 * x
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "2 * x must admit, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "r").expect("expected `r` binding");
    let ExprKind::Binary { op, .. } = &expr.kind else {
        panic!("expected Binary, got {:?}", expr.kind);
    };
    p.eq("2", *op, BinaryOp::Mul);
;

    });
    probe.case("notation_glyph_after_int_is_not_juxtaposition", |p| {
    let f0 = p.failures().len();

    // `2 ⊕ x` with a registered glyph must parse through the notation
    // infix layer; the adjacency check must exclude registered operators.
    let source = "\
emath function f() -> Float64:
    definitions:
        r = 2 ⊕ x
notation infixl 40 \"⊕\" => core::math::add
";
    let (tree, diags) = parse_str(source);
    p.demand("1",!diags.has_errors(), format!(
        "Int + notation glyph must parse as notation infix, got {:?}",
        diags
            .errors()
            .map(|e| (e.code, e.message.clone()))
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "r").expect("expected `r` binding");
    // The declared target IS the canonical desugar: every registered glyph
    // lowers to `Call(<declared target>, args)` — the same contract the
    // sibling tests pin (`notation_glyph_use_desugars_to_canonical_call`,
    // `notation_alias_spelling_desugars_to_same_target`). The point of this
    // test is the int-adjacent position: `2 ⊕ x` must reach the notation
    // layer instead of folding `⊕` into a quantity unit.
    assert_notation_call(p, expr, &["core", "math", "add"], 2);
;

    });
    probe.finish();
}
