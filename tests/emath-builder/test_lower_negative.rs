#![forbid(unsafe_code)]
//! Negative tests: programmatic test lowering returns BuilderError on
//! malformed given/expect expressions instead of panicking on the public
//! ModelBuilder::build path (bug-hunt residual).

use emath_builder::{BuilderModel, Expression, ModelBuilder, TestModel};

#[test]
fn bad_test_given_returns_error_not_panic() {
    let model = BuilderModel::custom("f").test(TestModel {
        name: "bad".into(),
        given: vec![("x".into(), Expression::Symbol("x".into()))],
        expect: Expression::Float(1.0),
    });
    assert!(model.build().is_err());
}

#[test]
fn bad_test_expect_returns_error_not_panic() {
    let model = BuilderModel::custom("f").test(TestModel {
        name: "bad".into(),
        given: vec![("x".into(), Expression::Float(1.0))],
        expect: Expression::Symbol("nope".into()),
    });
    assert!(model.build().is_err());
}

#[test]
fn well_formed_test_still_builds() {
    let model = BuilderModel::custom("f").test(TestModel {
        name: "ok".into(),
        given: vec![("x".into(), Expression::Float(1.0))],
        expect: Expression::Symbol("x".into()),
    });
    assert!(model.build().is_ok());
}
