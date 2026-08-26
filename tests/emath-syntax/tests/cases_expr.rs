//! U1: cases expression parse, format, and contextual keyword tests.

use emath_core::tree::{BinaryOp, ExprKind, Item, StmtKind};
use emath_core::FileId;
use emath_core::limits::Limits;
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
                        return Some(value)
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

#[test]
fn cases_with_subject_parses() {
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases x:
            | x > 0 => 1
            | x < 0 => -1
            | else => 0
";
    let (tree, diags) = parse_str(source);
    assert!(
        diags.errors().next().is_none(),
        "cases with subject should parse, got errors: {:?}",
        diags.errors().collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "f").expect("definition `f` not found");
    match &expr.kind {
        ExprKind::Cases { subject, arms, else_arm } => {
            assert!(subject.is_some(), "subject should be present");
            assert_eq!(arms.len(), 2, "should have 2 condition arms");
            assert!(matches!(&arms[0].0.kind, ExprKind::Binary { op: BinaryOp::Gt, .. }));
            assert!(matches!(&else_arm.kind, ExprKind::Int(_)));
        }
        other => panic!("expected Cases, got {other:?}"),
    }
}

#[test]
fn cases_without_subject_parses() {
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases:
            | x > 0 => 1
            | else => 0
";
    let (tree, diags) = parse_str(source);
    assert!(
        diags.errors().next().is_none(),
        "cases without subject should parse, got errors: {:?}",
        diags.errors().collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "f").expect("definition `f` not found");
    match &expr.kind {
        ExprKind::Cases { subject, arms, .. } => {
            assert!(subject.is_none(), "subject should be absent");
            assert_eq!(arms.len(), 1, "should have 1 condition arm");
        }
        other => panic!("expected Cases, got {other:?}"),
    }
}

#[test]
fn cases_missing_else_is_parse_error() {
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases x:
            | x > 0 => 1
            | x < 0 => -1
";
    let (_, diags) = parse_str(source);
    let errors: Vec<_> = diags.errors().collect();
    assert!(
        errors.iter().any(|d| d.code == "E-SYN-110"
            && d.message.contains("else")),
        "missing else must be E-SYN-110 naming the else arm, got {errors:?}"
    );
}

#[test]
fn cases_empty_body_expects_arm_pipe() {
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases x:
";
    let (_, diags) = parse_str(source);
    let errors: Vec<_> = diags.errors().collect();
    assert!(
        errors.iter().any(|d| d.code == "E-SYN-110"
            && d.message.contains("expected `|`")),
        "empty cases body must be E-SYN-110 expecting an arm, got {errors:?}"
    );
}

#[test]
fn cases_contextual_keyword_remains_valid_identifier() {
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases + 1
";
    let (tree, diags) = parse_str(source);
    assert!(
        diags.errors().next().is_none(),
        "`cases` as identifier should parse, got errors: {:?}",
        diags.errors().collect::<Vec<_>>()
    );
    let expr = def_expr(&tree, "f").expect("definition `f` not found");
    assert!(
        matches!(&expr.kind, ExprKind::Binary { op: BinaryOp::Add, .. }),
        "expected addition, got {:?}",
        expr.kind
    );
}

#[test]
fn cases_formatter_roundtrips() {
    let source = "\
emath function f(x: Float64) -> Float64:
    definitions:
        f = cases x:
            | x > 0 => 1
            | x < 0 => -1
            | else => 0
";
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    assert!(!parsed.diagnostics.has_errors(), "source must parse cleanly");
    let formatted = format(&parsed.tree, &parsed.comments);
    let reparsed = parse_lossless(&formatted, FileId(0), &Limits::default());
    assert!(
        !reparsed.diagnostics.has_errors(),
        "formatter output should reparse"
    );
    let expr = def_expr(&reparsed.tree, "f").expect("definition `f` not found in reparsed");
    assert!(
        matches!(&expr.kind, ExprKind::Cases { .. }),
        "reparsed should have Cases expression, got {:?}",
        expr.kind
    );
}
