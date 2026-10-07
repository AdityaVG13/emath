//! Compiler session: load → check → plan. The build step (backend +
//! artifact emission) lives in `emath-build`.

use crate::admit::{CheckResult, check_tree_at};
use emath_core::tree::{Item, SyntaxTree};
use emath_core::{Diagnostics, FileId, SourceStore, Span, limits::Limits};
use emath_ir::{RequestSpec, SemanticPackage};
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

    /// Package edition currently selecting parser behavior.
    #[must_use]
    pub fn edition(&self) -> emath_core::Edition {
        self.edition
    }

    /// Load a source file from disk into the session store.
    pub fn load_package(&mut self, path: impl AsRef<Path>) -> Result<SourcePackage, String> {
        let path = path.as_ref();
        // Ancestor search must include the CWD's parents for relative sources.
        // Keep lexical paths, display names and symlink policy unchanged.
        let manifest_source = std::path::absolute(path)
            .map_err(|error| format!("failed to resolve {}: {error}", path.display()))?;
        if let Some(manifest) = nearest_manifest(&manifest_source) {
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
        let file = self
            .store
            .try_add(path.display().to_string(), text.clone())?;
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

    /// `check`: parse + semantic admission.
    pub fn check(&mut self, file: FileId) -> CheckResult {
        let Some(source_file) = self.store.get(file) else {
            let mut diagnostics = Diagnostics::new();
            diagnostics.error("E-PKG-080", "source file was never loaded", Span::default());
            return CheckResult {
                syntax: None,
                parse_diagnostics: Diagnostics::new(),
                package: SemanticPackage::new(),
                diagnostics,
                trace: crate::admit::SemanticTrace::default(),
                units_profiles: Vec::new(),
                prepared: None,
            };
        };
        let (tree, parse_diagnostics) =
            match parse_through(&source_file.text, file, &self.limits, self.edition) {
                Ok(pair) => pair,
                Err(diagnostics) => {
                    return CheckResult {
                        syntax: None,
                        parse_diagnostics: diagnostics.clone(),
                        package: SemanticPackage::new(),
                        diagnostics,
                        trace: crate::admit::SemanticTrace::default(),
                        units_profiles: Vec::new(),
                        prepared: None,
                    };
                }
            };
        let source = Path::new(&source_file.name);
        let source = (!source_file.name.is_empty()).then_some(source);
        let mut result = check_tree_at(&tree, source);
        result.parse_diagnostics = parse_diagnostics.clone();
        result.diagnostics.extend_from(&parse_diagnostics);
        if result.diagnostics.has_errors() {
            result.prepared = None;
        }
        result
    }

    /// `check` on in-memory text.
    pub fn check_owned(&mut self, name: &str, text: &str) -> CheckResult {
        let file = self.load_text(name, text);
        self.check(file)
    }

    /// `check` over a multi-file package: the main file's in-package
    /// module imports (`use <package>.<module>` where the path prefix
    /// matches the file's own `package` line) resolve against the
    /// session's loaded `<module>.emath` sources, the siblings'
    /// declarations merge under the main file's package identity, and
    /// the merged tree goes through the normal admission lane
    /// (cross-file duplicate names refuse `E-NAME-022` there). An
    /// unresolved in-package module import refuses `E-PKG-050`, never
    /// a silent inert entry. Consumed in-package `use` items are
    /// stripped from the merged tree so admission does not re-resolve
    /// them from disk. The plain single-file [`Self::check`]
    /// keeps its existing behavior.
    pub fn check_package(&mut self, main: FileId) -> CheckResult {
        let Some(source_file) = self.store.get(main) else {
            let mut diagnostics = Diagnostics::new();
            diagnostics.error("E-PKG-080", "source file was never loaded", Span::default());
            return CheckResult {
                syntax: None,
                parse_diagnostics: Diagnostics::new(),
                package: SemanticPackage::new(),
                diagnostics,
                trace: crate::admit::SemanticTrace::default(),
                units_profiles: Vec::new(),
                prepared: None,
            };
        };
        let (tree, mut parse_diagnostics) =
            match parse_through(&source_file.text, main, &self.limits, self.edition) {
                Ok(pair) => pair,
                Err(diagnostics) => {
                    return CheckResult {
                        syntax: None,
                        parse_diagnostics: diagnostics.clone(),
                        package: SemanticPackage::new(),
                        diagnostics,
                        trace: crate::admit::SemanticTrace::default(),
                        units_profiles: Vec::new(),
                        prepared: None,
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
        // In-package `use` items are this lane's authority: each either
        // merges its sibling's declarations or refuses `E-PKG-050`, and
        // the item is consumed rather than kept in the merged tree:
        // the constructor admission must not re-resolve it from disk
        // (the session store, not the filesystem, is the source of
        // truth for this lane).
        let mut merged_items: Vec<Item> = Vec::with_capacity(tree.items.len());
        for item in &tree.items {
            let Item::Use { path, source, .. } = item else {
                merged_items.push(item.clone());
                continue;
            };
            let Some(module) = file_import_module(path, package_path.as_deref()) else {
                merged_items.push(item.clone());
                continue;
            };
            let target = format!("{module}.emath");
            let Some(sibling) = self
                .store
                .files()
                .iter()
                .rev()
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
            match parse_through(&sibling.text, sibling.id, &self.limits, self.edition) {
                Ok((sibling_tree, sibling_parse)) => {
                    parse_diagnostics.extend_from(&sibling_parse);
                    for sibling_item in sibling_tree.items {
                        match sibling_item {
                            Item::Declaration(_) | Item::Notation(_) => {
                                merged_items.push(sibling_item);
                            }
                            Item::Package { .. } | Item::Use { .. } | Item::Exports { .. } => {}
                        }
                    }
                }
                Err(sibling_diagnostics) => {
                    parse_diagnostics.extend_from(&sibling_diagnostics);
                }
            }
        }
        let source = Path::new(&source_file.name);
        let source = (!source_file.name.is_empty()).then_some(source);
        let mut result = check_tree_at(
            &SyntaxTree {
                source: tree.source,
                items: merged_items,
            },
            source,
        );
        result.diagnostics.extend_from(&parse_diagnostics);
        result.diagnostics.extend_from(&pre_diagnostics);
        result.parse_diagnostics = parse_diagnostics;
        if result.diagnostics.has_errors() {
            result.prepared = None;
        }
        result
    }

    /// Historical planner. Constructor work uses `check` and `emath run`.
    /// This entry still compiles leftover callers (RULE 1) and always
    /// refuses a `goals:` layer.
    pub fn plan(&mut self, file: FileId) -> PlanResult {
        let check = self.check(file);
        let mut diagnostics = check.diagnostics;
        diagnostics.error(
            "E-KIND-GONE",
            "there is no `goals:` planner; write an ordinary `emath function` or `emath query` and `emath run`",
            Span::default(),
        );
        PlanResult {
            package: check.package,
            requests: Vec::new(),
            plans: Vec::new(),
            diagnostics,
        }
    }
}
