//! Intent-driven tests for B04 (limit/sample_limit), B06 (series),
//! and B18 (asymptotic equivalence `~~`).
//!
//! These tests verify PARSING behavior: that the new constructs produce
//! the correct AST nodes, that contextual keywords don't break user
//! identifiers, and that the formatter roundtrips.

use emath_core::limits::Limits;
use emath_core::tree::{BinderKind, BinaryOp, ExprKind, LimitDirection, StmtKind};
use emath_core::FileId;
use emath_syntax::formatter::format;
use emath_syntax::{parse_lossless, parse_str};

/// Extract the expression bound to `name` in a declaration's
/// `definitions:` section.
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
                        return Some(value)
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

fn assert_parses_clean(source: &str) -> emath_core::tree::SyntaxTree {
    let (tree, diags) = parse_str(source);
    assert!(
        !diags.has_errors(),
        "must parse cleanly, got errors: {:?}",
        diags.errors().map(|e| e.code).collect::<Vec<_>>()
    );
    tree
}

// ---- B04: limit binder as claim -------------------------------------------

#[test]
fn limit_parses_as_claim_node() {
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        result = limit x -> 0: x * x
";
    let tree = assert_parses_clean(source);
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Limit {
            var,
            target,
            direction,
            body,
        } => {
            assert_eq!(var, "x");
            assert!(matches!(&target.kind, ExprKind::Int(t) if t == "0"));
            assert_eq!(*direction, LimitDirection::TwoSided);
            assert!(
                matches!(&body.kind, ExprKind::Binary { op: BinaryOp::Mul, .. }),
                "body should be x * x"
            );
        }
        other => panic!("expected ExprKind::Limit, got {other:?}"),
    }
}

#[test]
fn one_sided_limits_parse_correctly() {
    // From above: 0+
    let source_plus = "\
emath function f(x: Float64) -> Float64:
    definitions:
        a = limit x -> 0+: 1 / x
";
    let tree = assert_parses_clean(source_plus);
    let expr = def_expr(&tree, "a").expect("expected `a` binding");
    match &expr.kind {
        ExprKind::Limit { direction, .. } => {
            assert_eq!(*direction, LimitDirection::FromAbove);
        }
        other => panic!("expected Limit, got {other:?}"),
    }

    // From below: 0-
    let source_minus = "\
emath function f(x: Float64) -> Float64:
    definitions:
        b = limit x -> 0-: 1 / x
";
    let tree = assert_parses_clean(source_minus);
    let expr = def_expr(&tree, "b").expect("expected `b` binding");
    match &expr.kind {
        ExprKind::Limit { direction, .. } => {
            assert_eq!(*direction, LimitDirection::FromBelow);
        }
        other => panic!("expected Limit, got {other:?}"),
    }
}

// ---- B04: sample_limit as computation -------------------------------------

#[test]
fn sample_limit_parses_as_computation() {
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        result = sample_limit x -> 0: sin(x) / x
";
    let tree = assert_parses_clean(source);
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::SampleLimit {
            var,
            target,
            direction,
            body,
        } => {
            assert_eq!(var, "x");
            assert!(matches!(&target.kind, ExprKind::Int(t) if t == "0"));
            assert_eq!(*direction, LimitDirection::TwoSided);
            // body is sin(x) / x → Binary(Div, Call(sin, [x]), x)
            assert!(
                matches!(&body.kind, ExprKind::Binary { op: BinaryOp::Div, .. }),
                "body should be a division"
            );
        }
        other => panic!("expected ExprKind::SampleLimit, got {other:?}"),
    }
}

// ---- B06: series with contextual keyword ----------------------------------

#[test]
fn series_parses_with_contextual_keyword() {
    let source = "\
emath function s(n: Nat) -> Float64:
    definitions:
        result = series k in 0..10: 1 / (k + 1)
";
    let tree = assert_parses_clean(source);
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Binder {
            kind,
            binders,
            body,
            ..
        } => {
            assert_eq!(*kind, BinderKind::Series, "should be Series binder");
            assert_eq!(binders.len(), 1);
            assert_eq!(binders[0].name, "k");
            assert!(
                matches!(&body.kind, ExprKind::Binary { op: BinaryOp::Div, .. }),
                "body should be a division"
            );
        }
        other => panic!("expected Binder with Series kind, got {other:?}"),
    }
}

// ---- B18: asymptotic equivalence ~~ ---------------------------------------

#[test]
fn asymp_parses_as_binary_op() {
    let source = "\
emath function f(n: Nat) -> Float64:
    definitions:
        result = factorial(n) ~~ n ^ n
";
    let tree = assert_parses_clean(source);
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Binary { op, left, right } => {
            assert_eq!(*op, BinaryOp::Asymp, "operator should be Asymp");
            assert!(
                matches!(&left.kind, ExprKind::Call { .. }),
                "left should be factorial(n)"
            );
            assert!(
                matches!(&right.kind, ExprKind::Binary { op: BinaryOp::Pow, .. }),
                "right should be n^n"
            );
        }
        other => panic!("expected Binary with Asymp, got {other:?}"),
    }
}

// ---- Contextual keyword safety --------------------------------------------

#[test]
fn contextual_keywords_remain_valid_identifiers() {
    // `limit` used as a variable name must NOT trigger limit parsing.
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        limit = 5
        series = 3
        result = limit + series
";
    let tree = assert_parses_clean(source);
    let expr = def_expr(&tree, "limit").expect("expected `limit` binding");
    assert!(
        matches!(&expr.kind, ExprKind::Int(t) if t == "5"),
        "`limit` as identifier should bind to 5, got {:?}",
        expr.kind
    );
    let expr = def_expr(&tree, "series").expect("expected `series` binding");
    assert!(
        matches!(&expr.kind, ExprKind::Int(t) if t == "3"),
        "`series` as identifier should bind to 3, got {:?}",
        expr.kind
    );
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    assert!(
        matches!(&expr.kind, ExprKind::Binary { op: BinaryOp::Add, .. }),
        "result should be limit + series"
    );
}

// ---- Formatter roundtrip ---------------------------------------------------

#[test]
fn formatter_roundtrips_new_constructs() {
    let cases = [
        "emath function f(x: Float64) -> Float64:\n    definitions:\n        result = limit x -> 0: x * x\n",
        "emath function f(x: Float64) -> Float64:\n    definitions:\n        result = limit x -> 0+: 1 / x\n",
        "emath function f(x: Float64) -> Float64:\n    definitions:\n        result = limit x -> 0-: 1 / x\n",
        "emath function f(x: Float64) -> Float64:\n    definitions:\n        result = sample_limit x -> 0: sin(x) / x\n",
        "emath function s(n: Nat) -> Float64:\n    definitions:\n        result = series k in 0..10: 1 / (k + 1)\n",
        "emath function f(n: Nat) -> Float64:\n    definitions:\n        result = factorial(n) ~~ n ^ n\n",
    ];
    for source in cases {
        let parsed = parse_lossless(source, FileId(0), &Limits::default());
        assert!(
            !parsed.diagnostics.has_errors(),
            "source must parse cleanly: {source}"
        );
        let once = format(&parsed.tree, &parsed.comments);
        let reparsed = parse_lossless(&once, FileId(0), &Limits::default());
        assert!(
            !reparsed.diagnostics.has_errors(),
            "formatted output must parse back: {once}"
        );
        let twice = format(&reparsed.tree, &reparsed.comments);
        assert_eq!(once, twice, "format must be idempotent");
    }
}

// ---- Negative: `~~` token lexes correctly ---------------------------------

#[test]
fn single_tilde_is_rejected() {
    use emath_syntax::lexer::lex;
    let (_, diags) = lex("a ~ b", FileId(0), &Limits::default());
    assert!(
        diags.errors().any(|e| e.code == "E-SYN-101"),
        "single `~` should be rejected, use `~~`"
    );
}
