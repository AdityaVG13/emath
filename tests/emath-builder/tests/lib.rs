mod builder {
    use emath_builder::{BuilderModel, Expression, GoalModel, ModelBuilder, TestModel, TypeKind};

    #[test]
    fn builder_model_tests_surface_on_declaration_tests() {
        // Attach-by-id repair (l2pb.4): a builder model's `tests:` must
        // surface on `declaration.tests`, the same attachment the admit
        // lane uses, so identity and generated `#[test]` functions see
        // them. A span-based fallback would drop them (builder spans are
        // all `OWNER`).
        let package = BuilderModel::custom("Counter")
            .input("x", TypeKind::Float64)
            .output("y", TypeKind::Float64)
            .define("y", Expression::Symbol("x".to_string()))
            .goal(GoalModel {
                kind: "evaluate".to_string(),
                target: "y".to_string(),
                produce: "rust.library".to_string(),
            })
            .test(TestModel {
                name: "demo".to_string(),
                given: vec![("x".to_string(), Expression::Float(1.0))],
                expect: Expression::Symbol("x".to_string()),
            })
            .build()
            .expect("function builder model must lower");
        let declaration = package
            .declarations
            .first()
            .expect("builder lowers one declaration");
        assert_eq!(
            declaration.tests.len(),
            1,
            "declaration.tests must carry the builder model's tests"
        );
        let test = package
            .tests
            .get(declaration.tests[0].index())
            .expect("declaration test id must resolve into package.tests");
        assert_eq!(test.name, "demo");
        assert_eq!(test.given.len(), 1);
    }

    #[test]
    fn builder_model_goals_surface_on_declaration_goals() {
        // The same attach-by-id repair for goals: the evaluate goal the
        // model declares must be reachable from the declaration.
        let package = BuilderModel::custom("Counter")
            .input("x", TypeKind::Float64)
            .output("y", TypeKind::Float64)
            .define("y", Expression::Symbol("x".to_string()))
            .goal(GoalModel {
                kind: "evaluate".to_string(),
                target: "y".to_string(),
                produce: "rust.library".to_string(),
            })
            .build()
            .expect("function builder model must lower");
        let declaration = package
            .declarations
            .first()
            .expect("builder lowers one declaration");
        assert_eq!(declaration.goals.len(), 1);
        let goal = package
            .goals
            .get(declaration.goals[0].index())
            .expect("declaration goal id must resolve into package.goals");
        assert_eq!(goal.target, "y");
        assert_eq!(goal.kind.as_str(), "evaluate");
    }
}

// e3wv (F041): the malformed-given/expect negatives live in
// `test_lower_negative.rs` (same crate, shared target via this module
// include — a standalone file with no `[[test]]` entry never compiled).
mod test_lower_negative;
