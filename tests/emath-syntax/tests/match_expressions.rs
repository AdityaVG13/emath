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

fn check_source(source: &str) -> emath_sema::admit::CheckResult {
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned("match-expressions", source)
}

/// Parse a one-definition function and return the definition's value
/// expression (`f = <expr>` in `definitions:`).
fn defn_expr(p: &mut Probe, source: &str) -> Expr {
    let (tree, diags) = emath_syntax::parse_str(source);
    p.demand("parse", !diags.has_errors(), format!("{diags:?}"));
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

use emath_test_harness::{Probe, boot};

#[test]
fn match_expressions() {
    boot();
    let mut probe = Probe::new("U6 match expressions. `match subject { pattern => value, ... }` is expression-position sugar for `cases` (U1): a literal pattern (Int/Float/Str/Bool,");
    probe.case("match_expression_parses_to_cases_desugar", |p| {
    let f0 = p.failures().len();

    // Headline shape: `match x { 0 => 1.0, _ => 0.0 }` parses as
    // `Cases { subject: Some(x), arms: [(x == 0, 1.0)], else: 0.0 }`.
    let expr = defn_expr(p, 
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
    p.demand("1",matches!(
            &subject.kind,
            ExprKind::Path { segments, generics: None }
                if segments == &vec!["x".to_string()]
        ), format!(
        "subject was {subject:?}"
    ));
    if p.failures().len() != f0 { return; }
    p.eq("2", arms.len(), 1);
    let (condition, value) = &arms[0];
    p.demand("3",matches!(
            &condition.kind,
            ExprKind::Binary { op: BinaryOp::Eq, left, right }
                if matches!(&left.kind, ExprKind::Path { segments, generics: None } if segments == &vec!["x".to_string()])
                    && matches!(&right.kind, ExprKind::Int(text) if text == "0")
        ), format!(
        "condition was {condition:?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("4",matches!(&value.kind, ExprKind::Float(text) if text == "1.0"), stringify!(matches!(&value.kind, ExprKind::Float(text) if text == "1.0")));
    if p.failures().len() != f0 { return; }
    p.demand("5",matches!(&else_arm.kind, ExprKind::Float(text) if text == "0.0"), stringify!(matches!(&else_arm.kind, ExprKind::Float(text) if text == "0.0")));
    if p.failures().len() != f0 { return; }

    // String literal patterns are first-class (the own example).
    let expr = defn_expr(p, 
        "emath function g:\n    inputs:\n        x: Float64\n\n    definitions:\n        g = match x { 0 => \"zero\", _ => \"nonzero\" }\n",
    );
    let ExprKind::Cases { arms, else_arm, .. } = &expr.kind else {
        panic!("expected Cases desugar, got {:?}", expr.kind);
    };
    p.demand("6",matches!(&arms[0].1.kind, ExprKind::Str(text) if text == "zero"), stringify!(matches!(&arms[0].1.kind, ExprKind::Str(text) if text == "zero")));
    if p.failures().len() != f0 { return; }
    p.demand("7",matches!(&else_arm.kind, ExprKind::Str(text) if text == "nonzero"), stringify!(matches!(&else_arm.kind, ExprKind::Str(text) if text == "nonzero")));
    if p.failures().len() != f0 { return; }

    });
    probe.case("binding_arm_substitutes_subject", |p| {
    let f0 = p.failures().len();

    // A binding pattern (`other =>`) is the catch-all with a name: the
    // arm value must have the subject substituted for the name, or the
    // bound name would resolve to nothing at admission.
    let expr = defn_expr(p, 
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { 1.0 => 0.0, other => other * 2.0 }\n",
    );
    let ExprKind::Cases { else_arm, .. } = &expr.kind else {
        panic!("expected Cases desugar, got {:?}", expr.kind);
    };
    p.demand("1",matches!(
            &else_arm.kind,
            ExprKind::Binary { op: BinaryOp::Mul, left, right }
                if matches!(&left.kind, ExprKind::Path { segments, generics: None } if segments == &vec!["x".to_string()])
                    && matches!(&right.kind, ExprKind::Float(text) if text == "2.0")
        ), format!(
        "else arm was {else_arm:?} (binding name must be replaced by the subject)"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("match_admits_in_definition_position", |p| {
    let f0 = p.failures().len();

    // `y = match ...` in `definitions:` admits end to end (the desugared
    // cases lower through the existing nested-conditional path).
    let checked = check_source(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        y = match x { 0.0 => 1.0, _ => 0.0 }\n",
    );
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "match expression must admit in definition position, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("missing_catch_all_refuses", |p| {
    let f0 = p.failures().len();

    // Totality: a match whose last arm is a literal has uncovered
    // subject values — refuse at parse time (E-SYN-110), never a silent
    // fallthrough.
    let (tree, diags) = emath_syntax::parse_str(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { 0.0 => 1.0 }\n",
    );
    let _ = tree;
    p.demand("1",diags
            .errors()
            .any(|error| error.code == "E-SYN-110" && error.message.contains("catch-all")), format!(
        "missing catch-all must refuse E-SYN-110 naming the catch-all, got {diags:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("empty_match_refuses", |p| {
    let f0 = p.failures().len();

    let (tree, diags) = emath_syntax::parse_str(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { }\n",
    );
    let _ = tree;
    p.demand("1",diags
            .errors()
            .any(|error| error.code == "E-SYN-110" && error.message.contains("catch-all")), format!(
        "empty match must refuse E-SYN-110, got {diags:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("catch_all_must_be_last", |p| {
    let f0 = p.failures().len();

    // First-match-wins: a catch-all before the end makes every later
    // arm unreachable, so it refuses instead of silently shadowing.
    let (tree, diags) = emath_syntax::parse_str(
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { _ => 0.0, 1.0 => 2.0 }\n",
    );
    let _ = tree;
    p.demand("1",diags.errors().any(
            |error| error.code == "E-SYN-101" && error.message.contains("must be the last arm")
        ), format!(
        "mid-match catch-all must refuse E-SYN-101, got {diags:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("negative_literal_pattern_becomes_unary_neg", |p| {
    let f0 = p.failures().len();

    // `-1.0` in pattern position is a negative literal (Unary::Neg over
    // the literal), not a malformed pattern.
    let expr = defn_expr(p, 
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { -1.0 => 0.0, _ => 1.0 }\n",
    );
    let ExprKind::Cases { arms, .. } = &expr.kind else {
        panic!("expected Cases desugar, got {:?}", expr.kind);
    };
    p.demand("1",matches!(
            &arms[0].0.kind,
            ExprKind::Binary { op: BinaryOp::Eq, right, .. }
                if matches!(&right.kind, ExprKind::Unary { op: emath_core::tree::UnaryOp::Neg, value }
                    if matches!(&value.kind, ExprKind::Float(text) if text == "1.0"))
        ), format!(
        "condition was {:?}",
        arms[0].0
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("nested_match_in_arm_value_desugars", |p| {
    let f0 = p.failures().len();

    // An arm value may itself be a match; the inner one desugars to its
    // own Cases before the outer arm is built.
    let expr = defn_expr(p, 
        "emath function f:\n    inputs:\n        x: Float64\n\n    definitions:\n        f = match x { 0.0 => match x { 0.0 => 1.0, _ => 2.0 }, _ => 3.0 }\n",
    );
    let ExprKind::Cases { arms, .. } = &expr.kind else {
        panic!("expected Cases desugar, got {:?}", expr.kind);
    };
    p.demand("1",matches!(&arms[0].1.kind, ExprKind::Cases { .. }), format!(
        "nested match must desugar to a nested Cases, got {:?}",
        arms[0].1
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("example_model_executes", |p| {
    let f0 = p.failures().len();

    // E2E: both wildcard and binding catch-alls compute.
    let source = include_str!("../../../tests/fixtures/language/intro/match-expressions.emath");
    let checked = check_source(source);
    p.demand("1",!checked.diagnostics.has_errors(), format!(
        "example must admit, got {:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let report = run_package(&checked.package);
    let values = &report.declarations[0].tests[0].definitions;
    p.eq("2", values.get("gate"), Some(&Value::F64(1.0)));
    p.eq("3", values.get("scaled"), Some(&Value::F64(6.0)));

    });
    probe.finish();
}
