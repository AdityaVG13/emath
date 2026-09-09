//! Builder model attach-by-id: tests and goals surface on the declaration.
use emath_build::builder::{BuilderModel, Expression, GoalModel, ModelBuilder, TestModel, TypeKind};
use emath_test_harness::Probe;

fn counter() -> BuilderModel {
    BuilderModel::custom("Counter")
        .input("x", TypeKind::Float64)
        .output("y", TypeKind::Float64)
        .define("y", Expression::Symbol("x".to_string()))
        .goal(GoalModel { kind: "evaluate".to_string(), target: "y".to_string(), produce: "rust.library".to_string() })
}

#[test]
fn probe() {
    let mut p = Probe::new("builder tests and goals attach by id to the declaration");
    p.case("tests", |p| {
        let package = counter().test(TestModel { name: "demo".into(), given: vec![("x".into(), Expression::Float(1.0))], expect: Expression::Symbol("x".into()) }).build().expect("must lower");
        let decl = package.declarations.first().expect("one declaration");
        p.eq("count", decl.tests.len(), 1);
        match package.tests.get(decl.tests[0].index()) {
            None => {
                p.fail("resolve", "test id must resolve");
            }
            Some(t) => {
                p.eq("name", t.name.clone(), "demo".to_string());
                p.eq("given", t.given.len(), 1);
            }
        }
    });
    p.case("goals", |p| {
        let package = counter().build().expect("must lower");
        let decl = package.declarations.first().expect("one declaration");
        p.eq("count", decl.goals.len(), 1);
        match package.goals.get(decl.goals[0].index()) {
            None => {
                p.fail("resolve", "goal id must resolve");
            }
            Some(g) => {
                p.eq("target", g.target.clone(), "y".to_string());
                p.eq("kind", g.kind.as_str(), "evaluate");
            }
        }
    });
    p.finish();
}
