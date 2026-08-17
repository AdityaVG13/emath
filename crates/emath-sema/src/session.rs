//! Compiler session: load → check → plan. The build step (backend +
//! artifact emission) lives in `emath-build`.

use crate::admit::{CheckResult, check_tree};
use emath_core::parse::source_parser;
use emath_core::tree::{CommandArgument, ExprKind, Section, StmtKind};
use emath_core::{Diagnostics, FileId, SourceStore, Span, limits::Limits};
use emath_ir::{GoalId, RequestSpec, SemanticPackage, build_goal, native_plan};
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_session_token_budget_refuses_parse() {
        // Session limits must reach the lexer through the parser backend:
        // a tiny token budget refuses a larger source (E-SYN-108) instead
        // of parsing with `Limits::default()`.
        emath_syntax::install_source_parser();
        let mut session = CompilerSession::new(Limits {
            max_tokens: 8,
            max_source_bytes: 1 << 20,
            max_nesting: 8,
        });
        let result = session.check_owned("token-heavy", "def f(x) = x + y + z");
        assert!(
            result
                .diagnostics
                .errors()
                .any(|diagnostic| diagnostic.code == "E-SYN-108"),
            "tiny max_tokens must refuse with E-SYN-108, got {:?}",
            result
                .diagnostics
                .errors()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>()
        );
    }

    fn function_decl(name: &str, definitions: &[&str]) -> String {
        let mut text = format!(
            "emath function {name}:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n"
        );
        text.push_str("    definitions:\n");
        for definition in definitions {
            text.push_str("        ");
            text.push_str(definition);
            text.push('\n');
        }
        text
    }

    #[test]
    fn duplicate_declaration_name_is_refused_with_e_name_022() {
        // Two declarations with the same name would collide in generated
        // Rust; the second is a typed refusal (E-NAME-022), never a
        // silent overwrite.
        emath_syntax::install_source_parser();
        let mut session = CompilerSession::new(Limits::default());
        let mut text = function_decl("Left", &["y = x"]);
        text.push_str(&function_decl("Left", &["y = x * 2"]));
        let result = session.check_owned("dup", &text);
        assert!(
            result
                .diagnostics
                .errors()
                .any(|diagnostic| diagnostic.code == "E-NAME-022"),
            "duplicate declaration names must refuse with E-NAME-022"
        );
    }

    #[test]
    fn underscore_declaration_name_is_refused_with_e_name_023() {
        // `_` cannot be escaped into a Rust type name; the declaration is
        // refused up front (E-NAME-023).
        emath_syntax::install_source_parser();
        let mut session = CompilerSession::new(Limits::default());
        let result = session.check_owned("underscore", &function_decl("_", &["y = x"]));
        assert!(
            result
                .diagnostics
                .errors()
                .any(|diagnostic| diagnostic.code == "E-NAME-023"),
            "a declaration named `_` must refuse with E-NAME-023"
        );
    }

    #[test]
    fn confusable_lookalike_declaration_is_refused_with_e_name_024() {
        // Two public declarations distinguishable only by lookalike
        // glyphs — Latin `a` vs Cyrillic `а` (U+0430) — are refused with
        // E-NAME-024: the generated API would expose two visually
        // identical names. Order-independent: whichever spelling arrives
        // second collides with the first.
        emath_syntax::install_source_parser();
        let mut session = CompilerSession::new(Limits::default());
        let mut latin_then_cyrillic = function_decl("magnitude", &["y = x"]);
        latin_then_cyrillic.push_str(&function_decl("m\u{0430}gnitude", &["y = x"]));
        let forward = session.check_owned("confusable-forward", &latin_then_cyrillic);
        assert!(
            forward
                .diagnostics
                .errors()
                .any(|diagnostic| diagnostic.code == "E-NAME-024"),
            "Cyrillic lookalike after a Latin name must refuse with E-NAME-024"
        );

        let mut cyrillic_then_latin = function_decl("m\u{0430}gnitude", &["y = x"]);
        cyrillic_then_latin.push_str(&function_decl("magnitude", &["y = x"]));
        let backward = session.check_owned("confusable-backward", &cyrillic_then_latin);
        assert!(
            backward
                .diagnostics
                .errors()
                .any(|diagnostic| diagnostic.code == "E-NAME-024"),
            "Latin lookalike after a Cyrillic name must refuse with E-NAME-024"
        );
    }

    #[test]
    fn names_that_are_not_lookalikes_are_not_refused() {
        // The confusable lint must not reject names that merely share a
        // prefix: `magnitude` and `magnitude2` fold apart and both admit,
        // with no E-NAME-024 (and no E-NAME-022) on either pass.
        emath_syntax::install_source_parser();
        let mut session = CompilerSession::new(Limits::default());
        let mut text = function_decl("magnitude", &["y = x"]);
        text.push_str(&function_decl("magnitude2", &["y = x"]));
        let result = session.check_owned("distinct", &text);
        assert!(
            !result
                .diagnostics
                .errors()
                .any(|diagnostic| diagnostic.code == "E-NAME-024"),
            "distinct spellings must not be refused as confusable"
        );
        assert_eq!(
            result.package.declarations.len(),
            2,
            "both distinct declarations must admit"
        );
    }

    #[test]
    fn goals_attach_to_their_own_declaration_by_id_not_span() {
        // Attach-by-id repair: goals elaborate per declaration and attach
        // by the ids built for that declaration, never by span geometry.
        // Here the first declaration owns three default goals and the
        // second owns one explicit goal; a span-based attach would pile
        // both declarations' goals onto whichever span covered them.
        emath_syntax::install_source_parser();
        let mut session = CompilerSession::new(Limits::default());
        // Definitions reference inputs only (chained definitions are
        // outside the Phase 1 admission subset), so all three lower.
        let mut text = function_decl("Left", &["y = x", "y2 = x", "y3 = x"]);
        text.push_str(
            "emath function Right:\n    inputs:\n        a: Float64\n    outputs:\n        b: Float64\n    definitions:\n        b = a\n    goals:\n        evaluate <b>:\n            produce rust.library\n",
        );
        let file = session.load_text("two-decls", text);
        let plan = session.plan(file);
        assert_eq!(plan.package.declarations.len(), 2);
        let left = &plan.package.declarations[0];
        let right = &plan.package.declarations[1];
        assert_eq!(
            left.goals.len(),
            3,
            "Left's three definitions must elaborate into three goals"
        );
        assert_eq!(
            right.goals.len(),
            1,
            "Right's explicit goals: section must elaborate into one goal"
        );
        let right_goal = plan
            .package
            .goals
            .get(right.goals[0].index())
            .expect("Right's goal id must resolve");
        assert_eq!(right_goal.target, "b");
        // Every goal attached to a declaration sits inside that
        // declaration's own source span too — the geometric property the
        // old span filter approximated.
        for (declaration, goal_ids) in [
            (&plan.package.declarations[0], left.goals.as_slice()),
            (&plan.package.declarations[1], right.goals.as_slice()),
        ] {
            for goal_id in goal_ids {
                let goal = plan
                    .package
                    .goals
                    .get(goal_id.index())
                    .expect("attached goal id must resolve");
                assert!(
                    declaration.source.contains(goal.source.start),
                    "goal {} (start {}) must lie inside declaration `{}` span {:?}",
                    goal.target,
                    goal.source.start,
                    declaration.name.leaf(),
                    declaration.source,
                );
            }
        }
    }
}
