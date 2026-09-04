//!: U6 match expressions.
//!
//! `match subject { pattern => value, ... }` is expression-position
//! sugar for `cases` (U1): a literal pattern (Int/Float/Str/Bool, with
//! an optional leading `-`) becomes a `subject == pattern` condition,
//! and the mandatory FINAL catch-all (`_`, or a binding name whose arm
//! value has the subject substituted for the name) becomes the else
//! arm. Desugaring to the existing `Cases` kind means lowering,
//! same-type arm checks (`E-TYPE-012`), and the formatter are
//! inherited, not duplicated. No new tree variant, no new IR.
//!
//! Totality is a parse-time guarantee (mirror of `cases`' mandatory
//! `else`): a match with no catch-all arm refuses `E-SYN-110`, and a
//! catch-all that is not the last arm refuses `E-SYN-101`
//! (first-match-wins would make later arms unreachable).
//!
//! Failure-first: every pin below is RED until the parser arm lands
//! (`match` is a reserved keyword that previously had no parse path).

use emath_core::limits::Limits;
use emath_core::tree::{BinaryOp, Expr, ExprKind, StmtKind};
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn check_source(source: &str) -> emath_sema::admit::CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned("match-expressions", source)
}

/// Parse a one-definition function and return the definition's value
/// expression (`f = <expr>` in `definitions:`).
fn defn_expr(source: &str) -> Expr {
    install_source_parser();
    let (tree, diags) = emath_syntax::parse_str(source);
    assert!(!diags.has_errors(), "{diags:?}");
    let Some(emath_core::tree::Item::Declaration(decl)) = tree.items.last() else {
        panic!("declaration expected, got {:?}", tree.items.last());
    };
    let defs = decl
        .sections_vec()
        .into_iter()
        .find(|section| section.name == "definitions")
        .expect("definitions section");
    for stmt in &defs.suite.statements {
        if let StmtKind::Assign { value, .. } = &stmt.kind {
            return value.clone();
        }
    }
    panic!("no assignment found in definitions: {defs:?}");
}

#[test]
fn match_expression_parses_to_cases_desugar() {
    // Headline shape: `match x { 0 => 1.0, _ => 0.0 }` parses as
    // `Cases { subject: Some(x), arms: [(x == 0, 1.0)], else: 0.0 }`.
    let expr = defn_expr(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { 0 => 1.0, _ => 0.0 }\n",
    );
    let ExprKind::Cases {
        subject,
        arms,
        else_arm,
    } = &expr.kind
    else {
        panic!("expected Cases desugar, got {:?}", expr.kind);
    };
    let Some(subject) = subject else {
        panic!("match must carry its subject into the cases desugar");
    };
    assert!(
        matches!(
            &subject.kind,
            ExprKind::Path { segments, generics: None }
                if segments == &vec!["x".to_string()]
        ),
        "subject was {subject:?}"
    );
    assert_eq!(arms.len(), 1, "one literal arm");
    let (condition, value) = &arms[0];
    assert!(
        matches!(
            &condition.kind,
            ExprKind::Binary { op: BinaryOp::Eq, left, right }
                if matches!(&left.kind, ExprKind::Path { segments, generics: None } if segments == &vec!["x".to_string()])
                    && matches!(&right.kind, ExprKind::Int(text) if text == "0")
        ),
        "condition was {condition:?}"
    );
    assert!(matches!(&value.kind, ExprKind::Float(text) if text == "1.0"));
    assert!(matches!(&else_arm.kind, ExprKind::Float(text) if text == "0.0"));

    // String literal patterns are first-class (the own example).
    let expr = defn_expr(
        "emath function g:\n    inputs:\n        x: Float64\n\n    definitions:\n        g = match x { 0 => \"zero\", _ => \"nonzero\" }\n",
    );
    let ExprKind::Cases { arms, else_arm, .. } = &expr.kind else {
        panic!("expected Cases desugar, got {:?}", expr.kind);
    };
    assert!(matches!(&arms[0].1.kind, ExprKind::Str(text) if text == "zero"));
    assert!(matches!(&else_arm.kind, ExprKind::Str(text) if text == "nonzero"));
}

#[test]
fn binding_arm_substitutes_subject() {
    // A binding pattern (`other =>`) is the catch-all with a name: the
    // arm value must have the subject substituted for the name, or the
    // bound name would resolve to nothing at admission.
    let expr = defn_expr(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { 1.0 => 0.0, other => other * 2.0 }\n",
    );
    let ExprKind::Cases { else_arm, .. } = &expr.kind else {
        panic!("expected Cases desugar, got {:?}", expr.kind);
    };
    assert!(
        matches!(
            &else_arm.kind,
            ExprKind::Binary { op: BinaryOp::Mul, left, right }
                if matches!(&left.kind, ExprKind::Path { segments, generics: None } if segments == &vec!["x".to_string()])
                    && matches!(&right.kind, ExprKind::Float(text) if text == "2.0")
        ),
        "else arm was {else_arm:?} (binding name must be replaced by the subject)"
    );
}

#[test]
fn match_admits_in_definition_position() {
    // `y = match ...` in `definitions:` admits end to end (the desugared
    // cases lower through the existing nested-conditional path).
    let checked = check_source(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        y = match x { 0.0 => 1.0, _ => 0.0 }\n",
    );
    assert!(
        !checked.diagnostics.has_errors(),
        "match expression must admit in definition position, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
}

#[test]
fn missing_catch_all_refuses() {
    // Totality: a match whose last arm is a literal has uncovered
    // subject values — refuse at parse time (E-SYN-110), never a silent
    // fallthrough.
    let (tree, diags) = emath_syntax::parse_str(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { 0.0 => 1.0 }\n",
    );
    let _ = tree;
    assert!(
        diags
            .errors()
            .any(|error| error.code == "E-SYN-110" && error.message.contains("catch-all")),
        "missing catch-all must refuse E-SYN-110 naming the catch-all, got {diags:?}"
    );
}

#[test]
fn empty_match_refuses() {
    let (tree, diags) = emath_syntax::parse_str(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { }\n",
    );
    let _ = tree;
    assert!(
        diags
            .errors()
            .any(|error| error.code == "E-SYN-110" && error.message.contains("catch-all")),
        "empty match must refuse E-SYN-110, got {diags:?}"
    );
}

#[test]
fn catch_all_must_be_last() {
    // First-match-wins: a catch-all before the end makes every later
    // arm unreachable, so it refuses instead of silently shadowing.
    let (tree, diags) = emath_syntax::parse_str(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { _ => 0.0, 1.0 => 2.0 }\n",
    );
    let _ = tree;
    assert!(
        diags.errors().any(
            |error| error.code == "E-SYN-101" && error.message.contains("must be the last arm")
        ),
        "mid-match catch-all must refuse E-SYN-101, got {diags:?}"
    );
}

#[test]
fn negative_literal_pattern_becomes_unary_neg() {
    // `-1.0` in pattern position is a negative literal (Unary::Neg over
    // the literal), not a malformed pattern.
    let expr = defn_expr(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { -1.0 => 0.0, _ => 1.0 }\n",
    );
    let ExprKind::Cases { arms, .. } = &expr.kind else {
        panic!("expected Cases desugar, got {:?}", expr.kind);
    };
    assert!(
        matches!(
            &arms[0].0.kind,
            ExprKind::Binary { op: BinaryOp::Eq, right, .. }
                if matches!(&right.kind, ExprKind::Unary { op: emath_core::tree::UnaryOp::Neg, value }
                    if matches!(&value.kind, ExprKind::Float(text) if text == "1.0"))
        ),
        "condition was {:?}",
        arms[0].0
    );
}

#[test]
fn nested_match_in_arm_value_desugars() {
    // An arm value may itself be a match; the inner one desugars to its
    // own Cases before the outer arm is built.
    let expr = defn_expr(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { 0.0 => match x { 0.0 => 1.0, _ => 2.0 }, _ => 3.0 }\n",
    );
    let ExprKind::Cases { arms, .. } = &expr.kind else {
        panic!("expected Cases desugar, got {:?}", expr.kind);
    };
    assert!(
        matches!(&arms[0].1.kind, ExprKind::Cases { .. }),
        "nested match must desugar to a nested Cases, got {:?}",
        arms[0].1
    );
}

#[test]
fn example_model_executes() {
    // E2E: both wildcard and binding catch-alls compute.
    let source = include_str!("../../../language/examples/intro/match-expressions.emath");
    let checked = check_source(source);
    assert!(
        !checked.diagnostics.has_errors(),
        "example must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let report = run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    assert_eq!(values.get("gate"), Some(&Value::F64(1.0)));
    assert_eq!(values.get("scaled"), Some(&Value::F64(6.0)));
}
