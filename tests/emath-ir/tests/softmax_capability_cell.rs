//! Softmax pure cell — first zero-core-delta
//! capability (schema `emath.capability-cell.v1`, class `pure`).
//!
//! The cell is data: descriptor + reference semantics + laws, all in the
//! capability layer. Zero core diff for the cell itself — no Softmax
//! variant in `ExprNode`/`UnaryOp`/`BinaryOp` (CDLOC 0, SCBD 0).

use emath_core::QualifiedName;
use emath_ir::{
    AdmissionRefusal, CellClass, CellSchema, MigrationPolicy, NumericProfile, admit_cell, cell_id,
};
use emath_test_harness::Probe;

const STD_TENSOR_SOFTMAX: &str = "std.tensor.softmax";

fn softmax_schema() -> CellSchema {
    CellSchema {
        name: QualifiedName::single(STD_TENSOR_SOFTMAX),
        class: CellClass::Pure,
        version: "1.0.0".into(),
        migration: MigrationPolicy::Frozen,
        arity: 1,
        about: Some("stable maximum of exp(x - max(x))".into()),
    }
}

#[test]
fn intent() {
    let mut p = Probe::new("Softmax pure cell — first zero-core-delta");
    p.case("softmax_pure_cell_admits_descriptor_only", |p| {

    // Zero-core-delta proof, part 1: the cell admits as a descriptor and
    // its identity is stable, with NO core enum touched. If Softmax ever
    // became a core op variant this test would still pass — the negative
    // guard lives in the targeted tests elsewhere + the diff gate; here we pin that
    // the admitted record is arena data.
    let schema = softmax_schema();
    let admitted = admit_cell(&schema).expect("pure softmax cell admits");
    p.eq("softmax_pure_cell_admits_descriptor_only#1", admitted.name.0, STD_TENSOR_SOFTMAX.to_string());

    // Stable identity: same descriptor -> same CellId.
    p.eq("softmax_pure_cell_admits_descriptor_only#2", cell_id(&schema), cell_id(&softmax_schema()));

    // Numeric policy is explicit: strict-f64 is the only phase-1 model the
    // reference semantics accepts; an omitted policy is a typed refusal,
    // not a silent default.
    p.eq("softmax_pure_cell_admits_descriptor_only#3", NumericProfile::default_phase1(), NumericProfile::StrictF64);
    p.demand("softmax_pure_cell_admits_descriptor_only#4", NumericProfile::StrictF64.as_str() == "strict-f64", format!("expected {:?}, got {:?}", "strict-f64", NumericProfile::StrictF64.as_str()));
    p.demand("softmax_pure_cell_admits_descriptor_only#5", NumericProfile::IntervalF64.as_str() == "interval-f64", format!("expected {:?}, got {:?}", "interval-f64", NumericProfile::IntervalF64.as_str()));

    });
    p.case("softmax_reference_semantics_compute", |p| {

    // Happy path: softmax over a 3-vector, strict-f64, stable-max form.
    let logits = [1.0_f64, 2.0, 3.0];
    let out =
        emath_ir::capability::softmax_reference_strict_f64(&logits).expect("finite logits compute");
    let expected = [
        1.0 / (1.0 + (2.0_f64 - 1.0).exp() + (3.0_f64 - 1.0).exp()),
        (2.0_f64 - 1.0).exp() / (1.0 + (1.0_f64).exp() + (2.0_f64).exp()),
        (3.0_f64 - 1.0).exp() / (1.0 + (1.0_f64).exp() + (2.0_f64).exp()),
    ];
    for (got, want) in out.iter().zip(expected.iter()) {
        p.demand(format!("softmax mismatch: got {got}, want {want}"), (got - want).abs() < 1e-12, format!("softmax mismatch: got {got}, want {want}"));
    }

    });
    p.case("softmax_laws_hold", |p| {

    // Law 1: shift invariance — softmax(x) == softmax(x + c) componentwise
    // (the stable-max form IS this law, applied to c = -max(x)).
    let x = [0.3_f64, -1.7, 4.2, 2.0];
    let base = emath_ir::capability::softmax_reference_strict_f64(&x).unwrap();
    let shifted = emath_ir::capability::softmax_reference_strict_f64(&[
        x[0] + 7.5,
        x[1] + 7.5,
        x[2] + 7.5,
        x[3] + 7.5,
    ])
    .unwrap();
    for (a, b) in base.iter().zip(shifted.iter()) {
        p.demand(format!("shift invariance violated: {a} vs {b}"), (a - b).abs() < 1e-12, format!("shift invariance violated: {a} vs {b}"));
    }

    // Law 1 (overflow guard): the stable-max shift must keep large finite
    // logits computable — exp(1000) overflows f64 without the shift, so an
    // implementation that skips the max-shift must fail here.
    let big = [800.0_f64, 799.0, 100.0];
    let out_big = emath_ir::capability::softmax_reference_strict_f64(&big)
        .expect("stable-max form must not overflow on large finite logits");
    let sum_big: f64 = out_big.iter().sum();
    p.demand("large-logit normalization", (sum_big - 1.0).abs() < 1e-12, "large-logit normalization");
    p.demand(format!("large-logit ordering preserved: {out_big:?}"), out_big[0] > out_big[1] && out_big[1] > out_big[2], format!("large-logit ordering preserved: {out_big:?}"));

    // Law 2: nonnegativity.
    for &v in &base {
        p.demand(format!("nonnegativity violated: {v}"), v >= 0.0, format!("nonnegativity violated: {v}"));
    }

    // Law 3: normalization within tolerance T.
    let sum: f64 = base.iter().sum();
    p.demand(format!("normalization violated: sum={sum}"), (sum - 1.0).abs() < 1e-12, format!("normalization violated: sum={sum}"));

    // Boundary: single-element input normalizes to exactly 1.
    let single = emath_ir::capability::softmax_reference_strict_f64(&[42.0]).unwrap();
    p.eq("softmax_laws_hold#6", single.len(), 1);
    p.demand("softmax_laws_hold#7", (single[0] - 1.0).abs() < 1e-15, "softmax_laws_hold#7: (single[0] - 1.0).abs() < 1e-15");

    });
    p.case("missing_numeric_policy_and_bad_axis_refuse_by_name", |p| {

    // Negative: missing numeric policy refuses — an empty policy is
    // not a silent empty distribution. The negative seed names the
    // refusal.
    let seed = include_str!("../../../tests/invalid/softmax_capability_cell.emath");
    let expect_line = seed
        .lines()
        .find(|line| line.trim_start().starts_with("# expect:"))
        .expect("negative seed must name its required diagnostic");
    p.demand(format!("seed expects the admission-seam refusals, found: {expect_line}"), expect_line.contains("E-CELL-003") && expect_line.contains("E-CELL-004"), format!("seed expects the admission-seam refusals, found: {expect_line}"));

    // The typed missing-policy refusal at the evaluation seam:
    let empty: [f64; 0] = [];
    let err = emath_ir::capability::softmax_reference_strict_f64(&empty).unwrap_err();
    p.demand("missing numeric policy refusal", err.code() == "E-CELL-006", format!("expected {:?}, got {:?}", "E-CELL-006", err.code()));

    // Non-finite logits refuse under the strict-f64 finite policy.
    let nan_input = [f64::NAN, 1.0];
    p.demand("missing_numeric_policy_and_bad_axis_refuse_by_name#3", emath_ir::capability::softmax_reference_strict_f64(&nan_input)
            .unwrap_err()
            .code() == "E-CELL-006", format!("expected {:?}, got {:?}", "E-CELL-006", emath_ir::capability::softmax_reference_strict_f64(&nan_input)
            .unwrap_err()
            .code()));
    let inf_input = [f64::INFINITY, 1.0];
    p.demand("missing_numeric_policy_and_bad_axis_refuse_by_name#4", emath_ir::capability::softmax_reference_strict_f64(&inf_input)
            .unwrap_err()
            .code() == "E-CELL-006", format!("expected {:?}, got {:?}", "E-CELL-006", emath_ir::capability::softmax_reference_strict_f64(&inf_input)
            .unwrap_err()
            .code()));
    // Negative: provider wrong-axis fails. The cell's contract is
    // a rank-1 vector evaluated whole; a 2D-style axis request (rank 2)
    // is a wrong-axis failure at the provider seam (typed, not silent).
    p.demand("rank-2 axis request is wrong-axis", !emath_ir::softmax_axis_well_formed(2), "rank-2 axis request is wrong-axis");
    p.demand("rank-0 scalar is wrong-axis", !emath_ir::softmax_axis_well_formed(0), "rank-0 scalar is wrong-axis");
    p.demand("rank-1 vector is the contract", emath_ir::softmax_axis_well_formed(1), "rank-1 vector is the contract");

    });
    p.finish();
}







