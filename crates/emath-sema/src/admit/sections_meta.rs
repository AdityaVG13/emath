//! Meta section admission: about, evidence, host bindings, text helpers,
//! and the top-level `check_tree` entry point, extracted from `sections.rs`
//! isomorphically.

use emath_core::tree::{CommandArgument, Expr, ExprKind, Section, StmtKind, SyntaxTree};
use emath_core::Diagnostics;
use emath_ir::evidence::{ClaimVerdict, EvidenceClaim};
use emath_ir::goal::EvidenceLevel;
use emath_ir::{HostBinding, HostMethod, ImportEntry, ImportSelection, ModelResidual};
use emath_ir::ids::{ExprId, TypeId};
use emath_ir::{ExprNode, SliceAxis};
use std::collections::{BTreeMap, BTreeSet};

use super::types::type_display;
use super::{
    admit_declaration, Admitter, CheckResult, SemanticTrace, confusable_fold,
};

/// Offset all child ExprIds and TypeIds in one node from the admitter's
/// local arena into the package's global index space.
fn remap_expr_node(node: &mut ExprNode, expr_offset: u32, type_offset: u32) {
    let remap_e = |id: &mut ExprId| {
        id.0 += expr_offset;
    };
    let remap_t = |id: &mut TypeId| {
        id.0 += type_offset;
    };
    match node {
        ExprNode::Literal(_) | ExprNode::Variable(_) => {}
        ExprNode::Call { arguments, .. } => {
            for id in arguments {
                remap_e(id);
            }
        }
        ExprNode::Unary { value, .. } => remap_e(value),
        ExprNode::Binary { left, right, .. } => {
            remap_e(left);
            remap_e(right);
        }
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => {
            remap_e(condition);
            remap_e(then_value);
            remap_e(else_value);
        }
        ExprNode::Record { ty, fields } => {
            remap_t(ty);
            for (_, id) in fields {
                remap_e(id);
            }
        }
        ExprNode::Index { value, indices } => {
            remap_e(value);
            for id in indices {
                remap_e(id);
            }
        }
        ExprNode::Slice { value, axes } => {
            remap_e(value);
            for axis in axes.iter_mut() {
                match axis {
                    SliceAxis::Point(id) => remap_e(id),
                    SliceAxis::Range { start, end } => {
                        remap_e(start);
                        remap_e(end);
                    }
                }
            }
        }
        ExprNode::Binder { body, .. } => remap_e(body),
        ExprNode::Vector(ids) => {
            for id in ids {
                remap_e(id);
            }
        }
        ExprNode::Matrix(rows) => {
            for row in rows.iter_mut() {
                for id in row.iter_mut() {
                    remap_e(id);
                }
            }
        }
        ExprNode::Tensor { elements, .. } => {
            for id in elements {
                remap_e(id);
            }
        }
        ExprNode::Differentiate { body, .. } => remap_e(body),
        ExprNode::Solve { body, .. } => remap_e(body),
        ExprNode::Optimize { body, .. } => remap_e(body),
        ExprNode::SampleLimit { body, target, direction, .. } => {
            remap_e(target);
            remap_e(direction);
            remap_e(body);
        }
    }
}

/// Offset all ExprIds and TypeIds in a declaration and its test cases into
/// the package's global index space.
fn remap_ids(
    declaration: &mut emath_ir::Declaration,
    tests: &mut [emath_ir::constructor::TestCase],
    residuals: &mut [ModelResidual],
    expr_offset: u32,
    type_offset: u32,
) {
    let remap_expr = |id: &mut ExprId| {
        id.0 += expr_offset;
    };
    let remap_type = |id: &mut TypeId| {
        id.0 += type_offset;
    };

    // Definitions
    for (_, id) in &mut declaration.definitions {
        remap_expr(id);
    }
    // Invariants
    for id in &mut declaration.invariants {
        remap_expr(id);
    }
    // Inputs / outputs / state: Field ty
    for field in &mut declaration.inputs {
        remap_type(&mut field.ty);
    }
    for field in &mut declaration.outputs {
        remap_type(&mut field.ty);
    }
    for field in &mut declaration.state {
        remap_type(&mut field.ty);
    }
    // Constructors
    for ctor in &mut declaration.constructors {
        for id in &mut ctor.preconditions {
            remap_expr(id);
        }
        for (_, id) in &mut ctor.assignments {
            remap_expr(id);
        }
        for id in &mut ctor.postconditions {
            remap_expr(id);
        }
        for (_, id) in &mut ctor.defaults {
            remap_expr(id);
        }
        if let Some(id) = &mut ctor.error_type {
            remap_type(id);
        }
    }
    // Evidence claims: no ExprId fields, only string metadata
    // Constructors
    for test in tests.iter_mut() {
        for (_, id) in &mut test.given {
            remap_expr(id);
        }
        if let Some(id) = &mut test.expect {
            remap_expr(id);
        }
    }
    // Model residuals
    for residual in residuals.iter_mut() {
        remap_expr(&mut residual.expr);
    }
}

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

    // Item-attribute gates are file-scoped and run for every checked
    // tree, independent of the front-end lane: the Phase 1 compat lane
    // canonicalizes every kind to `custom`, so gating inside
    // `admit_front_end` (which runs only for package/use/notation
    // items) would silently skip ordinary files.
    crate::recognition::admit_capability_gates(tree, &mut diagnostics);

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
        let (declaration, mut tests, types, exprs, entries, admit_diagnostics, mut residuals) =
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
        // Remap local ExprIds and TypeIds to the package's global index
        // space before merging the arenas. Without this, declaration 2's
        // ExprId(0) would alias declaration 1's first expression.
        let expr_offset = u32::try_from(package.exprs.len()).unwrap_or(u32::MAX);
        let type_offset = u32::try_from(package.types.len()).unwrap_or(u32::MAX);
        remap_ids(&mut declaration, &mut tests, &mut residuals, expr_offset, type_offset);
        if !residuals.is_empty() {
            package.residuals.insert(declaration.id, residuals);
        }
        package.types.extend(types);
        for (e, _) in &exprs {
            let mut node = e.clone();
            remap_expr_node(&mut node, expr_offset, type_offset);
            package.exprs.push(node);
        }
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
