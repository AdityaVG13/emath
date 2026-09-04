//! Generic host-binding admission.

use super::*;

pub(in crate::admit) fn admit_host(
    admitter: &mut Admitter,
    section: Option<&Section>,
) -> Vec<HostBinding> {
    let Some(section) = section else {
        return Vec::new();
    };
    let mut bindings = Vec::new();
    for stmt in &section.suite.statements {
        let StmtKind::Section(language) = &stmt.kind else {
            admitter.error(
                "E-SYN-101",
                "expected a language section (`rust:`) inside `host:`",
                stmt.source,
            );
            continue;
        };
        for inner in &language.suite.statements {
            let StmtKind::Section(implement) = &inner.kind else {
                admitter.error(
                    "E-SYN-101",
                    "expected `implement Trait for Type:` inside `host:`",
                    inner.source,
                );
                continue;
            };
            if implement.name != "implement" {
                admitter.error(
                    "E-SYN-101",
                    format!("unknown host block `{}`", implement.name),
                    implement.head_source,
                );
                continue;
            }
            let generic = implement.generic.clone().unwrap_or_default();
            let (trait_path, target) = match generic.rsplit_once("::") {
                Some((trait_path, target)) => (trait_path.to_string(), target.to_string()),
                None => (generic, String::new()),
            };
            let mut methods = Vec::new();
            for method_stmt in &implement.suite.statements {
                let StmtKind::FnDecl {
                    name,
                    params,
                    ret,
                    suite,
                    ..
                } = &method_stmt.kind
                else {
                    admitter.error(
                        "E-SYN-101",
                        "expected `method name(...)` inside `implement`",
                        method_stmt.source,
                    );
                    continue;
                };
                let mut body = Vec::new();
                if let Some(suite) = suite {
                    for body_stmt in &suite.statements {
                        body.push(stmt_text(body_stmt));
                    }
                }
                methods.push(HostMethod {
                    name: name.clone(),
                    params: params
                        .iter()
                        .map(|param| {
                            let ty = type_display(&param.ty);
                            let ty = if param.by_ref { format!("&{ty}") } else { ty };
                            (param.name.clone(), ty)
                        })
                        .collect(),
                    ret: ret.as_ref().map(type_display),
                    body,
                });
            }
            admitter.record(
                "sema",
                format!(
                    "host binding `{}/{}` retained (trait impl codegen is a Phase 1 no-claim)",
                    language.name, trait_path
                ),
                implement.head_source,
            );
            bindings.push(HostBinding {
                language: language.name.clone(),
                trait_path,
                target,
                methods,
            });
        }
    }
    bindings
}

pub(super) fn expr_text(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Path { segments, .. } => segments.join("."),
        ExprKind::Call { function, args } => {
            format!(
                "{}({})",
                expr_text(function),
                args.iter().map(expr_text).collect::<Vec<_>>().join(", ")
            )
        }
        ExprKind::Str(text) => format!("\"{text}\""),
        ExprKind::Int(text) | ExprKind::Float(text) => text.clone(),
        ExprKind::Bool(value) => value.to_string(),
        _ => "expr".to_string(),
    }
}

pub(super) fn stmt_text(stmt: &emath_core::tree::Stmt) -> String {
    match &stmt.kind {
        StmtKind::Command { head, argument } => {
            let mut text = head.join(" ");
            if let Some(argument) = argument {
                text.push(' ');
                text.push_str(&command_argument_text(argument));
            }
            text
        }
        _ => "stmt".to_string(),
    }
}

pub(super) fn command_argument_text(argument: &CommandArgument) -> String {
    match argument {
        CommandArgument::Expr(expr) => expr_text(expr),
        CommandArgument::Assignment { name, value } => {
            format!("{name} = {}", expr_text(value))
        }
        CommandArgument::List(items) => format!(
            "[{}]",
            items.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
    }
}
