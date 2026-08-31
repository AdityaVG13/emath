//! Shared verdict vocabulary + proven resolution of DISC-DEW-INT-EXACT.
//!
//! Thin slice of `emath-conform-harness-thin-lfpg`: the Dew adapter
//! reports conformance through `TestResult` (shared via the crate lib
//! target). The integer-literal discrepancy is RESOLVED by a typed
//! `E-PROV-030` refusal at the exact-finite-f64 boundary; these tests
//! prove the resolution — silent rounding or `inf` evaluation is a
//! hard Fail, never an ExpectedFailure. The resolved behavior stays a
//! strict Pass/refusal contract.

use std::collections::BTreeMap;

use emath_adapter_dew::{EvalValue, evaluate_scalar, map_expression};
use emath_adapter_dew_tests::TestResult;
use emath_core::Span;
use emath_exec_ir::lower_definition;
use emath_ir::{ExprNode, Literal, SemanticPackage};

/// Discrepancy pin closed by the exact-finite-f64 admission gate:
/// `map_expression` used to admit every integer literal and
/// `evaluate_scalar` parsed them lossily (2^53+1 silently rounded;
/// non-finite digit strings evaluated to `inf` while the native
/// emitter refuses). Kept here as provenance id only; any regression
/// to silent rounding must be a hard Fail. Registrar:
/// `emath-conform-pin-register-1iip`.
const DISC_DEW_INT_EXACT: &str = "DISC-DEW-INT-EXACT";

/// Verdict for one Dew integer literal.
///
/// - `map_expression` refusal with `E-PROV-030` (not exactly
///   representable as finite f64): Pass — honest refusal, never a
///   silent lossy parse.
/// - Successful map and evaluation equal to the exact value: Pass.
/// - Anything else: Fail. The pin is closed; a lossy evaluation is a
///   regression, never an expected failure.
fn int_verdict(text: &str) -> TestResult {
    let mut package = SemanticPackage::new();
    let expr = package.push_expr(
        ExprNode::Literal(Literal::Integer(text.to_string())),
        Span::default(),
    );
    let mapped = map_expression(&package, expr);
    let Ok(dew) = mapped else {
        return TestResult::Pass;
    };
    let env = BTreeMap::new();
    let Some(EvalValue::F64(value)) = evaluate_scalar(&dew, &env) else {
        return TestResult::Fail;
    };
    match text.replace('_', "").parse::<i128>() {
        Ok(exact) if value as i128 == exact => TestResult::Pass,
        _ => TestResult::Fail,
    }
}

/// Literals inside the exact-f64 boundary map and evaluate to their
/// exact value, including the 2^53 edge and digit separators.
#[test]
fn exactly_representable_integer_is_pass() {
    assert_eq!(int_verdict("42"), TestResult::Pass);
    assert_eq!(int_verdict("9007199254740992"), TestResult::Pass);
    assert_eq!(int_verdict("-9007199254740992"), TestResult::Pass);
    assert_eq!(int_verdict("1_000"), TestResult::Pass);
}

/// 2^53 + 1 no longer rounds silently: the adapter refuses it with a
/// typed `E-PROV-030` mapping issue at the exact-finite-f64 boundary.
#[test]
fn lossy_integer_is_refused_not_expected_failure() {
    assert_eq!(int_verdict("9007199254740993"), TestResult::Pass);

    let mut package = SemanticPackage::new();
    let expr = package.push_expr(
        ExprNode::Literal(Literal::Integer("9007199254740993".into())),
        Span::default(),
    );
    let issue = map_expression(&package, expr).expect_err("must refuse the literal");
    assert_eq!(issue.code, "E-PROV-030");
    assert!(
        issue.detail.contains("9007199254740993"),
        "{DISC_DEW_INT_EXACT}: refusal must name the refused literal, got {}",
        issue.detail
    );
}

/// Non-finite digit strings are refused by both paths with the same
/// boundary; the Dew refusal is typed, matching the native refusal.
#[test]
fn non_finite_integer_refused_like_native() {
    let huge = format!("1{}", "0".repeat(400));
    assert_eq!(int_verdict(&huge), TestResult::Pass);

    let mut package = SemanticPackage::new();
    let expr = package.push_expr(
        ExprNode::Literal(Literal::Integer(huge.clone())),
        Span::default(),
    );
    let issue = map_expression(&package, expr).expect_err("must refuse the literal");
    assert_eq!(issue.code, "E-PROV-030");

    let mut native_package = SemanticPackage::new();
    let native_expr = native_package.push_expr(
        ExprNode::Literal(Literal::Integer(huge)),
        Span::default(),
    );
    assert!(
        lower_definition(&native_package, native_expr, &[], &[]).is_err(),
        "native emitter must refuse the same literal for the boundary to match"
    );
}
