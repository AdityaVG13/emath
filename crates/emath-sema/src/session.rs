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

/// Extract the `goals:` section into request specs and validate targets
/// against the admitted declaration (`E-GOAL-041`/`E-GOAL-042`/`E-GOAL-043`).
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
                // F3 (emath-r3-docs-fixes-3wtm): the flat-goal sugar — a
                // heading line with no indented payload — is permanently
                // refused. A goal is never guessed from a heading; write
                // the command form (`produce rust.library`).
                if request.suite.statements.is_empty() {
                    diagnostics.error(
                        "E-GOAL-042",
                        format!(
                            "flat goal `evaluate <{target}>:` is permanently refused: a goal heading with no payload is never guessed; write the command form (`evaluate <{target}>:` with `produce rust.library` inside)"
                        ),
                        request.source,
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
                // F3: same flat-goal rule as `evaluate`.
                if request.suite.statements.is_empty() {
                    diagnostics.error(
                        "E-GOAL-044",
                        format!(
                            "flat goal `differentiate <{target}>:` is permanently refused: write the command form (`differentiate <{target}>:` with `wrt [names]` inside)"
                        ),
                        request.source,
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
            "simplify" => {
                let target = request.generic.clone().unwrap_or_default();
                if target.is_empty() {
                    diagnostics.error(
                        "E-GOAL-041",
                        "`simplify` requires a target in `<...>`",
                        request.head_source,
                    );
                    continue;
                }
                requests.push(RequestSpec {
                    kind: "simplify".into(),
                    target,
                    produce: String::new(),
                    payload: GoalPayload::default(),
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
            // 04 §5.3 (emath-r3-fit-goal-4xjh): the generic fit goal
            // `fit <params> to <observable>:`. The whole fit program is
            // plain payload data (model path, prediction label, residual
            // method, optimizer method, initial seeds, explicit weights,
            // identifiability gate) — nothing domain-specific is bound
            // here; execution goes through the generic fit-goal runtime
            // seams (crates/emath-lab-core calibration module), and without a
            // structural-identifiability provider the goal resolves to
            // an honest typed unresolved disposition in every plan.
            "fit" => {
                let observable = request.generic.clone().unwrap_or_default();
                if observable.is_empty() {
                    diagnostics.error(
                        "E-GOAL-041",
                        "`fit` requires an observable name after `to` \
                         (`fit <params> to <observable>:`)",
                        request.head_source,
                    );
                    continue;
                }
                let parameters = fit_parameters(request);
                if parameters.is_empty() {
                    diagnostics.error(
                        "E-GOAL-041",
                        "`fit` requires at least one parameter (`fit <params> to <observable>:`)",
                        request.head_source,
                    );
                    continue;
                }
                let (mut payload, unrecognized) = read_fit_payload(&request.suite);
                // The parameter list is part of the fit program (reproduced
                // losslessly into the runtime goal; declared order fixed by
                // the head, independent of map iteration).
                payload.parameters = parameters;
                if payload.residual.is_empty() {
                    diagnostics.error(
                        "E-GOAL-042",
                        "`fit` requires `residual: <method>` (e.g. `residual: weighted_least_squares`)",
                        request.source,
                    );
                    continue;
                }
                if payload.method.is_empty() {
                    diagnostics.error(
                        "E-GOAL-042",
                        "`fit` requires `method <optimizer>` (e.g. `method levenberg_marquardt`)",
                        request.source,
                    );
                    continue;
                }
                for (row, span) in unrecognized {
                    diagnostics.error(
                        "E-GOAL-042",
                        format!("unrecognized fit row `{row}` (fit rows: model, prediction, residual, method, initial, weights, data, require identifiability structural)"),
                        span,
                    );
                }
                requests.push(RequestSpec {
                    kind: "fit".into(),
                    target: observable,
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

/// Parameter names of a `fit <params> to <observable>:` head, in
/// declared order (the parser stores them as path-expression
/// arguments).
fn fit_parameters(request: &emath_core::tree::Section) -> Vec<String> {
    let mut parameters = Vec::new();
    let Some(args) = &request.args else {
        return parameters;
    };
    for argument in args {
        if let ArgumentValue::Expr(expr) = &argument.value {
            if let ExprKind::Path { segments, generics: None } = &expr.kind {
                parameters.push(segments.join("."));
            }
        }
    }
    parameters
}

/// Generic fit-program payload from the fit goal's rows (04 §5.3): the
/// whole program is plain data — model path, prediction label, residual
/// method, optimizer method, initial seeds, explicit weights, and the
/// `require identifiability: structural` honesty gate. Unrecognized
/// rows are returned as typed refusals (never silent drops).
fn read_fit_payload(suite: &emath_core::tree::Suite) -> (GoalPayload, Vec<(String, Span)>) {
    let mut payload = GoalPayload::default();
    let mut unrecognized = Vec::new();
    for stmt in &suite.statements {
        match &stmt.kind {
            StmtKind::FieldDecl { name, ty, default, .. } => {
                let leaf = type_path_text(ty);
                match name.as_str() {
                    "residual" => payload.residual = leaf,
                    "method" => payload.method = leaf,
                    "initial" | "weights" => {
                        let Some(value) = default.as_ref() else {
                            unrecognized.push((
                                format!("`{name}: {leaf}` requires a numeric value (`{name}: <param> = <number>`)"),
                                stmt.source,
                            ));
                            continue;
                        };
                        let Some(literal) = expr_literal_text(value) else {
                            unrecognized.push((
                                format!("`{name}: {leaf}` value must be a numeric literal"),
                                stmt.source,
                            ));
                            continue;
                        };
                        if name == "initial" {
                            payload.initial.push((leaf, literal));
                        } else {
                            payload.weights.push((leaf, literal));
                        }
                    }
                    "data" => {
                        let Some(value) = default.as_ref() else {
                            unrecognized.push((
                                format!("`data: {leaf}` requires an array literal (`data: <entry> = [<number>, ...]`)"),
                                stmt.source,
                            ));
                            continue;
                        };
                        let Some(literals) = list_literal_texts(value) else {
                            unrecognized.push((
                                format!("`data: {leaf}` value must be an array of numeric literals"),
                                stmt.source,
                            ));
                            continue;
                        };
                        payload.data.push((leaf, literals));
                    }
                    _ => unrecognized.push((
                        format!("field row `{name}` is not a fit row"),
                        stmt.source,
                    )),
                }
            }
            StmtKind::Command { head, argument, .. } => {
                match head.first().map(String::as_str) {
                    Some("model") => {
                        payload.model = fit_command_path(head, argument.as_ref());
                    }
                    Some("prediction") => {
                        payload.prediction = fit_command_path(head, argument.as_ref())
                            .into_iter()
                            .next()
                            .unwrap_or_default();
                    }
                    Some("method") => {
                        payload.method = head
                            .get(1)
                            .cloned()
                            .or_else(|| {
                                fit_command_path(head, argument.as_ref()).into_iter().next()
                            })
                            .unwrap_or_default();
                    }
                    Some("require") => {
                        if head.iter().any(|word| word == "identifiability")
                            && head.iter().any(|word| word == "structural")
                        {
                            payload.require_identifiability = true;
                        } else {
                            unrecognized.push((
                                "`require` in a fit goal must be `require identifiability structural`".into(),
                                stmt.source,
                            ));
                        }
                    }
                    _ => unrecognized.push((
                        format!("command row `{}` is not a fit row", head.join(" ")),
                        stmt.source,
                    )),
                }
            }
            StmtKind::Require(expr) => {
                if let ExprKind::Path { segments, generics: None } = &expr.kind {
                    let text = segments.join(".");
                    if text.contains("identifiability") && text.contains("structural") {
                        payload.require_identifiability = true;
                        continue;
                    }
                }
                unrecognized.push((
                    "`require identifiability.structural` is the only admitted require row in a fit goal".into(),
                    stmt.source,
                ));
            }
            _ => unrecognized.push((
                "only model / prediction / residual / method / initial / weights / require rows are admitted inside a fit goal".into(),
                stmt.source,
            )),
        }
    }
    (payload, unrecognized)
}

/// A command row's path data: the argument path when present, else the
/// remaining head words (`model PK_TwoCompartment`,
/// `prediction [central]`).
fn fit_command_path(head: &[String], argument: Option<&CommandArgument>) -> Vec<String> {
    let mut words = Vec::new();
    match argument {
        Some(CommandArgument::List(items)) => {
            for item in items {
                if let ExprKind::Path { segments, .. } = &item.kind {
                    words.push(segments.join("::"));
                }
            }
        }
        Some(CommandArgument::Expr(expr)) => {
            if let ExprKind::Path { segments, .. } = &expr.kind {
                words.push(segments.join("::"));
            }
        }
        _ => words.extend(head.get(1..).unwrap_or_default().iter().cloned()),
    }
    if words.is_empty() {
        words.extend(head.get(1..).unwrap_or_default().iter().cloned());
    }
    words
}

/// The dotted name of a type expression (`weighted_least_squares`,
/// `k_el`), or empty when the expression is not a path type.
fn type_path_text(ty: &emath_core::tree::TypeExpr) -> String {
    if let emath_core::tree::TypeKind::Path { segments, .. } = &ty.kind {
        segments.join(".")
    } else {
        String::new()
    }
}

/// The literal text of a numeric expression (`0.2`, `2.0`,
/// `1 [unit 1/h]` → `1`), or `None` for any non-literal spelling.
fn expr_literal_text(expr: &emath_core::tree::Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Int(text) | ExprKind::Float(text) => Some(text.clone()),
        ExprKind::Quantity { value, .. } => expr_literal_text(value),
        _ => None,
    }
}

/// Numeric literal texts of an array literal (`[0.5, 1.0, 2.0]`);
/// `None` when the value is not an array or any entry is not a plain
/// numeric literal.
fn list_literal_texts(expr: &emath_core::tree::Expr) -> Option<Vec<String>> {
    let ExprKind::List(items) = &expr.kind else {
        return None;
    };
    let mut texts = Vec::with_capacity(items.len());
    for item in items {
        texts.push(expr_literal_text(item)?);
    }
    Some(texts)
}
/// Parse through the installed source-parser backend; `E-SYN-120` when none
/// is installed (wire `emath_syntax::install_source_parser` at startup).
fn parse_through(
    text: &str,
    limits: &Limits,
    edition: emath_core::Edition,
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
    Ok(parser.parse(text, FileId(0), limits, edition))
}

fn nearest_manifest(source: &Path) -> Option<std::path::PathBuf> {
    let mut directory = source.parent();
    while let Some(current) = directory {
        let candidate = current.join("emath.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        directory = current.parent();
    }
    None
}

fn edition_from_manifest(manifest: &str) -> Result<emath_core::Edition, String> {
    for line in manifest.lines() {
        let content = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = content.split_once('=') else {
            continue;
        };
        if key.trim() != "edition" {
            continue;
        }
        let value = value.trim();
        if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
            return emath_core::Edition::from_manifest_str(&value[1..value.len() - 1])
                .map_err(|error| error.to_string());
        }
        return Err(format!(
            "{}: edition must be a quoted string",
            emath_core::E_PKG_EDITION_UNKNOWN
        ));
    }
    Err("E-PKG-EDITION-MISSING: emath.toml requires `edition = \"2026\"`".to_string())
}

/// The module name of an in-package file import: `use <package>.<module>`
/// where `<package>` matches the importing file's own `package` path
/// (one extra segment; the dot is a path separator to the lexer).
/// Library paths (`std.numeric.Real`), curated law packages
/// (`physics::classical`), and unprefixed paths do not match.
fn file_import_module<'a>(path: &'a [String], package_path: Option<&[String]>) -> Option<&'a str> {
    let package_path = package_path?;
    if path.len() == package_path.len() + 1 && path.starts_with(package_path) {
        path.last().map(String::as_str)
    } else {
        None
    }
}
