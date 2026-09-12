//! Recognition-level admission (corpus front-end).
//!
//! Package identity, `use` imports, declaration-kind admission, the
//! custom-kind schema registry, and per-kind structural validation.
//! Out-of-subset constructs get a typed refusal; body expressions are
//! validated structurally (typed SIR lowering is the intent-compiler lane).

use crate::admit::SemanticTrace;
use emath_core::Diagnostics;
use emath_core::tree::{Expr, ExprKind, Item, StmtKind, UseTree};
use emath_ir::{ImportEntry, ImportSelection};
use std::collections::BTreeMap;

/// Declaration kinds admitted by this front-end.
pub const RECOGNIZED_KINDS: &[&str] = &["object", "function", "query"];

mod capability;
mod declaration;
mod feature_capsule;
mod reaction;
mod schema;
mod sections;
mod text;

pub use declaration::admit_declaration;
pub(crate) use declaration::validate_kind_application;
pub(super) use sections::*;

pub use schema::{KindDef, SchemaRule};
pub use text::{expr_text, type_text};

use schema::*;

// ---- admission ------------------------------------------------------------

/// Outcome of the file front-end.
#[derive(Clone, Debug, Default)]
pub struct V6FrontEnd {
    /// `package <dotted>` identity, if declared.
    pub package_path: Option<Vec<String>>,
    /// Admitted imports in source order.
    pub imports: Vec<ImportEntry>,
}

/// `emath kind` is not a core kind. The custom-kind registry is empty.
#[must_use]
pub fn collect_kind_defs(tree: &emath_core::tree::SyntaxTree) -> BTreeMap<String, KindDef> {
    let _ = tree;
    BTreeMap::new()
}

#[allow(dead_code)]
fn schema_rules_from_section(section: &emath_core::tree::Section) -> Vec<SchemaRule> {
    let mut rules = Vec::new();
    for stmt in &section.suite.statements {
        match &stmt.kind {
            StmtKind::Require(expr) => {
                if let Some(head) = require_head(expr) {
                    rules.push(head);
                }
            }
            // `allow section <name>` → [.., "section", name].
            StmtKind::Command { head, .. }
                if head.first().map(String::as_str) == Some("allow")
                    && head.get(1).map(String::as_str) == Some("section") =>
            {
                if let Some(name) = head.get(2) {
                    rules.push(SchemaRule::AllowSection(name.clone()));
                }
            }
            _ => {}
        }
    }
    rules
}

/// Interpret a `require <expr>` schema statement as a rule.
/// The parser folds `section input` / `exactly_one output` into a plain
/// path expression (`["section", "input"]`).
#[allow(dead_code)]
fn require_head(expr: &Expr) -> Option<SchemaRule> {
    let ExprKind::Path { segments, .. } = &expr.kind else {
        return None;
    };
    match segments.first().map(String::as_str) {
        Some("section") => segments.get(1).cloned().map(SchemaRule::RequireSection),
        Some("exactly_one") => segments
            .get(1)
            .cloned()
            .map(SchemaRule::RequireExactlyOneSection),
        _ => None,
    }
}

/// Admit the file-level front-end items (`package`, `use`).
pub fn admit_front_end(
    tree: &emath_core::tree::SyntaxTree,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) -> V6FrontEnd {
    let mut result = V6FrontEnd::default();
    for item in &tree.items {
        match item {
            Item::Package { path, source } => {
                result.package_path = Some(path.clone());
                trace.record("recognize:package", path.join("."), Some(*source));
            }
            Item::Use { path, tree, source } => {
                if is_external_import(path) {
                    diagnostics.error(
                        "E-PKG-050",
                        format!(
                            "external file import `{}` is outside the front-end subset (library-path imports only)",
                            path.join(".")
                        ),
                        *source,
                    );
                    continue;
                }
                let mut path = path.clone();
                let selection = match tree {
                    UseTree::All => ImportSelection::All,
                    UseTree::Named(names) if names.is_empty() && path.len() >= 2 => {
                        // `use package.path.Name`: the parser keeps the
                        // single imported name in the path.
                        let name = path.pop().unwrap_or_default();
                        ImportSelection::Named(vec![(name, None)])
                    }
                    UseTree::Named(names) => ImportSelection::Named(
                        names.iter().map(|(n, a)| (n.clone(), a.clone())).collect(),
                    ),
                };
                trace.record(
                    "recognize:import",
                    format!("{} {selection:?}", path.join("::")),
                    Some(*source),
                );
                result.imports.push(ImportEntry {
                    path,
                    selection,
                    source: *source,
                });
            }
            Item::Declaration(_) => {}
            Item::Notation(_) => {}
        }
    }
    result
}

fn is_external_import(path: &[String]) -> bool {
    let Some(first) = path.first() else {
        return true;
    };
    first.starts_with("./")
        || first.starts_with("../")
        || first.starts_with('/')
        || path
            .iter()
            .any(|segment| segment.to_ascii_lowercase().ends_with(".emath"))
}
