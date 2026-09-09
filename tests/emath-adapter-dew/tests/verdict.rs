//! Shared verdict vocabulary + proven resolution of DISC-DEW-INT-EXACT.
use std::collections::BTreeMap;
use emath_adapter_dew::{EvalValue, evaluate_scalar, map_expression};
use emath_adapter_dew_tests::TestResult;
use emath_core::Span;
use emath_exec_ir::lower_definition;
use emath_ir::{ExprNode, Literal, SemanticPackage};
use emath_test_harness::{Case, Probe, check_all};

/// Discrepancy pin closed by the exact-finite-f64 admission gate. Registrar: .
const DISC_DEW_INT_EXACT: &str = "DISC-DEW-INT-EXACT";

fn int_verdict(text: &str) -> TestResult {
    let mut package = SemanticPackage::new();
    let expr = package.push_expr(ExprNode::Literal(Literal::Integer(text.to_string())), Span::default());
    let Ok(dew) = map_expression(&package, expr) else { return TestResult::Pass };
    let Some(EvalValue::F64(value)) = evaluate_scalar(&dew, &BTreeMap::new()) else { return TestResult::Fail };
    match text.replace('_', "").parse::<i128>() {
        Ok(exact) if value as i128 == exact => TestResult::Pass,
        _ => TestResult::Fail,
    }
}

#[test]
fn probe() {
    let mut p = Probe::new("dew integer boundary refuses lossy literals with E-PROV-030 instead of rounding");
    if let Err(m) = check_all(
        &[
            Case::new("42", "42", TestResult::Pass),
            Case::new("2^53", "9007199254740992", TestResult::Pass),
            Case::new("neg-2^53", "-9007199254740992", TestResult::Pass),
            Case::new("sep", "1_000", TestResult::Pass),
            Case::new("2^53+1", "9007199254740993", TestResult::Pass),
        ],
        |s| int_verdict(s),
    ) {
        p.fail("int-verdict", m);
    } else {
        p.demand("int-verdict", true, "ok");
    }
    p.case("lossy-refusal", |p| {
        let mut package = SemanticPackage::new();
        let expr = package.push_expr(ExprNode::Literal(Literal::Integer("9007199254740993".into())), Span::default());
        match map_expression(&package, expr) {
            Ok(_) => {
                p.fail("lossy", "must refuse 2^53+1");
            }
            Err(issue) => {
                p.eq("lossy/code", issue.code, "E-PROV-030");
                p.contains(DISC_DEW_INT_EXACT, &issue.detail, "9007199254740993");
            }
        }
    });
    p.case("non-finite-refusal", |p| {
        let huge = format!("1{}", "0".repeat(400));
        p.eq("huge-verdict", int_verdict(&huge), TestResult::Pass);
        let mut package = SemanticPackage::new();
        let expr = package.push_expr(ExprNode::Literal(Literal::Integer(huge.clone())), Span::default());
        match map_expression(&package, expr) {
            Ok(_) => {
                p.fail("huge", "must refuse");
            }
            Err(issue) => {
                p.eq("huge/code", issue.code, "E-PROV-030");
            }
        }
        let mut native = SemanticPackage::new();
        let nexpr = native.push_expr(ExprNode::Literal(Literal::Integer(huge)), Span::default());
        p.demand("native-refuses", lower_definition(&native, nexpr, &[], &[]).is_err(), "native must refuse the same literal");
    });
    p.finish();
}
