//! Negative tests: the programmatic model/goal builder surface is
//! pruned constructor-side — `build()` always refuses `E-KIND-GONE`
//! with the teaching message (write an ordinary `emath function` or
//! `emath query`). The pre-cutover symbol-validation lowering is
//! unreachable behind that refusal.

use emath_build::builder::{BuilderError, BuilderModel, Expression, ModelBuilder, TestModel};
use emath_test_harness::{Probe, boot};

#[test]
fn probe() {
    boot();
    let mut p = Probe::new(
        "programmatic model/goal builders refuse E-KIND-GONE: constructor surface only",
    );
    p.case("malformed-given-refuses-e-kind-gone", |p| {
        let model = BuilderModel::custom("f").test(TestModel {
            name: "bad".into(),
            given: vec![("x".into(), Expression::Symbol("x".into()))],
            expect: Expression::Float(1.0),
        });
        match model.build() {
            Ok(_) => p.fail("given", "programmatic build must refuse"),
            Err(BuilderError(m)) => {
                p.contains("code", &m, "E-KIND-GONE");
                p.contains("teach", &m, "emath function")
            }
        };
    });
    p.case("well-formed-also-refuses", |p| {
        let model = BuilderModel::custom("f").test(TestModel {
            name: "ok".into(),
            given: vec![("x".into(), Expression::Float(1.0))],
            expect: Expression::Symbol("x".into()),
        });
        match model.build() {
            Ok(_) => p.fail(
                "well-formed",
                "no programmatic model builds: constructor surface only",
            ),
            Err(BuilderError(m)) => p.contains("code", &m, "E-KIND-GONE"),
        };
    });
    p.finish();
}
