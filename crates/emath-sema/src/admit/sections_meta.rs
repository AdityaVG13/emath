//! Meta section admission: about, evidence, host bindings, text helpers,
//! and the top-level `check_tree` entry point, extracted from `sections.rs`
//! isomorphically.

use emath_core::Diagnostics;
use emath_core::tree::SyntaxTree;
use emath_ir::ids::{ExprId, TypeId};
use emath_ir::{ExprNode, SliceAxis};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{
    CheckResult, SemanticTrace, admit_constructor_declaration, confusable_fold,
};

mod remap;

use remap::{remap_ids, remap_expr_node};

/// Parse the whole file and admit every declaration (used by the session).
pub fn check_tree(tree: &SyntaxTree) -> CheckResult {
    check_tree_at(tree, None)
}

/// Admit a constructor file, resolving `use` from `source` when present.
pub fn check_tree_at(tree: &SyntaxTree, source: Option<&Path>) -> CheckResult {
    let mut diagnostics = Diagnostics::new();
    let mut trace = SemanticTrace::default();
    let mut package = emath_ir::SemanticPackage::new();

    let has_declaration = tree
        .items
        .iter()
        .any(|item| matches!(item, emath_core::tree::Item::Declaration(_)));

    // Package/import recognition is structural and runs uniformly; no
    // declaration spelling selects a domain recognizer.
    let front_end = crate::recognition::admit_front_end(tree, &mut diagnostics, &mut trace);
    package.package_path = front_end.package_path;
    package.imports = front_end.imports;
    // Item-attribute governance (ELP lane, units profiles, sig-fig
    // contracts) runs file-wide before any declaration admission so
    // experimental syntax is never silently admitted.
    super::attributes::admit_capability_gates(tree, &mut diagnostics);
    let units_profiles = Vec::new();
    if !has_declaration {
        diagnostics.error("E-PKG-081", "source has no declarations", tree.source);
        return CheckResult {
            package,
            diagnostics,
            trace,
            units_profiles,
        };
    }
    for item in &tree.items {
        if let emath_core::tree::Item::Notation(notation) = item {
            diagnostics.error(
                "E-KIND-GONE",
                "notation aliases are not constructor surface; write the scalar operator or `use` an ordinary module",
                notation.source,
            );
        }
    }
    check_constructor_user_file(tree, source, diagnostics, package, trace, units_profiles)
}

fn check_constructor_user_file(
    tree: &SyntaxTree,
    source: Option<&Path>,
    mut diagnostics: Diagnostics,
    mut package: emath_ir::SemanticPackage,
    mut trace: SemanticTrace,
    units_profiles: Vec<(String, String)>,
) -> CheckResult {
    let mut declaration_id = 0_u32;
    let mut seen_declaration_names: BTreeSet<String> = BTreeSet::new();
    let mut seen_folded_declaration_names: BTreeMap<String, String> = BTreeMap::new();
    for item in &tree.items {
        let emath_core::tree::Item::Declaration(decl) = item else {
            continue;
        };
        if !seen_declaration_names.insert(decl.name.clone()) {
            diagnostics.error(
                "E-NAME-022",
                format!("duplicate declaration name `{}`", decl.name),
                decl.head_source,
            );
            continue;
        }
        if decl.name == "_" {
            diagnostics.error(
                "E-NAME-023",
                "declaration name `_` is reserved and cannot be a Rust type",
                decl.head_source,
            );
            continue;
        }
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
                    "declaration kind `{}` is not supported; write `emath object`, `emath function`, or `emath query`",
                    decl.item_kind
                ),
                decl.head_source,
            );
            continue;
        }
        if !matches!(decl.as_kind.as_str(), "object" | "function" | "query") {
            // The parser maps `emath custom C:` to an empty `as_kind`;
            // the refusal names what the user wrote.
            let kind_name = if decl.as_kind.is_empty() {
                "custom"
            } else {
                decl.as_kind.as_str()
            };
            diagnostics.error(
                "E-KIND-GONE",
                format!(
                    "declaration kind `{kind_name}` is not a core kind; write `emath object`, `emath function`, or `emath query`"
                ),
                decl.head_source,
            );
            continue;
        }
        let (
            declaration,
            mut tests,
            types,
            exprs,
            entries,
            admit_diagnostics,
            law_metadata,
            binding_provenance,
        ) = admit_constructor_declaration(decl);
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
        let expr_offset = u32::try_from(package.exprs.len()).unwrap_or(u32::MAX);
        let type_offset = u32::try_from(package.types.len()).unwrap_or(u32::MAX);
        remap_ids(&mut declaration, &mut tests, expr_offset, type_offset);
        if let Some(metadata) = law_metadata {
            package.law_metadata.insert(declaration.id, metadata);
        }
        for (binding, provenance) in binding_provenance {
            package.binding_provenance.insert(
                emath_ir::BindingSite::new(declaration.id, binding),
                provenance,
            );
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
    if let Err(err) = emath_exec_ir::constructor_layer::admit_tree_at(tree, source) {
        let code = emath_exec_ir::constructor_layer::constructor_admit_code(&err.code);
        // The per-declaration loop above already reported kind refusals
        // with real spans; this constructor pass is first-error-only. Append
        // its finding only when the loop did not record the same refusal —
        // one report per defect, never a duplicate emission.
        let already_reported = diagnostics
            .errors()
            .any(|d| d.code == code && d.message == err.message);
        if !already_reported {
            diagnostics.error(code, err.message, tree.source);
        }
    }
    CheckResult {
        package,
        diagnostics,
        trace,
        units_profiles,
    }
}
