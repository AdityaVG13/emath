//! Goal elaboration: `.emath` requests and compile sections to Goal IR.
//!
//! Phase 1 supports `evaluate <output>: produce rust.library` plus the
//! `compile:` section. Other request kinds receive typed capability
//! refusals (`E-GOAL-043`) so nothing is silently dropped.

#![forbid(unsafe_code)]

use emath_core::{Diagnostics, Span};
use emath_ir::{
    DeterminismPolicy, EvidenceLevel, ExactnessPolicy, FallbackPolicy, Goal, GoalKind,
    GoalRequirements, SemanticPackage, TargetProfile,
};
use emath_syntax::tree::{CommandArgument, ExprKind, Section, StmtKind};

pub mod schema;
pub use schema::{
    budget_token, custom_token, exactness_token, fallback_token, target_token, BudgetConstraint,
    GoalKindSpec, GoalSchema, GoalSchemaProblem,
};

pub const PRODUCE_RUST_LIBRARY: &str = "rust.library";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestSpec {
    pub kind: String,
    pub target: String,
    pub produce: String,
    pub source: Span,
}

/// Extract the `requests:` section into request specs and validate targets
/// against the admitted declaration (`E-GOAL-041`/`E-GOAL-042`/`E-GOAL-043`).
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

/// Build GIR for one request into the package arena.
pub fn build_goal(package: &mut SemanticPackage, request: &RequestSpec) -> Goal {
    let target_expr = package
        .declarations
        .iter()
        .find(|d| d.definitions.contains_key(&request.target))
        .and_then(|d| d.definitions.get(&request.target))
        .copied();
    let id = emath_ir::GoalId(u32::try_from(package.goals.len()).unwrap_or(u32::MAX));
    let goal = Goal {
        id,
        kind: GoalKind::Evaluate,
        target: request.target.clone(),
        expression: target_expr,
        requirements: GoalRequirements {
            evidence: EvidenceLevel::E1,
            exactness: ExactnessPolicy::Exact,
            determinism: DeterminismPolicy::Required,
            target: TargetProfile {
                family: "rust-library".into(),
                triple: None,
                features: vec![],
            },
            fallback: FallbackPolicy::NativeOnly,
            produce: if request.produce.is_empty() {
                PRODUCE_RUST_LIBRARY.to_string()
            } else {
                request.produce.clone()
            },
        },
        source: request.source,
    };
    package.push_goal(goal.clone());
    goal
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_ir::{NumericProfile, SafetyProfile};
    use emath_syntax::parse_str;

    fn compile_spec() -> emath_ir::CompileSpec {
        emath_ir::CompileSpec {
            target: "rust".into(),
            profile: "library".into(),
            numeric: NumericProfile::StrictF64,
            safety: SafetyProfile::ForbidUnsafe,
            unresolved: None,
        }
    }

    #[test]
    fn unknown_request_kind_is_refused_not_dropped() {
        let source = r"emath custom <X> as function:
    outputs:
        y: Float64
    definitions:
        y = 1
    requests:
        solve <y>:
            produce rust.library
";
        let (tree, _) = parse_str(source);
        let decl = &tree.items[0];
        let emath_syntax::tree::Item::Declaration(decl) = decl else {
            panic!()
        };
        let mut diagnostics = Diagnostics::new();
        let package = SemanticPackage::new();
        let requests = elaborate_requests(&package, "X", &decl.sections, &mut diagnostics);
        assert!(requests.is_empty());
        assert!(diagnostics.items().iter().any(|d| d.code == "E-GOAL-043"));
        let _ = compile_spec();
    }
}
