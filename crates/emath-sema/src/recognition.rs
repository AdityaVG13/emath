//! Recognition-level admission (corpus front-end).
//!
//! Package identity and `use` imports. Out-of-subset constructs get a
//! typed refusal; body expressions are validated structurally (typed
//! SIR lowering is the intent-compiler lane).

use crate::admit::SemanticTrace;
use emath_core::Diagnostics;
use emath_core::tree::{Item, UseTree};
use emath_ir::{ImportEntry, ImportSelection};

/// Declaration kinds admitted by this front-end.
pub const RECOGNIZED_KINDS: &[&str] = &["object", "function", "query"];

mod text;

pub use text::{expr_text, type_text};

// ---- admission ------------------------------------------------------------

/// Outcome of the file front-end.
#[derive(Clone, Debug, Default)]
pub struct V6FrontEnd {
    /// `package <dotted>` identity, if declared.
    pub package_path: Option<Vec<String>>,
    /// Admitted imports in source order.
    pub imports: Vec<ImportEntry>,
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
