#![forbid(unsafe_code)]
//! Negative tests: programmatic test lowering returns BuilderError on
//! malformed given/expect expressions instead of panicking on the public
//! ModelBuilder::build path (bug-hunt residual).
//!
//! e3wv enrichment (F041): the negatives pin the TYPED error text, not
//! just `is_err` — a wrong-payload error or an empty message must fail
//! these tests.

use emath_builder::{BuilderError, BuilderModel, Expression, ModelBuilder, TestModel};

#[test]
fn bad_test_given_returns_error_not_panic() {
    let model = BuilderModel::custom("f").test(TestModel {
        name: "bad".into(),
        // `x` is referenced in `given` BEFORE it is bound (given env is
        // built in order) — the lowered error names the symbol.
        given: vec![("x".into(), Expression::Symbol("x".into()))],
        expect: Expression::Float(1.0),
    });
    let error = model.build().expect_err("self-referential given refuses");
    let BuilderError(message) = &error;
    assert!(
        message.contains("unknown symbol `x`"),
        "given-negative must name the unknown symbol, got: {message}"
    );
}

#[test]
fn bad_test_expect_returns_error_not_panic() {
    let model = BuilderModel::custom("f").test(TestModel {
        name: "bad".into(),
        given: vec![("x".into(), Expression::Float(1.0))],
        expect: Expression::Symbol("nope".into()),
    });
    let error = model.build().expect_err("unknown expect symbol refuses");
    let BuilderError(message) = &error;
    assert!(
        message.contains("unknown symbol `nope`"),
        "expect-negative must name the unknown symbol, got: {message}"
    );
}

#[test]
fn well_formed_test_still_builds() {
    let model = BuilderModel::custom("f").test(TestModel {
        name: "ok".into(),
        given: vec![("x".into(), Expression::Float(1.0))],
        expect: Expression::Symbol("x".into()),
    });
    let package = model.build().expect("well-formed test builds");
    // The payload is real: the package carries the ONE test case with
    // its expect expression present.
    let declaration = package
        .declarations
        .first()
        .expect("builder lowers one declaration");
    assert_eq!(declaration.tests.len(), 1, "the test case lowers into the package");
    let test = package
        .tests
        .get(declaration.tests[0].index())
        .expect("declaration test id must resolve into package.tests");
    assert!(
        test.expect.is_some(),
        "an `expect`-carrying test must lower with its expect expression"
    );
}
