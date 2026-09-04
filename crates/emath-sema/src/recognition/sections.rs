//! Section and statement shape admission.

use emath_core::Diagnostics;
use emath_core::tree::{Declaration, Stmt, StmtKind, TypeKind};

use crate::admit::SemanticTrace;

use super::require_head;
use super::schema::{SectionRule, ShapeRule, StmtShapeKind};
use super::text::{argument_text, expr_text, place_text, type_text};

pub(super) fn format_section_trace(section: &emath_core::tree::Section) -> String {
    match &section.generic {
        Some(generic) => format!("{} {generic}", section.name),
        None => section.name.clone(),
    }
}

pub(super) fn body_command_allowed(decl: &Declaration, head: &[String]) -> bool {
    let Some(first) = head.first() else {
        return false;
    };
    match decl.item_kind.as_str() {
        // `extends policy` on kind definitions; `representation <type>` on
        // type aliases.
        "kind" => first == "extends",
        "type" => first == "representation",
        // World applications: `output: "Portfolio"` names the
        // interpretation portfolio at body level.
        "custom" => decl.as_kind == "world" && first == "output",
        _ => false,
    }
}

pub(super) fn generic_allowed(rule: &SectionRule, generic: Option<&str>) -> bool {
    match (rule.generics, generic) {
        (None, _) | (Some(_), None) => true,
        (Some(allowed), Some(generic)) => allowed.contains(&generic),
    }
}

pub(super) fn admit_section(
    section: &emath_core::tree::Section,
    rule: &SectionRule,
    decl: &Declaration,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    for stmt in &section.suite.statements {
        if let StmtKind::Section(nested) = &stmt.kind {
            // A nested rule with name "" accepts any nested section name
            // (used by `lower: <dotted>` blocks).
            let Some(nested_rule) = rule
                .nested
                .iter()
                .find(|nested_rule| nested_rule.name.is_empty() || nested_rule.name == nested.name)
            else {
                diagnostics.error(
                    "E-SYN-101",
                    format!(
                        "nested section `{}` is not admitted under `{}`",
                        nested.name, section.name
                    ),
                    nested.head_source,
                );
                continue;
            };
            for inner in &nested.suite.statements {
                admit_stmt(inner, nested_rule, decl, diagnostics, trace, None);
            }
            continue;
        }
        if let StmtKind::FnDecl { head, name, .. } = &stmt.kind {
            if rule.fn_heads.contains(&head.as_str()) {
                trace.record(
                    "recognize:fn",
                    format!("{} `{name}` under `{}`", head, section.name),
                    Some(stmt.source),
                );
                continue;
            }
        }
        admit_stmt(stmt, rule, decl, diagnostics, trace, Some(section));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn admit_stmt<R: ShapeRule>(
    stmt: &Stmt,
    rule: &R,
    decl: &Declaration,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
    section: Option<&emath_core::tree::Section>,
) {
    if !rule
        .statement_shapes()
        .iter()
        .any(|shape| shape_accepts(*shape, stmt))
    {
        diagnostics.error(
            "E-SYN-101",
            format!(
                "statement shape is not admitted in section `{}` of kind `{}`",
                section.map_or("body", |s| s.name.as_str()),
                decl.item_kind
            ),
            stmt.source,
        );
        return;
    }
    match &stmt.kind {
        StmtKind::FieldDecl { name, ty, .. } => {
            let detail = match &ty.kind {
                TypeKind::Path {
                    segments,
                    generic_args,
                } if generic_args.is_empty()
                    && segments.last().map(String::as_str) == Some("Infer") =>
                {
                    name.clone()
                }
                _ => format!("{}: {}", name, type_text(&ty)),
            };
            trace.record("recognize:field", detail, Some(stmt.source));
        }
        StmtKind::Assign { target, value } => {
            trace.record(
                "recognize:define",
                format!("{} = {}", place_text(&target), expr_text(&value)),
                Some(stmt.source),
            );
        }
        StmtKind::Equation { left, right } => {
            trace.record(
                "recognize:equation",
                format!("{} = {}", expr_text(&left), expr_text(&right)),
                Some(stmt.source),
            );
        }
        StmtKind::Require(expr) => {
            if let Some(section) = section {
                if section.name == "schema" {
                    if let Some(rule) = require_head(&expr) {
                        trace.record(
                            "recognize:schema-rule",
                            format!("{rule:?}"),
                            Some(stmt.source),
                        );
                        return;
                    }
                }
            }
            trace.record("recognize:require", expr_text(&expr), Some(stmt.source));
        }
        StmtKind::Expr(expr) => {
            trace.record("recognize:expression", expr_text(&expr), Some(stmt.source));
        }
        StmtKind::Invariant(expr) => {
            trace.record("recognize:invariant", expr_text(&expr), Some(stmt.source));
        }
        StmtKind::Command { head, argument } => {
            let first = head.first().map_or("", String::as_str);
            let allowed = rule.command_first_words().is_empty()
                || rule.command_first_words().contains(&first);
            if !allowed {
                diagnostics.error(
                    "E-SYN-101",
                    format!(
                        "command `{}` is not admitted in section `{}` of kind `{}`",
                        head.join(" "),
                        section.map_or("body", |s| s.name.as_str()),
                        decl.item_kind
                    ),
                    stmt.source,
                );
                return;
            }
            let mut text = head.join(" ");
            if let Some(argument) = argument {
                text.push(' ');
                text.push_str(&argument_text(argument));
            }
            trace.record(
                "recognize:command",
                format!("{} ({})", text, section.map_or("body", |s| s.name.as_str())),
                Some(stmt.source),
            );
        }
        StmtKind::SelfBlock { .. } => {
            trace.record("recognize:self", "Self { ... }", Some(stmt.source));
        }
        StmtKind::Let { name, ty, value } => {
            let ty_text = ty.as_ref().map_or_else(String::new, type_text);
            trace.record(
                "recognize:let",
                format!("{}: {} = {}", name, ty_text, expr_text(&value)),
                Some(stmt.source),
            );
        }
        other => {
            diagnostics.error(
                "E-SYN-101",
                format!(
                    "statement shape is not admitted in section `{}` of kind `{}`",
                    section.map_or("body", |s| s.name.as_str()),
                    decl.item_kind
                ),
                stmt.source,
            );
            let _ = other;
        }
    }
}

pub(super) fn shape_accepts(shape: StmtShapeKind, stmt: &Stmt) -> bool {
    match shape {
        StmtShapeKind::Fields => matches!(stmt.kind, StmtKind::FieldDecl { .. }),
        StmtShapeKind::Assigns => matches!(stmt.kind, StmtKind::Assign { .. }),
        StmtShapeKind::Equations => matches!(stmt.kind, StmtKind::Equation { .. }),
        StmtShapeKind::Exprs => matches!(stmt.kind, StmtKind::Expr(_)),
        StmtShapeKind::Requires => matches!(stmt.kind, StmtKind::Require(_)),
        StmtShapeKind::CommandsAny => matches!(stmt.kind, StmtKind::Command { .. }),
    }
}

pub(super) fn admit_fn_head(
    head: &str,
    name: &str,
    suite: Option<&emath_core::tree::Suite>,
    decl: &Declaration,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    if let Some(suite) = suite {
        for stmt in &suite.statements {
            admit_fn_statement(head, name, stmt, decl, diagnostics, trace);
        }
    }
}

pub(super) fn admit_fn_statement(
    head: &str,
    fn_name: &str,
    stmt: &Stmt,
    _decl: &Declaration,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    match &stmt.kind {
        StmtKind::Require(expr) => {
            trace.record(
                "recognize:constructor-require",
                format!("{head} {fn_name}: require {}", expr_text(&expr)),
                Some(stmt.source),
            );
        }
        StmtKind::SelfBlock { assignments } => {
            let assignments: Vec<String> = assignments
                .iter()
                .map(|(name, value)| format!("{name} = {}", expr_text(&value)))
                .collect();
            trace.record(
                "recognize:constructor-self",
                format!("{head} {fn_name}: Self {{ {} }}", assignments.join("; ")),
                Some(stmt.source),
            );
        }
        StmtKind::Expr(expr) => {
            trace.record(
                "recognize:fn-expression",
                format!("{head} {fn_name}: {}", expr_text(&expr)),
                Some(stmt.source),
            );
        }
        StmtKind::Assign { target, value } => {
            trace.record(
                "recognize:fn-assign",
                format!(
                    "{head} {fn_name}: {} = {}",
                    place_text(&target),
                    expr_text(&value)
                ),
                Some(stmt.source),
            );
        }
        StmtKind::Command { .. } if head == "constructor" => {
            trace.record(
                "recognize:constructor-command",
                format!("{head} {fn_name}"),
                Some(stmt.source),
            );
        }
        other => {
            diagnostics.error(
                "E-SYN-101",
                format!("statement is not admitted inside `{head} {fn_name}`"),
                stmt.source,
            );
            let _ = other;
        }
    }
}
