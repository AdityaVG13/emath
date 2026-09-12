//! Programmatic model/goal builders are leftover, not constructor surface.
use emath_build::builder::{BuilderModel, Expression, GoalModel, ModelBuilder, TypeKind};
use emath_test_harness::Probe;

fn counter() -> BuilderModel {
    BuilderModel::custom("Counter")
        .input("x", TypeKind::Float64)
        .output("y", TypeKind::Float64)
        .define("y", Expression::Symbol("x".to_string()))
        .goal(GoalModel {
            kind: "evaluate".to_string(),
            target: "y".to_string(),
            produce: "rust.library".to_string(),
        })
}

#[test]
fn probe() {
    let mut p = Probe::new("programmatic model/goal builders refuse E-KIND-GONE");
    p.case("build-gone", |p| {
        match counter().build() {
            Err(error) => {
                p.contains("gone", &error.0, "E-KIND-GONE");
            }
            Ok(package) => {
                p.fail(
                    "builder",
                    format!(
                        "goal builder must refuse, got {} declarations",
                        package.declarations.len()
                    ),
                );
            }
        }
    });
    p.finish();
}
