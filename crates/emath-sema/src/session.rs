//! Compiler session: load → check → plan. The build step (backend +
//! artifact emission) lives in `emath-build`.

use crate::admit::{CheckResult, check_tree};
use emath_core::parse::source_parser;
use emath_core::tree::{
    ArgumentValue, CommandArgument, ExprKind, Item, Section, StmtKind, SyntaxTree,
};
use emath_core::{Diagnostics, FileId, SourceStore, Span, limits::Limits};
use emath_ir::{
    GoalId, GoalKind, GoalPayload, RequestSpec, SemanticPackage, build_goal, native_plan,
    simplify_integer_expression,
};
use std::collections::BTreeMap;
use std::path::Path;

mod requests;

pub use requests::elaborate_requests;
pub(super) use requests::*;

/// Build policy knobs.
#[derive(Clone, Debug, Default)]
pub struct CompilerPolicy {
    /// Whether artifact build verifies the staged crate with `cargo test`.
    pub verify_generated_crate: bool,
}

/// A loaded source package.
#[derive(Clone, Debug)]
pub struct SourcePackage {
    pub file: FileId,
    pub name: String,
    pub text: String,
}

/// Result of `plan`: admitted package plus GIR goals and native plans.
#[derive(Clone, Debug)]
pub struct PlanResult {
    pub package: SemanticPackage,
    pub requests: Vec<RequestSpec>,
    pub plans: Vec<emath_ir::ResolutionPlan>,
    pub diagnostics: Diagnostics,
}

/// Result of the build step, produced by `emath-build` from a plan.
#[derive(Clone, Debug, Default)]
pub struct GeneratedCrate {
    pub crate_name: String,
    pub package_name: String,
    pub version: String,
    /// Relative path → file content.
    pub files: BTreeMap<String, String>,
    /// Source-map anchors (semantic label → generated range).
    pub anchors: Vec<EmittedAnchor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmittedAnchor {
    pub label: String,
    pub generated_file: String,
    pub generated_start: u32,
    pub generated_end: u32,
}

/// The compiler session facade (`PUBLIC_API_INVENTORY.md`): `load_package`,
/// `check`, `plan`, `build`.
pub struct CompilerSession {
    pub store: SourceStore,
    pub limits: Limits,
    edition: emath_core::Edition,
}

impl CompilerSession {
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            store: SourceStore::new(),
            limits,
            edition: emath_core::Edition::Ed2026,
        }
    }

    /// Construct a session pinned to a shipped package edition.
    #[must_use]
    pub fn with_edition(limits: Limits, edition: emath_core::Edition) -> Self {
        Self {
            store: SourceStore::new(),
            limits,
            edition,
        }
    }

    /// Package edition currently selecting parser behavior.
    #[must_use]
    pub fn edition(&self) -> emath_core::Edition {
        self.edition
    }

    /// Load a source file from disk into the session store.
    pub fn load_package(&mut self, path: impl AsRef<Path>) -> Result<SourcePackage, String> {
        let path = path.as_ref();
        if let Some(manifest) = nearest_manifest(path) {
            let manifest_text = std::fs::read_to_string(&manifest)
                .map_err(|error| format!("failed to read {}: {error}", manifest.display()))?;
            self.edition = edition_from_manifest(&manifest_text)?;
        }
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if let Err(max) = self.limits.check_source(text.len()) {
            return Err(format!(
                "source {} exceeds the {max}-byte session limit",
                path.display()
            ));
        }
        let file = self.store.add(path.display().to_string(), text.clone());
        Ok(SourcePackage {
            file,
            name: path.display().to_string(),
            text,
        })
    }

    /// Load an in-memory source under a display name.
    pub fn load_text(&mut self, name: impl Into<String>, text: impl Into<String>) -> FileId {
        self.store.add(name, text)
    }

    /// Parse a source string (no admission).
    pub fn parse_text(&self, text: &str) -> (emath_core::tree::SyntaxTree, Diagnostics) {
        match parse_through(text, &self.limits, self.edition) {
            Ok(pair) => pair,
            Err(diagnostics) => (
                emath_core::tree::SyntaxTree {
                    source: Span::default(),
                    items: Vec::new(),
                },
                diagnostics,
            ),
        }
    }

    /// `check`: parse + semantic admission.
    pub fn check(&mut self, file: FileId) -> CheckResult {
        let Some(source_file) = self.store.get(file) else {
            let mut diagnostics = Diagnostics::new();
            diagnostics.error("E-PKG-080", "source file was never loaded", Span::default());
            return CheckResult {
                package: SemanticPackage::new(),
                diagnostics,
                trace: crate::admit::SemanticTrace::default(),
                units_profiles: Vec::new(),
            };
        };
        let (tree, parse_diagnostics) =
            match parse_through(&source_file.text, &self.limits, self.edition) {
                Ok(pair) => pair,
                Err(diagnostics) => {
                    return CheckResult {
                        package: SemanticPackage::new(),
                        diagnostics,
                        trace: crate::admit::SemanticTrace::default(),
                        units_profiles: Vec::new(),
                    };
                }
            };
        let mut result = check_tree(&tree);
        result.diagnostics.extend_from(&parse_diagnostics);
        result
    }

    /// `check` on in-memory text.
    pub fn check_owned(&mut self, name: &str, text: &str) -> CheckResult {
        let file = self.load_text(name, text);
        self.check(file)
    }

    /// `check` over a multi-file package: the main file's in-package
    /// module imports — `use <package>.<module>` where the path prefix
    /// matches the file's own `package` line — resolve against the
    /// session's loaded `<module>.emath` sources, the siblings'
    /// declarations merge under the main file's package identity, and
    /// the merged tree goes through the normal admission lane
    /// (cross-file duplicate names refuse `E-NAME-022` there). An
    /// unresolved in-package module import refuses `E-PKG-050` — never
    /// a silent inert entry. The plain single-file [`Self::check`]
    /// keeps its existing behavior.
    pub fn check_package(&mut self, main: FileId) -> CheckResult {
        let Some(source_file) = self.store.get(main) else {
            let mut diagnostics = Diagnostics::new();
            diagnostics.error("E-PKG-080", "source file was never loaded", Span::default());
            return CheckResult {
                package: SemanticPackage::new(),
                diagnostics,
                trace: crate::admit::SemanticTrace::default(),
                units_profiles: Vec::new(),
            };
        };
        let (tree, mut parse_diagnostics) =
            match parse_through(&source_file.text, &self.limits, self.edition) {
                Ok(pair) => pair,
                Err(diagnostics) => {
                    return CheckResult {
                        package: SemanticPackage::new(),
                        diagnostics,
                        trace: crate::admit::SemanticTrace::default(),
                        units_profiles: Vec::new(),
                    };
                }
            };
        // The main file's package identity gates in-package module
        // imports (`use demo.geometry` under `package demo`); without a
        // package line every `use` stays on the normal lane.
        let package_path: Option<Vec<String>> = tree.items.iter().find_map(|item| {
            let Item::Package { path, .. } = item else {
                return None;
            };
            Some(path.clone())
        });
        // Resolve in-package module imports against the loaded sources,
        // merging the siblings' declarations (and notations) under the
        // main file's identity. Sibling `package`/`use` items stay the
        // main file's authority (transitive file imports are the next
        // slice).
        let mut pre_diagnostics = Diagnostics::new();
        let mut merged_items: Vec<Item> = tree.items.clone();
        for item in &tree.items {
            let Item::Use { path, source, .. } = item else {
                continue;
            };
            let Some(module) = file_import_module(path, package_path.as_deref()) else {
                continue;
            };
            let target = format!("{module}.emath");
            let Some(sibling) = self
                .store
                .files()
                .iter()
                .find(|file| file.name == target || file.name.ends_with(&format!("/{target}")))
                .cloned()
            else {
                pre_diagnostics.error(
                    "E-PKG-050",
                    format!(
                        "in-package module import `{module}` is not loaded in this session (load `{target}` with load_text/load_package)"
                    ),
                    *source,
                );
                continue;
            };
            match parse_through(&sibling.text, &self.limits, self.edition) {
                Ok((sibling_tree, sibling_parse)) => {
                    parse_diagnostics.extend_from(&sibling_parse);
                    for sibling_item in sibling_tree.items {
                        match sibling_item {
                            Item::Declaration(_) | Item::Notation(_) => {
                                merged_items.push(sibling_item);
                            }
                            Item::Package { .. } | Item::Use { .. } => {}
                        }
                    }
                }
                Err(sibling_diagnostics) => {
                    parse_diagnostics.extend_from(&sibling_diagnostics);
                }
            }
        }
        let mut result = check_tree(&SyntaxTree {
            source: tree.source,
            items: merged_items,
        });
        result.diagnostics.extend_from(&parse_diagnostics);
        result.diagnostics.extend_from(&pre_diagnostics);
        result
    }

    /// `plan`: elaborate requests into GIR and build deterministic native
    /// resolution plans.
    pub fn plan(&mut self, file: FileId) -> PlanResult {
        let Some(source_file) = self.store.get(file) else {
            // Missing source must be a typed refusal, not an empty-source
            // plan that silently passes admission (E-PKG-080).
            let mut diagnostics = Diagnostics::new();
            diagnostics.error("E-PKG-080", "source file was never loaded", Span::default());
            return PlanResult {
                package: SemanticPackage::new(),
                requests: Vec::new(),
                plans: Vec::new(),
                diagnostics,
            };
        };
        let text = source_file.text.clone();
        let (tree, parse_diagnostics) = match parse_through(&text, &self.limits, self.edition) {
            Ok(pair) => pair,
            Err(diagnostics) => {
                return PlanResult {
                    package: SemanticPackage::new(),
                    requests: Vec::new(),
                    plans: Vec::new(),
                    diagnostics,
                };
            }
        };
        let mut check = check_tree(&tree);
        check.diagnostics.extend_from(&parse_diagnostics);
        let mut diagnostics = check.diagnostics;
        let mut package = check.package;
        let mut requests = Vec::new();
        let mut plans = Vec::new();

        let declarations = package.declarations.clone();
        for (index, declaration) in declarations.iter().enumerate() {
            // Recover this declaration's sections from the syntax tree.
            let sections: Vec<emath_core::tree::Section> = tree
                .items
                .iter()
                .filter_map(|item| match item {
                    emath_core::tree::Item::Declaration(d) if d.name == declaration.name.leaf() => {
                        Some(d.sections_vec())
                    }
                    _ => None,
                })
                .flatten()
                .collect();
            let decl_requests = elaborate_requests(
                &package,
                declaration.name.leaf(),
                &sections,
                &mut diagnostics,
            );
            // Goals attach by construction (the ids built for this
            // declaration), never by span geometry: an overlapping offset
            // in another file must not cross-attach a goal.
            let mut goal_ids: Vec<GoalId> = Vec::new();
            for request in &decl_requests {
                let mut goal = build_goal(&mut package, request);
                if let Some(expression) = declaration.definitions.get(&request.target).copied() {
                    goal.expression = Some(expression);
                    package.goals[goal.id.index()].expression = Some(expression);
                }
                goal_ids.push(goal.id);
                let mut artifact_class = if goal.kind.as_str() == "evaluate" {
                    "native"
                } else {
                    "diagnostic"
                };
                if goal.kind == GoalKind::Simplify {
                    if let Some(expression) = goal.expression {
                        let integer_contract = declaration.inputs.iter().all(|field| {
                            matches!(
                                package.ty(field.ty),
                                Some(emath_ir::TypeNode::Int | emath_ir::TypeNode::Nat)
                            )
                        }) && declaration.outputs.iter().any(|field| {
                            field.name == goal.target
                                && matches!(
                                    package.ty(field.ty),
                                    Some(emath_ir::TypeNode::Int | emath_ir::TypeNode::Nat)
                                )
                        });
                        if !integer_contract {
                            diagnostics.error(
                                "E-SYM-003",
                                "`simplify` native v1 requires Int/Nat inputs and target output",
                                goal.source,
                            );
                        } else {
                            match simplify_integer_expression(&mut package, expression) {
                                Ok(simplified) => {
                                    goal.expression = Some(simplified.expression);
                                    package.goals[goal.id.index()].expression =
                                        Some(simplified.expression);
                                    artifact_class = "native-symbolic";
                                }
                                Err(error) => {
                                    diagnostics.error(error.code, error.message, goal.source);
                                }
                            }
                        }
                    }
                }
                plans.push(native_plan(goal.id, artifact_class));
            }
            package.declarations[index].goals = goal_ids;
            requests.extend(decl_requests);
        }
        package.seal();
        PlanResult {
            package,
            requests,
            plans,
            diagnostics,
        }
    }
}
