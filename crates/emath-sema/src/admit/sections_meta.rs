//! Meta section admission: about, evidence, host bindings, text helpers,
//! and the top-level `check_tree` entry point, extracted from `sections.rs`
//! isomorphically.

use emath_core::Diagnostics;
use emath_core::tree::{CommandArgument, Expr, ExprKind, Section, StmtKind, SyntaxTree};
use emath_ir::evidence::{ClaimVerdict, EvidenceClaim};
use emath_ir::goal::EvidenceLevel;
use emath_ir::ids::{ExprId, TypeId};
use emath_ir::{EventDecl, ExprNode, LawMetadata, Provenance, SliceAxis, TransitionDecl};
use emath_ir::{HostBinding, HostMethod, ImportEntry, ImportSelection, ModelResidual};
use std::collections::{BTreeMap, BTreeSet};

use super::equations::is_infer_marker;
use super::infer::infer_from_node;
use super::types::{map_type, type_display};
use super::{
    Admitter, CapabilityCallBinding, CheckResult, SemanticTrace, SiblingFunction,
    admit_declaration, confusable_fold,
};

mod host;
mod provenance;
mod remap;

pub(super) use host::*;
pub(super) use provenance::*;
pub(super) use remap::*;

/// Parse the whole file and admit every declaration (used by the session).
pub fn check_tree(tree: &SyntaxTree) -> CheckResult {
    let mut diagnostics = Diagnostics::new();
    let mut trace = SemanticTrace::default();
    let mut package = emath_ir::SemanticPackage::new();

    let has_declaration = tree
        .items
        .iter()
        .any(|item| matches!(item, emath_core::tree::Item::Declaration(_)));

    // Package/import recognition and local kind collection are structural and
    // run uniformly; no declaration spelling selects a domain recognizer.
    let front_end = crate::recognition::admit_front_end(tree, &mut diagnostics, &mut trace);
    package.package_path = front_end.package_path;
    package.imports = front_end.imports;
    let kind_defs = crate::recognition::collect_kind_defs(tree);
    // Item-attribute governance (ELP lane, units profiles, sig-fig
    // contracts) runs file-wide before any declaration admission so
    // experimental syntax is never silently admitted.
    super::attributes::admit_capability_gates(tree, &mut diagnostics);
    let units_profiles = super::attributes::admit_units_profiles(tree, &mut diagnostics);
    if !has_declaration {
        diagnostics.error("E-PKG-081", "source has no declarations", tree.source);
        return CheckResult {
            package,
            diagnostics,
            trace,
            units_profiles,
        };
    }
    let host_types = host_imported_types(&package.imports);

    // Sibling `emath function` declarations callable from lowering time
    // head-args or `inputs:`/`outputs:` section form. This
    // is function DATA for the generic declared-call seam's inline path —
    // no new AST node, no registry entry. A callee whose parameter types
    // do not map is not registered; its own admission reports the error.
    let mut sibling_functions: BTreeMap<String, SiblingFunction> = BTreeMap::new();
    for item in &tree.items {
        let emath_core::tree::Item::Declaration(decl) = item else {
            continue;
        };
        if decl.as_kind != "function" {
            continue;
        }
        let mut params: Vec<(String, super::infer::Infer)> = Vec::new();
        let mut param_types_ok = true;
        // Metadata pass: type diagnostics were (or will be) reported by the
        // declaration's own admission — route map_type diagnostics into a
        // throwaway sink so a refused type site (bare `Real`, unknown name)
        // is reported exactly once, by the pass that owns it.
        let mut type_diagnostics = Diagnostics::new();
        let mut collect_param = |ty: &emath_core::tree::TypeExpr, name: &str| {
            // Untyped inputs are the Infer marker: admission defaults them
            // to Float64 (N-TYPE-001) without an error, so the sibling
            // signature must mirror that default instead of routing the
            // marker into `map_type` (which would emit a spurious
            // E-TYPE-001 "unknown type `Infer`" no other pass reports).
            if is_infer_marker(ty) {
                params.push((name.to_string(), super::infer::Infer::F64));
                return;
            }
            match map_type(ty, &mut type_diagnostics, &host_types) {
                Some(node) => params.push((name.to_string(), infer_from_node(&node))),
                None => param_types_ok = false,
            }
        };
        if let Some(signature) = &decl.signature {
            for param in &signature.params {
                collect_param(&param.ty, &param.name);
            }
        } else {
            for section in decl.body.iter().filter_map(|stmt| match &stmt.kind {
                emath_core::tree::StmtKind::Section(section) if section.name == "inputs" => {
                    Some(section)
                }
                _ => None,
            }) {
                for stmt in &section.suite.statements {
                    let emath_core::tree::StmtKind::FieldDecl { name, ty, .. } = &stmt.kind else {
                        continue;
                    };
                    collect_param(ty, name);
                }
            }
        }
        if !param_types_ok {
            continue;
        }
        let output_name = decl
            .body
            .iter()
            .find_map(|stmt| match &stmt.kind {
                emath_core::tree::StmtKind::Section(section) if section.name == "outputs" => {
                    section
                        .suite
                        .statements
                        .iter()
                        .find_map(|stmt| match &stmt.kind {
                            emath_core::tree::StmtKind::FieldDecl { name, .. } => {
                                Some(name.clone())
                            }
                            _ => None,
                        })
                }
                _ => None,
            })
            .unwrap_or_else(|| decl.name.clone());
        let definitions: Vec<emath_core::tree::Stmt> = if decl.signature.is_some() {
            decl.body
                .iter()
                .filter(|stmt| matches!(stmt.kind, emath_core::tree::StmtKind::Assign { .. }))
                .cloned()
                .collect()
        } else {
            decl.body
                .iter()
                .filter_map(|stmt| match &stmt.kind {
                    emath_core::tree::StmtKind::Section(section)
                        if section.name == "definitions" =>
                    {
                        Some(section.suite.statements.clone())
                    }
                    _ => None,
                })
                .flatten()
                .collect()
        };
        // Alpha-rename the parameters inside the callee's own body to
        // `param#owner`: `#` is not a valid identifier
        // character (lexer: alphanumeric, `_`, alphabetic, combining
        // marks), so a renamed parameter can never collide with a caller
        // variable and the inline substitution can never make a
        // definition self-referential. One rename per function at
        // collection time; call sites bind the renamed names.
        let rename_map: BTreeMap<String, String> = params
            .iter()
            .map(|(name, _)| {
                (
                    name.clone(),
                    super::lowering::sibling_calls::renamed_parameter(&decl.name, name),
                )
            })
            .collect();
        let definitions: Vec<emath_core::tree::Stmt> = definitions
            .into_iter()
            .map(|stmt| {
                let emath_core::tree::StmtKind::Assign { target, value } = &stmt.kind else {
                    return stmt;
                };
                emath_core::tree::Stmt {
                    kind: emath_core::tree::StmtKind::Assign {
                        target: target.clone(),
                        value: super::lowering::sibling_calls::rename_parameter_uses(
                            value,
                            &rename_map,
                            &mut Vec::new(),
                        ),
                    },
                    source: stmt.source,
                }
            })
            .collect();
        let params = params
            .into_iter()
            .map(|(name, infer)| {
                (
                    super::lowering::sibling_calls::renamed_parameter(&decl.name, &name),
                    infer,
                )
            })
            .collect();
        sibling_functions.insert(
            decl.name.clone(),
            SiblingFunction {
                params,
                output_name,
                definitions,
            },
        );
    }

    let mut declaration_id = 0_u32;
    let mut seen_declaration_names: BTreeSet<String> = BTreeSet::new();
    let mut seen_folded_declaration_names: BTreeMap<String, String> = BTreeMap::new();
    // Declared capability cells' output-type text, keyed by canonical
    // cell name, captured when a cell admits cleanly. This is the
    // cell's OWN contract data for the generic capability-call path —
    // never a guessed type.
    let mut capability_output_types: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut capability_arities: BTreeMap<String, Option<usize>> = BTreeMap::new();
    let mut capability_inputs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut capability_diagnostics: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut capability_aliases: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for binding in crate::language::language_bindings() {
        package.capabilities.push(emath_ir::Capability {
            name: emath_core::QualifiedName(binding.feature_id.clone()),
            class: emath_ir::CellClass::Pure,
        });
        capability_output_types.insert(binding.feature_id.clone(), binding.output);
        capability_arities.insert(binding.feature_id.clone(), binding.arity);
        capability_inputs.insert(binding.feature_id.clone(), binding.inputs);
        capability_diagnostics.insert(binding.feature_id.clone(), binding.diagnostic);
        capability_aliases.insert(binding.feature_id, binding.aliases);
    }
    for item in &tree.items {
        let emath_core::tree::Item::Declaration(decl) = item else {
            continue;
        };
        let local_kind_application =
            decl.item_kind == "custom" && kind_defs.contains_key(&decl.as_kind);
        if decl.item_kind != "custom" || local_kind_application {
            crate::recognition::admit_declaration(
                decl,
                &kind_defs,
                &mut package,
                &mut diagnostics,
                &mut trace,
            );
            continue;
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
        // Parser remaps `emath kind Name:` to `item_kind=custom`,
        // `as_kind=kind`; validate and register its local structural schema.
        if decl.as_kind == "kind" {
            let mut kind_decl = decl.clone();
            kind_decl.item_kind = "kind".to_string();
            crate::recognition::admit_declaration(
                &kind_decl,
                &BTreeMap::new(),
                &mut package,
                &mut diagnostics,
                &mut trace,
            );
            continue;
        }
        // `emath field_pack Name:`: pack exports are
        // artifact data admitted at the recognition seam — never lowered
        // into strict meaning, never a silent custom fallthrough.
        if decl.as_kind == "field_pack" {
            crate::recognition::admit_field_pack(decl, &mut package, &mut diagnostics, &mut trace);
            continue;
        }
        // `emath feature Name:` is one generic mounted data shell. The
        // schema crate owns all twenty class rules; no feature name is
        // recognized here and candidate capsules gain no live authority.
        if decl.as_kind == "feature" {
            crate::recognition::admit_feature_capsule(
                decl,
                &mut package,
                &mut diagnostics,
                &mut trace,
            );
            continue;
        }
        if !matches!(
            decl.as_kind.as_str(),
            "function" | "policy" | "model" | "law"
        ) {
            let type_name = if decl.as_kind.is_empty() {
                "custom"
            } else {
                decl.as_kind.as_str()
            };
            diagnostics.error(
                "E-KIND-100",
                format!(
                    "declaration type `{type_name}` is outside the Phase 1 subset (function, policy, model, law)"
                ),
                decl.head_source,
            );
            continue;
        }
        // The generic declared/mounted capability surface: every cell in
        // the package's capability arena is callable by name — the
        // canonical dotted form, plus the bare declaration name when it
        // is unambiguous across cells. A call resolving here lowers to
        // `ExprNode::Apply` (the emitter's ApplyCapability path); no
        // builtin name is added and unknown names still refuse typed.
        let mut bare_cell_counts: BTreeMap<String, usize> = BTreeMap::new();
        for capability in &package.capabilities {
            let bare = capability
                .name
                .0
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_string();
            *bare_cell_counts.entry(bare).or_insert(0) += 1;
        }
        let capability_cells: Vec<CapabilityCallBinding> = package
            .capabilities
            .iter()
            .enumerate()
            .flat_map(|(index, capability)| {
                let capability_index = u32::try_from(index).unwrap_or(u32::MAX);
                let output = capability_output_types
                    .get(&capability.name.0)
                    .cloned()
                    .flatten();
                let arity = capability_arities
                    .get(&capability.name.0)
                    .copied()
                    .flatten();
                let inputs = capability_inputs
                    .get(&capability.name.0)
                    .cloned()
                    .unwrap_or_default();
                let diagnostic = capability_diagnostics
                    .get(&capability.name.0)
                    .cloned()
                    .flatten();
                let bare = capability
                    .name
                    .0
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let mut keys = vec![capability.name.0.clone()];
                if bare != capability.name.0 && bare_cell_counts.get(&bare).copied() == Some(1) {
                    keys.push(bare);
                }
                keys.extend(
                    capability_aliases
                        .get(&capability.name.0)
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
                keys.into_iter().map(move |key| CapabilityCallBinding {
                    key,
                    capability: capability_index,
                    inputs: inputs.clone(),
                    output: output.clone(),
                    arity,
                    diagnostic: diagnostic.clone(),
                })
            })
            .collect();
        let (
            declaration,
            mut tests,
            types,
            exprs,
            entries,
            admit_diagnostics,
            mut residuals,
            mut events,
            mut transitions,
            law_metadata,
            binding_provenance,
        ) = admit_declaration(decl, &host_types, &capability_cells, &sibling_functions);
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
        remap_ids(
            &mut declaration,
            &mut tests,
            &mut residuals,
            &mut events,
            &mut transitions,
            expr_offset,
            type_offset,
        );
        if !residuals.is_empty() {
            package.residuals.insert(declaration.id, residuals);
        }
        if !events.is_empty() {
            package.events.insert(declaration.id, events);
        }
        if !transitions.is_empty() {
            package.transitions.insert(declaration.id, transitions);
        }
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
    CheckResult {
        package,
        diagnostics,
        trace,
        units_profiles,
    }
}
