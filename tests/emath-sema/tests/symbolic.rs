//! `simplify` goal elaboration executes the native symbolic slice.

use emath_core::limits::Limits;
use emath_ir::{ExprNode, GoalKind};
use emath_sema::CompilerSession;
use emath_test_harness::{boot, Probe, Source};

fn plan_of(name: &str, text: &str) -> emath_sema::session::PlanResult {
    let mut session = CompilerSession::new(Limits::default());
    let file = session.load_text(name, text);
    session.plan(file)
}

#[test]
fn symbolic_contract() {
    boot();
    let mut p = Probe::new("simplify goals elaborate to native-symbolic plans");
    p.case("native-expression", |p| {
        let text = Source::from_workspace("language/examples/algebra/symbolic-cas.emath");
        let planned = plan_of("symbolic-cas", &text.text());
        let errors: Vec<&str> = planned.diagnostics.errors().map(|e| e.code).collect();
        p.demand("admits", errors.is_empty(), format!("must admit, got {errors:?}"));
        match planned.package.goals.iter().find(|g| g.kind == GoalKind::Simplify) {
            Some(goal) => {
                let expression = goal.expression.and_then(|id| planned.package.expr(id));
                p.demand(
                    "variable-form",
                    matches!(expression, Some(ExprNode::Variable(_))),
                    format!("simplify must return a native symbolic variable, got {expression:?}"),
                );
                p.demand(
                    "native-plan",
                    planned.plans.iter().any(|plan| plan.goal == goal.id && plan.artifact_class == "native-symbolic"),
                    "simplify goal needs a native-symbolic plan".to_string(),
                );
            }
            None => {
                p.fail("goal", "a Simplify goal must exist".to_string());
            }
        }
    });
    p.case("non-exact-refuses", |p| {
        let planned = plan_of(
            "general-real-claim",
            "\
emath function GeneralRealClaim:
    inputs:
        x: Float64
    outputs:
        value: Float64
    definitions:
        value = sin(x)
    goals:
        simplify <value>:
            require exact
",
        );
        let codes: Vec<&str> = planned.diagnostics.errors().map(|e| e.code).collect();
        p.demand("E-SYM-003", codes.contains(&"E-SYM-003"), format!("got {codes:?}"));
        p.demand(
            "no-native-plan",
            planned.plans.iter().all(|plan| plan.artifact_class != "native-symbolic"),
            "refused goal must not carry a native-symbolic plan".to_string(),
        );
    });
    p.case("goal-attachment", |p| {
        let planned = plan_of(
            "owned-symbolic-goal",
            "\
emath function First:
    inputs:
        x: Int
    outputs:
        value: Int
    definitions:
        value = x
emath function Second:
    inputs:
        y: Int
    outputs:
        value: Int
    definitions:
        value = y * 1
    goals:
        simplify <value>:
            require exact
",
        );
        let errors: Vec<&str> = planned.diagnostics.errors().map(|e| e.code).collect();
        p.demand("admits", errors.is_empty(), format!("must admit, got {errors:?}"));
        match planned.package.goals.iter().find(|g| g.kind == GoalKind::Simplify) {
            Some(goal) => {
                let expression = goal.expression.and_then(|id| planned.package.expr(id));
                p.demand(
                    "attached-to-second",
                    matches!(expression, Some(ExprNode::Variable(name)) if name.leaf() == "y"),
                    format!("simplify must stay attached to Second's y, got {expression:?}"),
                );
            }
            None => {
                p.fail("goal", "a Simplify goal must exist".to_string());
            }
        }
    });
    p.finish();
}
