//! Lexer/parser nucleus edge cases (constructor surface).

use emath_core::limits::Limits;
use emath_core::tree::{BinaryOp, ExprKind, Item, StmtKind};
use emath_core::{Diagnostic, FileId, SourceStore, Span};
use emath_syntax::lexer::{lex, lex_with_comments};
use emath_syntax::token::TokenKind;
use emath_syntax::{parse, parse_lossless, parse_str};

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

use emath_test_harness::{Probe, boot};

#[test]
fn edge_cases() {
    boot();
    let mut probe = Probe::new("Lexer/parser nucleus edge cases (constructor surface).");
    probe.case("empty_source_lexes_only_eof", |p| {
        let f0 = p.failures().len();

        let (tokens, diagnostics) = lex("", FileId(0), &Limits::default());
        p.demand(
            "1",
            !diagnostics.has_errors(),
            stringify!(!diagnostics.has_errors()),
        );
        if p.failures().len() != f0 {
            return;
        }
        p.eq("2", tokens.len(), 1);
        p.demand(
            "3",
            matches!(tokens[0].kind, TokenKind::Eof),
            stringify!(matches!(tokens[0].kind, TokenKind::Eof)),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case("exact_rational_double_slash_is_not_a_comment", |p| {
        let f0 = p.failures().len();

        // Spec literal family `3//7`. Treating `//` as a line comment used to
        // silently drop the denominator, leaving only Int("3").
        let (tokens, diagnostics) = lex("3//7", FileId(0), &Limits::default());
        p.demand(
            "1",
            !diagnostics.has_errors(),
            format!(
                "exact rational must lex cleanly, got {:?}",
                diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
        p.demand(
            "2",
            matches!(
                kinds.as_slice(),
                [TokenKind::Int(n), TokenKind::SlashSlash, TokenKind::Int(d), TokenKind::Eof]
                    if n == "3" && d == "7"
            ),
            format!("3//7 must be Int // Int, got {kinds:?}"),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            diags.errors().any(|e| e.code == "E-SYN-101"),
            format!(
                "non-integer denominator must be E-SYN-101, got {:?}",
                diags
                    .errors()
                    .map(|e| (e.code, e.message.clone()))
                    .collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case("hash_and_doc_comments_still_skip_the_line", |p| {
        let f0 = p.failures().len();

        let (tokens, diagnostics, comments) = lex_with_comments(
            "# ordinary\n/// documentation\nx",
            FileId(0),
            &Limits::default(),
        );
        p.demand(
            "1",
            !diagnostics.has_errors(),
            stringify!(!diagnostics.has_errors()),
        );
        if p.failures().len() != f0 {
            return;
        }
        let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
        p.demand(
            "2",
            matches!(
                kinds.as_slice(),
                [TokenKind::Ident(name), TokenKind::Eof] if name == "x"
            ),
            format!("comments must not emit tokens, got {kinds:?}"),
        );
        if p.failures().len() != f0 {
            return;
        }
        p.demand(
            "3",
            comments.iter().any(|c| c.text.starts_with('#')),
            format!("ordinary `#` comment must retain the marker, got {comments:?}"),
        );
        if p.failures().len() != f0 {
            return;
        }
        p.demand(
            "4",
            comments.iter().any(|c| c.text.starts_with("///")),
            format!("doc `///` comment must retain the marker, got {comments:?}"),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case("double_slash_comment_is_not_admitted", |p| {
        let f0 = p.failures().len();

        // Spec comments are `#` and `///` only. `// rest` is SlashSlash plus
        // whatever follows, not a silent skip of `rest`.
        let (tokens, diagnostics) = lex("a//b", FileId(0), &Limits::default());
        p.demand(
            "1",
            !diagnostics.has_errors(),
            stringify!(!diagnostics.has_errors()),
        );
        if p.failures().len() != f0 {
            return;
        }
        let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
        p.demand(
            "2",
            matches!(
                kinds.as_slice(),
                [
                    TokenKind::Ident(left),
                    TokenKind::SlashSlash,
                    TokenKind::Ident(right),
                    TokenKind::Eof
                ] if left == "a" && right == "b"
            ),
            format!("`//` must not eat the following ident, got {kinds:?}"),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case("braces_suppress_newlines_like_parens", |p| {
        let f0 = p.failures().len();

        // Spec: NEWLINE is suppressed inside `()`, `[]`, and `{}`.
        let (paren, _) = lex("(\n1\n)", FileId(0), &Limits::default());
        let (brace, _) = lex("{\n1\n}", FileId(0), &Limits::default());
        let paren_kinds: Vec<&TokenKind> = paren.iter().map(|t| &t.kind).collect();
        let brace_kinds: Vec<&TokenKind> = brace.iter().map(|t| &t.kind).collect();
        p.demand(
            "1",
            !paren_kinds.iter().any(|k| matches!(k, TokenKind::Newline)),
            format!("parens already suppress newlines, got {paren_kinds:?}"),
        );
        if p.failures().len() != f0 {
            return;
        }
        p.demand(
            "2",
            !brace_kinds.iter().any(|k| matches!(k, TokenKind::Newline)),
            format!("braces must suppress newlines, got {brace_kinds:?}"),
        );
        if p.failures().len() != f0 {
            return;
        }
        p.demand(
            "3",
            matches!(
                brace_kinds.as_slice(),
                [
                    TokenKind::LBrace,
                    TokenKind::Int(n),
                    TokenKind::RBrace,
                    TokenKind::Eof
                ] if n == "1"
            ),
            format!("expected brace-wrapped int, got {brace_kinds:?}"),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case("tabs_are_rejected_in_canonical_source", |p| {
        let f0 = p.failures().len();

        let (_, diagnostics) = lex("a\tb", FileId(0), &Limits::default());
        p.demand(
            "1",
            diagnostics.errors().any(|error| error.code == "E-SYN-101"),
            format!(
                "tab must be a typed refusal, got {:?}",
                diagnostics
                    .errors()
                    .map(|e| (e.code, e.message.clone()))
                    .collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case(
        "invalid_unicode_escape_keeps_string_and_following_ident",
        |p| {
            let f0 = p.failures().len();

            // `\u` without `{` used to `break` the string loop, emit a truncated Str,
            // and retokenize `x"` as Ident + junk. Recovery must stay in the string.
            let (tokens, diagnostics) = lex(r#""\ux" after"#, FileId(0), &Limits::default());
            p.demand(
                "1",
                diagnostics.errors().any(|error| error.code == "E-SYN-109"),
                format!("expected E-SYN-109 for malformed \\u"),
            );
            if p.failures().len() != f0 {
                return;
            }
            let kinds: Vec<&TokenKind> = tokens.iter().map(|token| &token.kind).collect();
            p.demand(
                "2",
                kinds.iter().any(|kind| matches!(kind, TokenKind::Str(_))),
                format!("still emits one Str token: {kinds:?}"),
            );
            if p.failures().len() != f0 {
                return;
            }
            p.demand(
                "3",
                kinds
                    .iter()
                    .any(|kind| matches!(kind, TokenKind::Ident(name) if name == "after")),
                format!("tail after the closing quote must lex as Ident(after), got {kinds:?}"),
            );
            if p.failures().len() != f0 {
                return;
            }
        },
    );

    probe.case("oversized_unicode_escape_hex_is_diagnosed", |p| {
        let f0 = p.failures().len();

        // Hex wider than u32 used to be silently skipped (no diagnostic, no char).
        let (tokens, diagnostics) = lex(r#""\u{100000000}""#, FileId(0), &Limits::default());
        p.demand(
            "1",
            diagnostics.errors().any(|error| error.code == "E-SYN-109"),
            format!("expected E-SYN-109 for overflow hex"),
        );
        if p.failures().len() != f0 {
            return;
        }
        let value = tokens
            .iter()
            .find_map(|token| match &token.kind {
                TokenKind::Str(value) => Some(value.as_str()),
                _ => None,
            })
            .expect("expected a Str token");
        p.demand(
            "2",
            value.is_empty(),
            format!("overflow escape must not invent a char, got {value:?}"),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case("empty_unicode_escape_is_diagnosed", |p| {
        let f0 = p.failures().len();

        let (_, diagnostics) = lex(r#""\u{}""#, FileId(0), &Limits::default());
        p.demand(
            "1",
            diagnostics.errors().any(|error| error.code == "E-SYN-109"),
            stringify!(diagnostics.errors().any(|error| error.code == "E-SYN-109")),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case("line_col_max_offset_does_not_overflow", |p| {
        let mut store = SourceStore::new();
        let id = store.add("t.emath", "");
        let file = store.get(id).expect("file");
        let (line, col) = file.line_col(u32::MAX);
        p.eq("1", line, 1);
        p.eq("2", col, u32::MAX);
    });

    probe.case(
        "token_limit_does_not_emit_unbounded_trailing_dedents",
        |p| {
            let f0 = p.failures().len();

            let limits = Limits {
                max_tokens: 4,
                ..Limits::default()
            };
            // Many successively deeper indents after the budget is spent must not
            // append one Dedent per phantom stack frame past max_tokens (+Eof).
            let source = "a\n b\n  c\n   d\n    e\n     f\n      g\n";
            let (tokens, diagnostics) = lex(source, FileId(0), &limits);
            p.demand(
                "1",
                diagnostics.errors().any(|error| error.code == "E-SYN-108"),
                format!("expected token-limit diagnostic"),
            );
            if p.failures().len() != f0 {
                return;
            }
            let dedents = tokens
                .iter()
                .filter(|token| matches!(token.kind, TokenKind::Dedent))
                .count();
            p.demand(
                "2",
                tokens.len() <= limits.max_tokens + 1,
                format!(
                    "tokens={} exceeds max_tokens+Eof; dedents={dedents}",
                    tokens.len()
                ),
            );
            if p.failures().len() != f0 {
                return;
            }
        },
    );

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
        p.demand(
            "1",
            (caret_line) == ("   ^"),
            format!("expected {:?}, got {:?}", ("   ^"), (caret_line)),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case("source_over_byte_limit_refuses_without_scanning", |p| {
        let f0 = p.failures().len();

        let limits = Limits {
            max_source_bytes: 8,
            ..Limits::default()
        };
        let source = "aaaaaaaaaa"; // 10 bytes > 8
        let (tokens, diagnostics) = lex(source, FileId(0), &limits);
        p.demand(
            "1",
            diagnostics.errors().any(|error| error.code == "E-SYN-116"),
            format!("expected E-SYN-116 for oversized source"),
        );
        if p.failures().len() != f0 {
            return;
        }
        p.eq("2", tokens.len(), 1);
        p.demand(
            "3",
            matches!(tokens[0].kind, TokenKind::Eof),
            stringify!(matches!(tokens[0].kind, TokenKind::Eof)),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case(
        "parse_skips_scratch_expand_when_source_exceeds_limits",
        |p| {
            let f0 = p.failures().len();

            let limits = Limits {
                max_source_bytes: 8,
                ..Limits::default()
            };
            let source = "aaaaaaaaaa"; // 10 bytes > 8
            let (_, diagnostics) = parse(source, FileId(0), &limits);
            p.demand(
                "1",
                diagnostics.errors().any(|error| error.code == "E-SYN-116"),
                format!("expected E-SYN-116 without expanding oversized scratch"),
            );
            if p.failures().len() != f0 {
                return;
            }
            let lossless = parse_lossless(source, FileId(0), &limits);
            p.demand(
                "2",
                lossless
                    .diagnostics
                    .errors()
                    .any(|error| error.code == "E-SYN-116"),
                format!("expected E-SYN-116 from lossless parse of oversized source"),
            );
            if p.failures().len() != f0 {
                return;
            }
            p.demand(
                "3",
                lossless.comments.is_empty(),
                format!("oversized lossless parse must not retain comments"),
            );
            if p.failures().len() != f0 {
                return;
            }
        },
    );

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
        p.demand(
            "1",
            !header.contains('\n') && header.contains("first injected:1:1: E-FAKE: second"),
            format!("message controls must flatten into one header line, got {rendered:?}"),
        );
        if p.failures().len() != f0 {
            return;
        }
        p.demand(
            "2",
            rendered.contains("= help: help line"),
            format!("help controls must flatten, got {rendered:?}"),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "colon-form conditional must parse cleanly, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "2",
            matches!(
                &condition.kind,
                ExprKind::Binary {
                    op: BinaryOp::Gt,
                    ..
                }
            ),
            format!("condition should be x > 0, got {:?}", condition.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
        p.demand(
            "3",
            matches!(&then_value.kind, ExprKind::Int(_)),
            format!("then_value should be 1, got {:?}", then_value.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
        p.demand(
            "4",
            matches!(&else_value.kind, ExprKind::Int(_)),
            format!("else_value should be 0, got {:?}", else_value.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            matches!(&expr.kind, ExprKind::Float(_)),
            format!(
                "x should be bound to a Float literal (9.81), not an Index, got {:?}",
                expr.kind
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case("c3_variable_indexing_still_works", |p| {
        let f0 = p.failures().len();

        // `v[0]` on a non-literal primary (path/identifier) must still parse
        // as indexing.  The C3 fix only refuses `[` after numeric literals.
        let source = "\
emath function idx(v) -> Float64:
    definitions:
        x = v[0]
";
        let (tree, diags) = parse_str(source);
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "variable indexing must parse cleanly, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        let expr = def_expr(&tree, "x").expect("expected `x` binding");
        let ExprKind::Index { value, indices } = &expr.kind else {
            panic!("expected Index expression, got {:?}", expr.kind);
        };
        p.demand(
            "2",
            matches!(&value.kind, ExprKind::Path { .. }),
            format!("indexed value should be a path (v), got {:?}", value.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
        p.eq("3", indices.len(), 1);
    });

    probe.case("c3_list_literal_indexing_still_works", |p| {
        let f0 = p.failures().len();

        // `[[1, 2], [3, 4]][0]` on a list literal must still parse as indexing.
        // The C3 fix only refuses `[` after numeric scalar literals (Int,
        // Float, Quantity), not after list/tuple primaries.
        let source = "\
emath function mat():
    definitions:
        row = [[1, 2], [3, 4]][0]
";
        let (tree, diags) = parse_str(source);
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "list literal indexing must parse cleanly, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        let expr = def_expr(&tree, "row").expect("expected `row` binding");
        let ExprKind::Index { value, .. } = &expr.kind else {
            panic!("expected Index expression, got {:?}", expr.kind);
        };
        p.demand(
            "2",
            matches!(&value.kind, ExprKind::List(_)),
            format!("indexed value should be a list, got {:?}", value.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "==> must parse cleanly, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        let expr = def_expr(&tree, "result").expect("expected `result` binding");
        p.demand(
            "2",
            matches!(
                &expr.kind,
                ExprKind::Binary {
                    op: BinaryOp::Imply,
                    ..
                }
            ),
            format!("expected Imply, got {:?}", expr.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "<==> must parse cleanly, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        let expr = def_expr(&tree, "result").expect("expected `result` binding");
        p.demand(
            "2",
            matches!(
                &expr.kind,
                ExprKind::Binary {
                    op: BinaryOp::Iff,
                    ..
                }
            ),
            format!("expected Iff, got {:?}", expr.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "chained ==> must parse cleanly, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        let expr = def_expr(&tree, "result").expect("expected `result` binding");
        let ExprKind::Binary { op, left, right } = &expr.kind else {
            panic!("expected Binary, got {:?}", expr.kind);
        };
        p.eq("2", *op, BinaryOp::Imply);
        // Right child should also be Imply (right-associative).
        p.demand(
            "3",
            matches!(
                &right.kind,
                ExprKind::Binary {
                    op: BinaryOp::Imply,
                    ..
                }
            ),
            format!("right child should be Imply, got {:?}", right.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
        // Left child should be a Bool literal (true), not another Imply.
        p.demand(
            "4",
            matches!(&left.kind, ExprKind::Bool(true)),
            format!("left child should be true, got {:?}", left.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "plain expression must parse cleanly, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        // Verify the expression is just `true`, not something involving =>.
        let expr = def_expr(&tree, "result").expect("expected `result` binding");
        p.demand(
            "2",
            matches!(&expr.kind, ExprKind::Bool(true)),
            format!("expected Bool(true), got {:?}", expr.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "binder with guard must parse cleanly, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        let expr = def_expr(&tree, "result").expect("expected `result` binding");
        match &expr.kind {
            ExprKind::CallableBinder { .. } => {}
            other => panic!("expected CallableBinder, got {:?}", other),
        }
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
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "`partial` as identifier must parse, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        let expr = def_expr(&tree, "result").expect("expected `result` binding");
        p.demand(
            "2",
            matches!(
                &expr.kind,
                ExprKind::Binary {
                    op: BinaryOp::Add,
                    ..
                }
            ),
            format!("expected addition, got {:?}", expr.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "`d` as identifier must parse, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        let expr = def_expr(&tree, "result").expect("expected `result` binding");
        p.demand(
            "2",
            matches!(
                &expr.kind,
                ExprKind::Binary {
                    op: BinaryOp::Add,
                    ..
                }
            ),
            format!("expected addition, got {:?}", expr.kind),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "`image` should be a regular identifier, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
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
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "`unit` as identifier must parse, got: {:?}",
                diags.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case("math_symbol_does_not_glue_to_adjacent_letters", |p| {
        let f0 = p.failures().len();

        // `x⊕y` and `√a` must be operator uses, not one unknown identifier.
        let (tokens, diagnostics) = lex("x⊕y √a αβ", FileId(0), &Limits::default());
        p.demand(
            "1",
            !diagnostics.has_errors(),
            format!(
                "juxtaposed glyphs must lex, got {:?}",
                diagnostics.errors().map(|e| e.code).collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        let idents: Vec<&str> = tokens
            .iter()
            .filter_map(|token| match &token.kind {
                TokenKind::Ident(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        p.demand(
            "2",
            (idents) == (["x", "⊕", "y", "√", "a", "αβ"]),
            format!(
                "expected {:?}, got {:?}",
                (["x", "⊕", "y", "√", "a", "αβ"]),
                (idents)
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            diags.errors().any(|e| {
                e.code == "E-SYN-101"
                    && e.message
                        .contains("keyword `if` cannot be used as an identifier")
            }),
            format!(
                "keyword declaration name must refuse, got {:?}",
                diags
                    .errors()
                    .map(|e| (e.code, e.message.clone()))
                    .collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            diags.errors().any(|e| {
                e.code == "E-SYN-101"
                    && e.message
                        .contains("keyword `if` cannot be used as an identifier")
            }),
            format!(
                "keyword field name must refuse, got {:?}",
                diags
                    .errors()
                    .map(|e| (e.code, e.message.clone()))
                    .collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
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
        p.demand(
            "1",
            diags.errors().any(|e| {
                e.code == "E-SYN-101"
                    && e.message
                        .contains("keyword `if` cannot be used as an identifier")
            }),
            format!(
                "keyword package segment must refuse, got {:?}",
                diags
                    .errors()
                    .map(|e| (e.code, e.message.clone()))
                    .collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        p.demand(
            "2",
            !tree.items.iter().any(
                |item| matches!(item, Item::Package { path, .. } if path.as_slice() == ["tst"]),
            ),
            format!("truncated package path `tst` must not be recorded"),
        );
        if p.failures().len() != f0 {
            return;
        }
    });

    probe.case(
        "juxtaposition_2x_is_refused_with_2_times_x_suggestion",
        |p| {
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
            p.demand(
                "1",
                rendered
                    .iter()
                    .any(|m| m.contains("juxtapos") && m.contains("2 * x")),
                format!(
                    "`2x` must refuse with a juxtaposition + `2 * x` suggestion, got {rendered:?}"
                ),
            );
            if p.failures().len() != f0 {
                return;
            }
        },
    );

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
        p.demand(
            "1",
            !diags.has_errors(),
            format!(
                "2 * x must admit, got {:?}",
                diags
                    .errors()
                    .map(|e| (e.code, e.message.clone()))
                    .collect::<Vec<_>>()
            ),
        );
        if p.failures().len() != f0 {
            return;
        }
        let expr = def_expr(&tree, "r").expect("expected `r` binding");
        let ExprKind::Binary { op, .. } = &expr.kind else {
            panic!("expected Binary, got {:?}", expr.kind);
        };
        p.eq("2", *op, BinaryOp::Mul);
    });

    probe.finish();
}
