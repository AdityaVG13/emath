//! `simplify` goal elaboration executes the native symbolic slice.

use emath_core::limits::Limits;
use emath_ir::{ExprNode, GoalKind};
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

#[test]
fn simplify_goal_returns_native_symbolic_expression() {
    install_source_parser();
    let source = include_str!("../../../language/examples/algebra/symbolic-cas.emath");
    let mut session = CompilerSession::new(Limits::default());
    let file = session.load_text("symbolic-cas", source);
    let checked = session.plan(file);
    assert!(
        !checked.diagnostics.has_errors(),
        "{:?}",
        checked.diagnostics.errors().collect::<Vec<_>>()
    );
    let goal = checked
        .package
        .goals
        .iter()
        .find(|goal| goal.kind == GoalKind::Simplify)
        .unwrap();
    let expression = checked.package.expr(goal.expression.unwrap());
    assert!(
        matches!(expression, Some(ExprNode::Variable(_))),
        "{expression:?}"
    );
    assert!(
        checked
            .plans
            .iter()
            .any(|plan| plan.goal == goal.id && plan.artifact_class == "native-symbolic")
    );
}

#[test]
fn simplify_goal_refuses_non_exact_domain() {
    install_source_parser();
    let source = "\
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
";
    let mut session = CompilerSession::new(Limits::default());
    let file = session.load_text("general-real-claim", source);
    let planned = session.plan(file);
    assert!(
        planned
            .diagnostics
            .errors()
            .any(|error| error.code == "E-SYM-003")
    );
    assert!(
        planned
            .plans
            .iter()
            .all(|plan| plan.artifact_class != "native-symbolic")
    );
}

#[test]
fn simplify_goal_stays_attached_to_its_declaration() {
    install_source_parser();
    let source = "\
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
";
    let mut session = CompilerSession::new(Limits::default());
    let file = session.load_text("owned-symbolic-goal", source);
    let planned = session.plan(file);
    assert!(!planned.diagnostics.has_errors());
    let goal = planned
        .package
        .goals
        .iter()
        .find(|goal| goal.kind == GoalKind::Simplify)
        .unwrap();
    assert!(matches!(
        planned.package.expr(goal.expression.unwrap()),
        Some(ExprNode::Variable(name)) if name.leaf() == "y"
    ));
}
