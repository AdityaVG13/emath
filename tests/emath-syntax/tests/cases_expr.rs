//! U1: cases expression parse, format, and contextual keyword tests.

use emath_core::FileId;
use emath_core::limits::Limits;
use emath_core::tree::{BinaryOp, ExprKind, Item, StmtKind};
use emath_syntax::formatter::format;
use emath_syntax::{parse_lossless, parse_str};

fn def_expr<'a>(
    tree: &'a emath_core::tree::SyntaxTree,
    name: &str,
) -> Option<&'a emath_core::tree::Expr> {
    let item = tree.items.first()?;
    let Item::Declaration(decl) = item else {
        return None;
    };
    for section in decl.sections() {
        if section.name == "definitions" {
            for stmt in &section.suite.statements {
                match &stmt.kind {
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
fn cases_expr() {
    boot();
    let mut probe = Probe::new("U1: cases expression parse, format, and contextual keyword tests.");
    probe.case("cases_with_subject_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases x:
            | x > 0 => 1
            | x < 0 => -1
            | else => 0
";
    let (tree, diags) = parse_str(source);
    p.demand("1",diags.errors().next().is_none(), format!(
        "cases with subject should parse, got errors: {:?}",
        diags.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "f").expect("definition `f` not found");
    match &expr.kind {
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            p.demand("2",subject.is_some(), format!( "subject should be present"));
            if p.failures().len() != f0 { return; }
            p.eq("3", arms.len(), 2);
            p.demand("4",matches!(
                &arms[0].0.kind,
                ExprKind::Binary {
                    op: BinaryOp::Gt,
                    ..
                }
            ), stringify!(matches!(
                &arms[0].0.kind,
                ExprKind::Binary {
                    op: BinaryOp::Gt,
                    ..
                }
            )));
            if p.failures().len() != f0 { return; }
            p.demand("5",matches!(&else_arm.kind, ExprKind::Int(_)), stringify!(matches!(&else_arm.kind, ExprKind::Int(_))));
            if p.failures().len() != f0 { return; }
        }
        other => panic!("expected Cases, got {other:?}"),
    }

    });
    probe.case("cases_without_subject_parses", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases:
            | x > 0 => 1
            | else => 0
";
    let (tree, diags) = parse_str(source);
    p.demand("1",diags.errors().next().is_none(), format!(
        "cases without subject should parse, got errors: {:?}",
        diags.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "f").expect("definition `f` not found");
    match &expr.kind {
        ExprKind::Cases { subject, arms, .. } => {
            p.demand("2",subject.is_none(), format!( "subject should be absent"));
            if p.failures().len() != f0 { return; }
            p.eq("3", arms.len(), 1);
        }
        other => panic!("expected Cases, got {other:?}"),
    }

    });
    probe.case("cases_missing_else_is_parse_error", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases x:
            | x > 0 => 1
            | x < 0 => -1
";
    let (_, diags) = parse_str(source);
    let errors: Vec<_> = diags.errors().collect();
    p.demand("1",errors
            .iter()
            .any(|d| d.code == "E-SYN-110" && d.message.contains("else")), format!(
        "missing else must be E-SYN-110 naming the else arm, got {errors:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("cases_empty_body_expects_arm_pipe", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases x:
";
    let (_, diags) = parse_str(source);
    let errors: Vec<_> = diags.errors().collect();
    p.demand("1",errors
            .iter()
            .any(|d| d.code == "E-SYN-110" && d.message.contains("expected `|`")), format!(
        "empty cases body must be E-SYN-110 expecting an arm, got {errors:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("cases_contextual_keyword_remains_valid_identifier", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases + 1
";
    let (tree, diags) = parse_str(source);
    p.demand("1",diags.errors().next().is_none(), format!(
        "`cases` as identifier should parse, got errors: {:?}",
        diags.errors().collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&tree, "f").expect("definition `f` not found");
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
    probe.case("cases_formatter_roundtrips", |p| {
    let f0 = p.failures().len();

    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases x:
            | x > 0 => 1
            | x < 0 => -1
            | else => 0
";
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    p.demand("1",!parsed.diagnostics.has_errors(), format!(
        "source must parse cleanly"
    ));
    if p.failures().len() != f0 { return; }
    let formatted = format(&parsed.tree, &parsed.comments);
    let reparsed = parse_lossless(&formatted, FileId(0), &Limits::default());
    p.demand("2",!reparsed.diagnostics.has_errors(), format!(
        "formatter output should reparse"
    ));
    if p.failures().len() != f0 { return; }
    let expr = def_expr(&reparsed.tree, "f").expect("definition `f` not found in reparsed");
    p.demand("3",matches!(&expr.kind, ExprKind::Cases { .. }), format!(
        "reparsed should have Cases expression, got {:?}",
        expr.kind
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
