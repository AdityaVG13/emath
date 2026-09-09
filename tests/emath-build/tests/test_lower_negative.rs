//! Negative tests: programmatic test lowering returns BuilderError on malformed given/expect.
use emath_build::builder::{BuilderError, BuilderModel, Expression, ModelBuilder, TestModel};
use emath_test_harness::Probe;

#[test]
fn probe() {
    let mut p = Probe::new("test lowering refuses malformed given/expect with typed symbol errors");
    p.case("bad-given", |p| {
        let model = BuilderModel::custom("f").test(TestModel { name: "bad".into(), given: vec![("x".into(), Expression::Symbol("x".into()))], expect: Expression::Float(1.0) });
        match model.build() {
            Ok(_) => p.fail("given", "self-referential given must refuse"),
            Err(BuilderError(m)) => p.contains("given/symbol", &m, "unknown symbol `x`"),
        };
    });
    p.case("bad-expect", |p| {
        let model = BuilderModel::custom("f").test(TestModel { name: "bad".into(), given: vec![("x".into(), Expression::Float(1.0))], expect: Expression::Symbol("nope".into()) });
        match model.build() {
            Ok(_) => p.fail("expect", "unknown expect must refuse"),
            Err(BuilderError(m)) => p.contains("expect/symbol", &m, "unknown symbol `nope`"),
        };
    });
    p.case("well-formed", |p| {
        let package = BuilderModel::custom("f").test(TestModel { name: "ok".into(), given: vec![("x".into(), Expression::Float(1.0))], expect: Expression::Symbol("x".into()) }).build().expect("must build");
        let decl = package.declarations.first().expect("one declaration");
        p.eq("count", decl.tests.len(), 1);
        match package.tests.get(decl.tests[0].index()) {
            None => p.fail("resolve", "must resolve"),
            Some(t) => p.demand("expect", t.expect.is_some(), "must carry expect"),
        };
    });
    p.finish();
}
