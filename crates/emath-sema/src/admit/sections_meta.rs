//! Meta section admission: about, evidence, host bindings, text helpers,
//! and the top-level `check_tree` entry point, extracted from `sections.rs`
//! isomorphically.

use emath_core::tree::{CommandArgument, Expr, ExprKind, Section, StmtKind, SyntaxTree};
use emath_core::Diagnostics;
use emath_ir::evidence::{ClaimVerdict, EvidenceClaim};
use emath_ir::goal::EvidenceLevel;
use emath_ir::{HostBinding, HostMethod, ImportEntry, ImportSelection};
use std::collections::{BTreeMap, BTreeSet};

use super::types::type_display;
use super::{
    admit_declaration, Admitter, CheckResult, SemanticTrace, confusable_fold,
};

pub(super) fn host_imported_types(imports: &[ImportEntry]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for import in imports {
        if import.path.first().map(String::as_str) != Some("host") {
            continue;
        }
        if let ImportSelection::Named(pairs) = &import.selection {
            for (name, alias) in pairs {
                names.insert(alias.clone().unwrap_or_else(|| name.clone()));
            }
        }
    }
    names
}

pub(super) fn admit_about(admitter: &mut Admitter, section: Option<&Section>) -> Option<String> {
    let section = section?;
    let mut summary = None;
    for stmt in &section.suite.statements {
        match &stmt.kind {
            StmtKind::Command { head, argument } if head.first().map(String::as_str) == Some("summary") => {
                if let Some(CommandArgument::Expr(expr)) = argument {
                    if let ExprKind::Str(text) = &expr.kind {
                        summary = Some(text.clone());
                        admitter.record("sema", "about summary retained", expr.source);
                        continue;
                    }
                }
                admitter.error(
                    "E-SYN-101",
                    "`about.summary` must be a string literal",
                    stmt.source,
                );
            }
            _ => {
                admitter.error(
                    "E-SYN-101",
                    "`about:` admits `summary: \"...\"` in Phase 1",
                    stmt.source,
                );
            }
        }
    }
    summary
}

pub(super) fn admit_evidence(admitter: &mut Admitter, section: Option<&Section>) -> Vec<EvidenceClaim> {
    let Some(section) = section else {
        return Vec::new();
    };
    let mut claims = Vec::new();
    for stmt in &section.suite.statements {
        let StmtKind::Section(claim) = &stmt.kind else {
            admitter.error(
                "E-SYN-101",
                "expected `claim <name>:` blocks inside `evidence:`",
                stmt.source,
            );
            continue;
        };
        if claim.name != "claim" {
            admitter.error(
                "E-SYN-101",
                format!("unknown evidence block `{}`", claim.name),
                claim.head_source,
            );
            continue;
        }
        let id = claim.generic.clone().unwrap_or_default();
        if id.is_empty() {
            admitter.error(
                "E-SYN-101",
                "`claim` requires a name in `<...>`",
                claim.head_source,
            );
            continue;
        }
        let mut statement = String::new();
        let mut class = String::new();
        for inner in &claim.suite.statements {
            match &inner.kind {
                StmtKind::Command { head, argument } if head.first().map(String::as_str) == Some("statement") => {
                    statement = match argument {
                        Some(CommandArgument::Expr(expr)) => expr_text(expr),
                        _ if head.len() > 1 => head[1..].join(" "),
                        _ => String::new(),
                    };
                }
                StmtKind::Require(expr) => {
                    class = expr_text(expr);
                }
                StmtKind::Command { head, .. } if head.first().map(String::as_str) == Some("require") => {
                    class = head.get(1).cloned().unwrap_or_default();
                }
                _ => {
                    admitter.error(
                        "E-SYN-101",
                        "evidence claims admit `statement ...` and `require ...`",
                        inner.source,
                    );
                }
            }
        }
        admitter.record(
            "sema",
            format!("evidence claim `{id}` recorded (verdict not-run)"),
            claim.head_source,
        );
        claims.push(EvidenceClaim {
            id,
            statement,
            class,
            scope: "declaration".into(),
            assumptions: Vec::new(),
            producer: "source".into(),
            checker: None,
            verdict: ClaimVerdict::NotRun,
            level: EvidenceLevel::E1,
            falsifiers: Vec::new(),
            artifacts: Vec::new(),
            fresh_until: None,
        });
    }
    claims
}

pub(super) fn admit_host(admitter: &mut Admitter, section: Option<&Section>) -> Vec<HostBinding> {
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
                    name, params, ret, suite, ..
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
                            let ty = if param.by_ref {
                                format!("&{ty}")
                            } else {
                                ty
                            };
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

/// Parse the whole file and admit every declaration (used by the session).
pub fn check_tree(tree: &SyntaxTree) -> CheckResult {
    let mut diagnostics = Diagnostics::new();
    let mut trace = SemanticTrace::default();
    let mut package = emath_ir::SemanticPackage::new();

    // Front-end: package identity and `use` imports. External file
    // imports remain a Phase 2 refusal (E-PKG-050).
    let has_recognition_items = tree.items.iter().any(|item| match item {
        emath_core::tree::Item::Package { .. } | emath_core::tree::Item::Use { .. } => true,
        emath_core::tree::Item::Declaration(decl) => decl.item_kind != "custom",
        emath_core::tree::Item::Notation(_) => true,
    });
    let recognition = if has_recognition_items {
        let front_end = crate::recognition::admit_front_end(tree, &mut diagnostics, &mut trace);
        package.package_path = front_end.package_path;
        package.imports = front_end.imports;
        Some(crate::recognition::collect_kind_defs(tree))
    } else {
        None
    };
    let host_types = host_imported_types(&package.imports);

    let mut declaration_id = 0_u32;
    let mut seen_declaration_names: BTreeSet<String> = BTreeSet::new();
    let mut seen_folded_declaration_names: BTreeMap<String, String> = BTreeMap::new();
    for item in &tree.items {
        let emath_core::tree::Item::Declaration(decl) = item else {
            continue;
        };
        if let Some(kind_defs) = &recognition {
            if decl.item_kind != "custom" {
                crate::recognition::admit_declaration(
                    decl,
                    kind_defs,
                    &mut package,
                    &mut diagnostics,
                    &mut trace,
                );
                continue;
            }
        }
        // Duplicate declaration names are a typed refusal (E-NAME-022):
        // two `custom <Foo>` declarations would collide in generated
        // Rust, so the second is never admitted.
        if !seen_declaration_names.insert(decl.name.clone()) {
            diagnostics.error(
                "E-NAME-022",
                format!("duplicate declaration name `{}`", decl.name),
                decl.head_source,
            );
            continue;
        }
        // `_` is not a valid Rust type name and cannot be escaped; a
        // declaration named `_` is refused up front (E-NAME-023).
        if decl.name == "_" {
            diagnostics.error(
                "E-NAME-023",
                "declaration name `_` is reserved and cannot be a Rust type",
                decl.head_source,
            );
            continue;
        }
        // Confusable identity (spec `01_LEXICAL_LAYOUT_AND_SOURCE`): a
        // declaration name that differs from an already-seen one only by
        // lookalike glyphs (Latin `o` vs Cyrillic `о`) is refused
        // (E-NAME-024) — the public API would present two visually
        // indistinguishable names.
        let folded = confusable_fold(&decl.name);
        if let Some(existing) = seen_folded_declaration_names.get(&folded) {
            diagnostics.error(
                "E-NAME-024",
                format!(
                    "declaration name `{}` is confusable with `{existing}` and is refused",
                    decl.name
                ),
                decl.head_source,
            );
            continue;
        }
        seen_folded_declaration_names.insert(folded, decl.name.clone());
        if decl.item_kind != "custom" {
            diagnostics.error(
                "E-KIND-001",
                format!(
                    "declaration kind `{}` is not supported; Phase 1 uses `emath custom`",
                    decl.item_kind
                ),
                decl.head_source,
            );
            continue;
        }
        if decl.as_kind != "function" && decl.as_kind != "policy" && decl.as_kind != "model" {
            diagnostics.error(
                "E-KIND-100",
                format!(
                    "declaration type `{}` is outside the Phase 1 subset (function, policy, model)",
                    decl.as_kind
                ),
                decl.head_source,
            );
            continue;
        }
        let (declaration, tests, types, exprs, entries, admit_diagnostics, residuals) =
            admit_declaration(decl, &host_types);
        diagnostics.extend_from(&admit_diagnostics);
        trace.entries.extend(entries);
        let Some(mut declaration) = declaration else {
            diagnostics.error(
                "E-KIND-002",
                "declaration could not be admitted",
                decl.head_source,
            );
            continue;
        };
        declaration.id = emath_ir::DeclarationId(declaration_id);
        declaration_id += 1;
        if !residuals.is_empty() {
            package.residuals.insert(declaration.id, residuals);
        }
        package.types.extend(types);
        package.exprs.extend(exprs.iter().map(|(e, _)| e.clone()));
        package.expr_spans.extend(exprs.iter().map(|(_, s)| *s));
        for test in tests {
            declaration.tests.push(package.push_test(test));
        }
        package.declarations.push(declaration);
    }
    if !package.declarations.is_empty() {
        package.seal();
    }
    CheckResult {
        package,
        diagnostics,
        trace,
    }
}
