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
fn softmax_pure_cell_admits_descriptor_only() {
    // Zero-core-delta proof, part 1: the cell admits as a descriptor and
    // its identity is stable, with NO core enum touched. If Softmax ever
    // became a core op variant this test would still pass — the negative
    // guard lives in the targeted tests elsewhere + the diff gate; here we pin that
    // the admitted record is arena data.
    let schema = softmax_schema();
    let admitted = admit_cell(&schema).expect("pure softmax cell admits");
    assert_eq!(admitted.name.0, STD_TENSOR_SOFTMAX);

    // Stable identity: same descriptor -> same CellId.
    assert_eq!(cell_id(&schema), cell_id(&softmax_schema()));

    // Numeric policy is explicit: strict-f64 is the only phase-1 model the
    // reference semantics accepts; an omitted policy is a typed refusal,
    // not a silent default.
    assert_eq!(NumericProfile::default_phase1(), NumericProfile::StrictF64);
    assert_eq!(NumericProfile::StrictF64.as_str(), "strict-f64");
    assert_eq!(NumericProfile::IntervalF64.as_str(), "interval-f64");
}

#[test]
fn softmax_reference_semantics_compute() {
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
        assert!(
            (got - want).abs() < 1e-12,
            "softmax mismatch: got {got}, want {want}"
        );
    }
}

#[test]
fn softmax_laws_hold() {
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
        assert!(
            (a - b).abs() < 1e-12,
            "shift invariance violated: {a} vs {b}"
        );
    }

    // Law 1 (overflow guard): the stable-max shift must keep large finite
    // logits computable — exp(1000) overflows f64 without the shift, so an
    // implementation that skips the max-shift must fail here.
    let big = [800.0_f64, 799.0, 100.0];
    let out_big = emath_ir::capability::softmax_reference_strict_f64(&big)
        .expect("stable-max form must not overflow on large finite logits");
    let sum_big: f64 = out_big.iter().sum();
    assert!((sum_big - 1.0).abs() < 1e-12, "large-logit normalization");
    assert!(
        out_big[0] > out_big[1] && out_big[1] > out_big[2],
        "large-logit ordering preserved: {out_big:?}"
    );

    // Law 2: nonnegativity.
    for &v in &base {
        assert!(v >= 0.0, "nonnegativity violated: {v}");
    }

    // Law 3: normalization within tolerance T.
    let sum: f64 = base.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-12,
        "normalization violated: sum={sum}"
    );

    // Boundary: single-element input normalizes to exactly 1.
    let single = emath_ir::capability::softmax_reference_strict_f64(&[42.0]).unwrap();
    assert_eq!(single.len(), 1);
    assert!((single[0] - 1.0).abs() < 1e-15);
}

#[test]
fn missing_numeric_policy_and_bad_axis_refuse_by_name() {
    // Negative: missing numeric policy refuses — an empty policy is
    // not a silent empty distribution. The negative seed names the
    // refusal.
    let seed = include_str!("../../../tests/invalid/softmax_capability_cell.emath");
    let expect_line = seed
        .lines()
        .find(|line| line.trim_start().starts_with("# expect:"))
        .expect("negative seed must name its required diagnostic");
    assert!(
        expect_line.contains("E-CELL-003") && expect_line.contains("E-CELL-004"),
        "seed expects the admission-seam refusals, found: {expect_line}"
    );

    // The typed missing-policy refusal at the evaluation seam:
    let empty: [f64; 0] = [];
    let err = emath_ir::capability::softmax_reference_strict_f64(&empty).unwrap_err();
    assert_eq!(err.code(), "E-CELL-006", "missing numeric policy refusal");

    // Non-finite logits refuse under the strict-f64 finite policy.
    let nan_input = [f64::NAN, 1.0];
    assert_eq!(
        emath_ir::capability::softmax_reference_strict_f64(&nan_input)
            .unwrap_err()
            .code(),
        "E-CELL-006"
    );
    let inf_input = [f64::INFINITY, 1.0];
    assert_eq!(
        emath_ir::capability::softmax_reference_strict_f64(&inf_input)
            .unwrap_err()
            .code(),
        "E-CELL-006"
    );
    // Negative: provider wrong-axis fails. The cell's contract is
    // a rank-1 vector evaluated whole; a 2D-style axis request (rank 2)
    // is a wrong-axis failure at the provider seam (typed, not silent).
    assert!(
        !emath_ir::softmax_axis_well_formed(2),
        "rank-2 axis request is wrong-axis"
    );
    assert!(
        !emath_ir::softmax_axis_well_formed(0),
        "rank-0 scalar is wrong-axis"
    );
    assert!(
        emath_ir::softmax_axis_well_formed(1),
        "rank-1 vector is the contract"
    );
}
