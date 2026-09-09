//! Intent-driven tests for B04 (limit/sample_limit), B06 (series),
//! and B18 (asymptotic equivalence `~~`).
//!
//! These tests verify PARSING behavior: that the new constructs produce
//! the correct AST nodes, that contextual keywords don't break user
//! identifiers, and that the formatter roundtrips.

use emath_core::FileId;
use emath_core::limits::Limits;
use emath_core::tree::{BinaryOp, BinderKind, ExprKind, LimitDirection, StmtKind};
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
                        return Some(value);
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

fn assert_parses_clean(p: &mut Probe, source: &str) -> emath_core::tree::SyntaxTree {
    let (tree, diags) = parse_str(source);
    p.demand(
        "parse",
        !diags.has_errors(),
        format!(
            "must parse cleanly, got errors: {:?}",
            diags.errors().map(|e| e.code).collect::<Vec<_>>()
        ),
    );
    tree
}

// ---- B04: limit binder as claim -------------------------------------------

// ---- B04: sample_limit as computation -------------------------------------

// ---- B06: series with contextual keyword ----------------------------------

// ---- B18: asymptotic equivalence ~~ ---------------------------------------

// ---- Contextual keyword safety --------------------------------------------

// ---- Formatter roundtrip ---------------------------------------------------

// ---- Negative: `~~` token lexes correctly ---------------------------------

use emath_test_harness::{Probe, boot};

#[test]
fn limits_series() {
    boot();
    let mut probe = Probe::new("Intent-driven tests for B04 (limit/sample_limit), B06 (series), and B18 (asymptotic equivalence `~~`). These tests verify PARSING behavior: that the");
    probe.case("limit_parses_as_claim_node", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        result = limit x -> 0: x * x
";
    let tree = assert_parses_clean(p, source);
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Limit {
            var,
            target,
            direction,
            body,
        } => {
            p.demand("1", (var) == ("x"), format!("expected {:?}, got {:?}", ("x"), (var)));
            if p.failures().len() != f0 { return; }
            p.demand("2",matches!(&target.kind, ExprKind::Int(t) if t == "0"), stringify!(matches!(&target.kind, ExprKind::Int(t) if t == "0")));
            if p.failures().len() != f0 { return; }
            p.eq("3", *direction, LimitDirection::TwoSided);
            p.demand("4",matches!(
                    &body.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Mul,
                        ..
                    }
                ), format!(
                "body should be x * x"
            ));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected ExprKind::Limit, got {other:?}"),
    }

    });
    probe.case("one_sided_limits_parse_correctly", |p| {

    // From above: 0+
    let source_plus = "\
emath function f(x: Float64) -> Float64:
    definitions:
        a = limit x -> 0+: 1 / x
";
    let tree = assert_parses_clean(p, source_plus);
    let expr = def_expr(&tree, "a").expect("expected `a` binding");
    match &expr.kind {
        ExprKind::Limit { direction, .. } => {
            p.eq("1", *direction, LimitDirection::FromAbove);
        }
        other => panic!("expected Limit, got {other:?}"),
    }

    // From below: 0-
    let source_minus = "\
emath function f(x: Float64) -> Float64:
    definitions:
        b = limit x -> 0-: 1 / x
";
    let tree = assert_parses_clean(p, source_minus);
    let expr = def_expr(&tree, "b").expect("expected `b` binding");
    match &expr.kind {
        ExprKind::Limit { direction, .. } => {
            p.eq("2", *direction, LimitDirection::FromBelow);
        }
        other => panic!("expected Limit, got {other:?}"),
    }

    });
    probe.case("sample_limit_parses_as_computation", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        result = sample_limit x -> 0: sin(x) / x
";
    let tree = assert_parses_clean(p, source);
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::SampleLimit {
            var,
            target,
            direction,
            body,
        } => {
            p.demand("1", (var) == ("x"), format!("expected {:?}, got {:?}", ("x"), (var)));
            if p.failures().len() != f0 { return; }
            p.demand("2",matches!(&target.kind, ExprKind::Int(t) if t == "0"), stringify!(matches!(&target.kind, ExprKind::Int(t) if t == "0")));
            if p.failures().len() != f0 { return; }
            p.eq("3", *direction, LimitDirection::TwoSided);
            // body is sin(x) / x → Binary(Div, Call(sin, [x]), x)
            p.demand("4",matches!(
                    &body.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Div,
                        ..
                    }
                ), format!(
                "body should be a division"
            ));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected ExprKind::SampleLimit, got {other:?}"),
    }

    });
    probe.case("series_parses_with_contextual_keyword", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function s(n: Nat) -> Float64:
    definitions:
        result = series k in 0..10: 1 / (k + 1)
";
    let tree = assert_parses_clean(p, source);
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Binder {
            kind,
            binders,
            body,
            ..
        } => {
            p.eq("1", *kind, BinderKind::Series);
            p.eq("2", binders.len(), 1);
            p.demand("3", (binders[0].name) == ("k"), format!("expected {:?}, got {:?}", ("k"), (binders[0].name)));
            if p.failures().len() != f0 { return; }
            p.demand("4",matches!(
                    &body.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Div,
                        ..
                    }
                ), format!(
                "body should be a division"
            ));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected Binder with Series kind, got {other:?}"),
    }

    });
    probe.case("asymp_parses_as_binary_op", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f(n: Nat) -> Float64:
    definitions:
        result = factorial(n) ~~ n ^ n
";
    let tree = assert_parses_clean(p, source);
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    match &expr.kind {
        ExprKind::Binary { op, left, right } => {
            p.eq("1", *op, BinaryOp::Asymp);
            p.demand("2",matches!(&left.kind, ExprKind::Call { .. }), format!(
                "left should be factorial(n)"
            ));
            if p.failures().len() != f0 { return; }
            p.demand("3",matches!(
                    &right.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Pow,
                        ..
                    }
                ), format!(
                "right should be n^n"
            ));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected Binary with Asymp, got {other:?}"),
    }

    });
    probe.case("contextual_keywords_remain_valid_identifiers", |p| {
    let f0 = p.failures().len();

    // `limit` used as a variable name must NOT trigger limit parsing.
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        limit = 5
        series = 3
        result = limit + series
";
    let tree = assert_parses_clean(p, source);
    let expr = def_expr(&tree, "limit").expect("expected `limit` binding");
    p.demand("1",matches!(&expr.kind, ExprKind::Int(t) if t == "5"), format!(
        "`limit` as identifier should bind to 5, got {:?}",
        expr.kind
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "series").expect("expected `series` binding");
    p.demand("2",matches!(&expr.kind, ExprKind::Int(t) if t == "3"), format!(
        "`series` as identifier should bind to 3, got {:?}",
        expr.kind
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "result").expect("expected `result` binding");
    p.demand("3",matches!(
            &expr.kind,
            ExprKind::Binary {
                op: BinaryOp::Add,
                ..
            }
        ), format!(
        "result should be limit + series"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("formatter_roundtrips_new_constructs", |p| {
    let f0 = p.failures().len();

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
        p.demand("1",!parsed.diagnostics.has_errors(), format!(
            "source must parse cleanly: {source}"
        ));
        if p.failures().len() != f0 { return; }
        let once = format(&parsed.tree, &parsed.comments);
        let reparsed = parse_lossless(&once, FileId(0), &Limits::default());
        p.demand("2",!reparsed.diagnostics.has_errors(), format!(
            "formatted output must parse back: {once}"
        ));
        if p.failures().len() != f0 { return; }
        let twice = format(&reparsed.tree, &reparsed.comments);
        p.eq("3", &once, &twice);
    }

    });
    probe.case("single_tilde_is_rejected", |p| {
    let f0 = p.failures().len();

    use emath_syntax::lexer::lex;
    let (_, diags) = lex("a ~ b", FileId(0), &Limits::default());
    p.demand("1",diags.errors().any(|e| e.code == "E-SYN-101"), format!(
        "single `~` should be rejected, use `~~`"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
