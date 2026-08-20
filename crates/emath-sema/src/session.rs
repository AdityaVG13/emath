//! Compiler session: load → check → plan. The build step (backend +
//! artifact emission) lives in `emath-build`.

use crate::admit::{check_tree, CheckResult};
use emath_core::parse::source_parser;
use emath_core::tree::{CommandArgument, ExprKind, Section, StmtKind};
use emath_core::{limits::Limits, Diagnostics, FileId, SourceStore, Span};
use emath_ir::{build_goal, native_plan, GoalId, GoalPayload, RequestSpec, SemanticPackage};
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
    pub fn parse_text(&self, text: &str) -> (emath_core::tree::SyntaxTree, Diagnostics) {
        match parse_through(text, &self.limits) {
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
            };
        };
        let (tree, parse_diagnostics) = match parse_through(&source_file.text, &self.limits) {
            Ok(pair) => pair,
            Err(diagnostics) => {
                return CheckResult {
                    package: SemanticPackage::new(),
                    diagnostics,
                    trace: crate::admit::SemanticTrace::default(),
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
        let (tree, parse_diagnostics) = match parse_through(&text, &self.limits) {
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
                    // Declarations are admitted by the front-end; goal
                    // elaboration remains a custom (`emath custom`) path
                    // until the intent-compiler lane lands.
                    emath_core::tree::Item::Declaration(d)
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
            // Goals attach by construction (the ids built for this
            // declaration), never by span geometry: an overlapping offset
            // in another file must not cross-attach a goal.
            let mut goal_ids: Vec<GoalId> = Vec::new();
            for request in &decl_requests {
                let goal = build_goal(&mut package, request);
                goal_ids.push(goal.id);
                let artifact_class = if goal.kind.as_str() == "evaluate" {
                    "native"
                } else {
                    "diagnostic"
                };
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

/// Extract the `goals:` section into request specs and validate targets
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
    let Some(section) = sections.iter().find(|s| s.name == "goals") else {
        // Ergonomics default: with no `goals:` section, every definition is
        // an evaluate goal (`produce rust.library`). Declaring `goals:`
        // selects the subset you want; definitions stay queryable either
        // way. The request carries the declaration head as its source so
        // goal ownership attaches to the declaration.
        let Some(declaration) = package
            .declarations
            .iter()
            .find(|d| d.name.leaf() == declaration_name)
        else {
            return requests;
        };
        for target in declaration.definitions.keys() {
            requests.push(RequestSpec {
                kind: "evaluate".into(),
                target: target.clone(),
                produce: "rust.library".into(),
                payload: GoalPayload::default(),
                source: declaration.source,
            });
        }
        return requests;
    };
    for stmt in &section.suite.statements {
        let StmtKind::Section(request) = &stmt.kind else {
            diagnostics.error(
                "E-SYN-101",
                "unexpected statement inside `goals:`",
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
                    payload: GoalPayload::default(),
                    source: request.source,
                });
            }
            "differentiate" => {
                let target = request.generic.clone().unwrap_or_default();
                if target.is_empty() {
                    diagnostics.error(
                        "E-GOAL-041",
                        "`differentiate` requires a target in `<...>`",
                        request.head_source,
                    );
                    continue;
                }
                let payload = read_payload(&request.suite);
                if payload.wrt.is_empty() {
                    diagnostics.error(
                        "E-GOAL-044",
                        "`differentiate` requires `wrt [names]`",
                        request.source,
                    );
                    continue;
                }
                requests.push(RequestSpec {
                    kind: "differentiate".into(),
                    target,
                    produce: String::new(),
                    payload,
                    source: request.source,
                });
            }
            "benchmark" => {
                let target = request.generic.clone().unwrap_or_default();
                if target.is_empty() {
                    diagnostics.error(
                        "E-GOAL-041",
                        "`benchmark` requires a target in `<...>`",
                        request.head_source,
                    );
                    continue;
                }
                let payload = read_payload(&request.suite);
                if payload.against.is_none() {
                    diagnostics.error(
                        "E-GOAL-045",
                        "`benchmark` requires `against <path>`",
                        request.source,
                    );
                    continue;
                }
                requests.push(RequestSpec {
                    kind: "benchmark".into(),
                    target,
                    produce: String::new(),
                    payload,
                    source: request.source,
                });
            }
            other => {
                diagnostics.error(
                    "E-GOAL-043",
                    format!(
                        "request kind `{other}` is outside the Phase 1 subset (supported: evaluate, differentiate, benchmark)"
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

fn read_produce(suite: &emath_core::tree::Suite) -> String {
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

fn read_payload(suite: &emath_core::tree::Suite) -> GoalPayload {
    let mut payload = GoalPayload::default();
    for stmt in &suite.statements {
        let StmtKind::Command { head, argument } = &stmt.kind else {
            continue;
        };
        let Some(word) = head.first() else {
            continue;
        };
        match word.as_str() {
            "wrt" => payload.wrt = command_names(head, argument.as_ref()),
            "order" => {
                payload.order = command_u32(head, argument.as_ref());
            }
            "against" => {
                let path = command_path(head, argument.as_ref());
                if !path.is_empty() {
                    payload.against = Some(path);
                }
            }
            "measure" => payload.measure = command_names(head, argument.as_ref()),
            _ => {}
        }
    }
    payload
}

fn command_names(head: &[String], argument: Option<&CommandArgument>) -> Vec<String> {
    match argument {
        Some(CommandArgument::List(items)) => items
            .iter()
            .filter_map(|item| match &item.kind {
                ExprKind::Path { segments, .. } => Some(segments.join(".")),
                _ => None,
            })
            .collect(),
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Path { segments, .. } => vec![segments.join(".")],
            ExprKind::List(items) => items
                .iter()
                .filter_map(|item| match &item.kind {
                    ExprKind::Path { segments, .. } => Some(segments.join(".")),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        },
        None if head.len() > 1 => head[1..].to_vec(),
        _ => Vec::new(),
    }
}

fn command_path(head: &[String], argument: Option<&CommandArgument>) -> String {
    match argument {
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Path { segments, .. } => segments.join("::"),
            _ => String::new(),
        },
        None if head.len() > 1 => head[1..].join("::"),
        _ => String::new(),
    }
}

fn command_u32(head: &[String], argument: Option<&CommandArgument>) -> Option<u32> {
    let text = match argument {
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Int(text) | ExprKind::Float(text) => text.as_str(),
            _ => return None,
        },
        None => head.get(1).map(String::as_str)?,
        _ => return None,
    };
    text.parse().ok()
}
/// Parse through the installed source-parser backend.
///
/// Returns a typed refusal (E-SYN-120) when no backend is installed; hosts
/// wire `emath_syntax::install_source_parser` once per process at startup.
fn parse_through(
    text: &str,
    limits: &Limits,
) -> Result<(emath_core::tree::SyntaxTree, Diagnostics), Diagnostics> {
    let Some(parser) = source_parser() else {
        let mut diagnostics = Diagnostics::new();
        diagnostics.error(
            "E-SYN-120",
            "source parser backend not installed: call emath_syntax::install_source_parser once per process before parsing",
            Span::default(),
        );
        return Err(diagnostics);
    };
    Ok(parser.parse(text, FileId(0), limits))
}
