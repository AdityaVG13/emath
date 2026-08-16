//! Compiler session: load → check → plan. The build step (backend +
//! artifact emission) lives in `emath-build`.

use crate::admit::{check_tree, CheckResult};
use emath_core::{limits::Limits, Diagnostics, FileId, SourceStore, Span};
use emath_ir::{build_goal, native_plan, RequestSpec, SemanticPackage};
use emath_syntax::parse_str;
use emath_syntax::tree::{CommandArgument, ExprKind, Section, StmtKind};
use std::collections::BTreeMap;
use std::path::Path;

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
}

impl CompilerSession {
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            store: SourceStore::new(),
            limits,
        }
    }

    /// Load a source file from disk into the session store.
    pub fn load_package(&mut self, path: impl AsRef<Path>) -> Result<SourcePackage, String> {
        let path = path.as_ref();
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
    pub fn parse_text(&self, text: &str) -> (emath_syntax::tree::SyntaxTree, Diagnostics) {
        parse_str(text)
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
            };
        };
        let (tree, parse_diagnostics) = parse_str(&source_file.text);
        let mut result = check_tree(&tree, &());
        result.diagnostics.extend_from(&parse_diagnostics);
        result
    }

    /// `check` on in-memory text.
    pub fn check_owned(&mut self, name: &str, text: &str) -> CheckResult {
        let file = self.load_text(name, text);
        self.check(file)
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
        let (tree, parse_diagnostics) = parse_str(&text);
        let mut check = check_tree(&tree, &());
        check.diagnostics.extend_from(&parse_diagnostics);
        let mut diagnostics = check.diagnostics;
        let mut package = check.package;
        let mut requests = Vec::new();
        let mut plans = Vec::new();

        let declarations = package.declarations.clone();
        for declaration in &declarations {
            // Recover this declaration's sections from the syntax tree.
            let sections: Vec<emath_syntax::tree::Section> = tree
                .items
                .iter()
                .filter_map(|item| match item {
                    // Declarations are admitted by the front-end; goal
                    // elaboration remains a custom (`emath custom`) path
                    // until the intent-compiler lane lands.
                    emath_syntax::tree::Item::Declaration(d)
                        if d.item_kind == "custom" && d.name == declaration.name.leaf() =>
                    {
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
            for request in &decl_requests {
                let goal = build_goal(&mut package, request);
                let artifact_class = if goal.kind.as_str() == "evaluate" {
                    "native"
                } else {
                    "diagnostic"
                };
                plans.push(native_plan(goal.id, artifact_class));
            }
            requests.extend(decl_requests);
        }
        // Attach goal ids to their declarations.
        for declaration in &mut package.declarations {
            let owner = declaration.source;
            declaration.goals = package
                .goals
                .iter()
                .filter(|goal| owner.contains(goal.source.start))
                .map(|goal| goal.id)
                .collect();
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

/// Extract the `requests:` section into request specs and validate targets
/// against the admitted declaration (`E-GOAL-041`/`E-GOAL-042`/`E-GOAL-043`).
///
/// Lives in the session (its only consumer; the crate docs list goal
/// elaboration as an orchestrated admission responsibility) rather than in
/// `emath-goal`, keeping the admission crate's import surface on
/// core/ir/syntax only.
pub fn elaborate_requests(
    package: &SemanticPackage,
    declaration_name: &str,
    sections: &[Section],
    diagnostics: &mut Diagnostics,
) -> Vec<RequestSpec> {
    let mut requests = Vec::new();
    let Some(section) = sections.iter().find(|s| s.name == "requests") else {
        return requests;
    };
    for stmt in &section.suite.statements {
        let StmtKind::Section(request) = &stmt.kind else {
            diagnostics.error(
                "E-SYN-101",
                "unexpected statement inside `requests:`",
                stmt.source,
            );
            continue;
        };
        match request.name.as_str() {
            "evaluate" => {
                let target = request.generic.clone().unwrap_or_default();
                if target.is_empty() {
                    diagnostics.error(
                        "E-GOAL-041",
                        "`evaluate` requires a target in `<...>`",
                        request.head_source,
                    );
                    continue;
                }
                let produce = read_produce(&request.suite);
                if produce.is_empty() {
                    diagnostics.error(
                        "E-GOAL-042",
                        "`evaluate` requires `produce rust.library` in Phase 1",
                        request.source,
                    );
                    continue;
                }
                if produce != "rust.library" {
                    // Accepting an arbitrary produce target would silently
                    // admit an unimplemented export surface; refuse.
                    diagnostics.error(
                        "E-GOAL-042",
                        format!(
                            "produce target `{produce}` is outside the Phase 1 subset (`rust.library` only)"
                        ),
                        request.source,
                    );
                    continue;
                }
                requests.push(RequestSpec {
                    kind: "evaluate".into(),
                    target,
                    produce,
                    source: request.source,
                });
            }
            other => {
                diagnostics.error(
                    "E-GOAL-043",
                    format!(
                        "request kind `{other}` is outside the Phase 1 subset (supported: evaluate)"
                    ),
                    request.source,
                );
            }
        }
    }
    // targets must be outputs or definitions
    let declared: Vec<&String> = package
        .declarations
        .iter()
        .find(|d| d.name.leaf() == declaration_name)
        .map(|d| {
            d.outputs
                .iter()
                .map(|f| &f.name)
                .chain(d.definitions.keys())
                .collect()
        })
        .unwrap_or_default();
    for request in &requests {
        if !declared.contains(&&request.target) {
            diagnostics.error(
                "E-GOAL-041",
                format!(
                    "request target `{}` is not an output or definition",
                    request.target
                ),
                request.source,
            );
        }
    }
    requests
}

fn read_produce(suite: &emath_syntax::tree::Suite) -> String {
    for stmt in &suite.statements {
        if let StmtKind::Command { head, argument } = &stmt.kind {
            if head.first().is_some_and(|h| h == "produce") {
                if let Some(CommandArgument::Expr(expr)) = argument {
                    if let ExprKind::Path { segments, .. } = &expr.kind {
                        return segments.join(".");
                    }
                }
                if head.len() > 1 {
                    return head[1..].join(".");
                }
            }
        }
    }
    String::new()
}
