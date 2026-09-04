//! (thin B43 control surface): transfer
//! functions, state-space DC gain, and stability — stacked on B28
//! polynomials, B34 linear solves, B14 complex.
//!
//! The law, sliced to the numeric-kernel + EMIR seam (B37 SDE is
//! WORLD-DEPENDENT and needs seed/stream World support — the
//! delayed item; controller DESIGN (pole
//! placement, LQR) is not claimed either):
//! - **Transfer law**: `transfer_eval(num, den, x)` = num(x)/den(x)
//!   over ASCENDING coefficient vectors (the B28 representation). A
//!   denominator that evaluates to 0 at the point (including the zero
//!   polynomial: empty or all-zero) refuses `E-CONTROL-002` — the
//!   value does not exist, never Inf-by-silent-convention.
//! - **Stability honesty**: `poles_stable(den)` is the Routh–Hurwitz
//!   first-column sign test — pure polynomial arithmetic (B28
//!   stacking), no root-finding, no claimed eigenvalues. Strictly
//!   stable (all real parts < 0) is TRUE, provably unstable is FALSE,
//!   and a DEGENERATE table (zero first-column entry: marginal or
//!   ε-ambiguous) refuses `E-CONTROL-005` — the auxiliary-polynomial
//!   refinement is the named deferral. The identically-zero polynomial
//!   refuses `E-CONTROL-002` (it has no pole set).
//! - **State-space law**: `dc_gain(A, b, c)` = c·(−A)⁻¹·b for the
//!   carrier with implicit D = 0 (the full feedthrough term is the
//!   named deferral). The characteristic polynomial comes from the
//!   Faddeev–LeVerrier recursion (deterministic matrix arithmetic),
//!   stability is the SAME Routh–Hurwitz predicate, an unstable
//!   carrier refuses `E-CONTROL-003` (the DC gain does not exist),
//!   and the solve is pivoted Gauss elimination with deterministic
//!   tie-breaking (first index on exact ties).
//! - **Shape law**: a non-square A or a b/c length ≠ n refuses
//!   `E-CONTROL-004` at the kernel; a scalar where a vector is needed
//!   refuses through the capsule contract's carrier law. A
//!   non-finite coefficient/entry/point refuses `E-CONTROL-001`.
//!
//! MIGRATION (dispatch `emath:cleanup0904:p1:dynamics`, correction
//! mail 104): the retired domain-named `EmirOp::ControlTransferEval` /
//! `ControlDcGain` / `ControlPolesStable` variants are gone from the
//! closed op set; every call site is the universal `ApplyCapability`
//! seam over the capsule-active FeatureIDs of
//! `language/spec/capabilities/dynamics-control-pde.emath`, whose
//! native kernels (`checked-polynomial-ratio`, `checked-linear-projection`,
//! `checked-sign-table`) install with the checked-in Language Image.
//! The kernels check non-finite parameters themselves, so the former
//! cell-guard refusal (`E-CELL-006`) surfaces as the capsule's
//! `E-CONTROL-001`.

use std::path::{Path, PathBuf};

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::{install_language_distribution, native_kernel};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};

fn language_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

/// Capability cells resolve against the installed Language Image
/// (thread-local bindings); every evaluating test installs the
/// checked-in distribution first.
fn install_language() {
    let distribution = load_language_distribution(&language_root()).expect("language distribution");
    install_language_distribution(&distribution).expect("control kernels install");
}

/// The active universal seam for domain math: an `ApplyCapability`
/// over a capsule-active FeatureID (no domain-named `EmirOp`).
fn cell(capability: &str, args: Vec<EmirValue>) -> EmirOp {
    EmirOp::ApplyCapability {
        capability: capability.to_string(),
        class: CellClass::Pure,
        args,
    }
}

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    install_language();
    // The seam law: LoadInput per input, result = last register.
    let mut program_ops: Vec<(EmirOp, Span)> = (0..inputs.len())
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    program_ops.extend(ops.into_iter().map(|op| (op, Span::default())));
    let result = EmirValue(program_ops.len() as u32 - 1);
    let program = EmirProgram {
        ops: program_ops,
        result,
        input_count: inputs.len() as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

fn f64_of(value: &Value) -> f64 {
    let Value::F64(x) = value else {
        panic!("expected a scalar, got {value:?}")
    };
    *x
}

fn bool_of(value: &Value) -> bool {
    let Value::Bool(b) = value else {
        panic!("expected a bool, got {value:?}")
    };
    *b
}

fn refused_code(fault: &EvalFault) -> String {
    let EvalFault::CapabilityRefused { code, .. } = fault else {
        panic!("expected a typed capability refusal, got {fault:?}")
    };
    code.clone()
}

const TRANSFER_EVAL: &str = "std.capability.control.transfer-eval";
const DC_GAIN: &str = "std.capability.control.dc-gain";
const POLES_STABLE: &str = "std.capability.control.poles-stable";

/// H(s) = (s + 6)/(s² + 3s + 2) = (s + 6)/((s + 1)(s + 2)): poles at
/// −1 and −2, H(0) = 3, H(−3) = (−3 + 6)/(9 − 9 + 2) = 3/2 — exact
/// integer/half-integer anchors (no floating-point slack to hide a
/// wrong Horner or a wrong division).
#[test]
fn transfer_function_evaluates_known_rational() {
    let num = Value::Vector(vec![6.0, 1.0]);
    let den = Value::Vector(vec![2.0, 3.0, 1.0]);
    let at_0 = eval(
        vec![cell(
            TRANSFER_EVAL,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[num.clone(), den.clone(), Value::F64(0.0)],
    )
    .expect("transfer evaluates at s = 0");
    assert_eq!(f64_of(&at_0), 3.0, "H(0) = 6/2");
    let at_minus_3 = eval(
        vec![cell(
            TRANSFER_EVAL,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[num, den, Value::F64(-3.0)],
    )
    .expect("transfer evaluates at s = -3");
    assert_eq!(f64_of(&at_minus_3), 1.5, "H(-3) = 3/2");
}

/// A pole hit (den(−1) = 0 for (s+1)(s+2)) and the zero denominator
/// polynomial (empty or all-zero) both refuse `E-CONTROL-002`: the
/// value does not exist — never Inf-by-silent-convention.
#[test]
fn transfer_function_zero_denominator_refuses() {
    let num = Value::Vector(vec![6.0, 1.0]);
    let den = Value::Vector(vec![2.0, 3.0, 1.0]);
    let fault = eval(
        vec![cell(
            TRANSFER_EVAL,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[num.clone(), den.clone(), Value::F64(-1.0)],
    )
    .expect_err("s = -1 is a pole of the denominator");
    assert_eq!(refused_code(&fault), "E-CONTROL-002");
    for zero_den in [Value::Vector(vec![]), Value::Vector(vec![0.0, 0.0])] {
        let fault = eval(
            vec![cell(
                TRANSFER_EVAL,
                vec![EmirValue(0), EmirValue(1), EmirValue(2)],
            )],
            &[num.clone(), zero_den, Value::F64(0.5)],
        )
        .expect_err("the zero polynomial divides nothing");
        assert_eq!(refused_code(&fault), "E-CONTROL-002");
    }
}

/// A non-finite numerator coefficient, denominator coefficient, or
/// evaluation point refuses `E-CONTROL-001` — never a silently
/// corrupted ratio.
#[test]
fn transfer_function_non_finite_input_refuses() {
    let num = Value::Vector(vec![6.0, f64::NAN]);
    let den = Value::Vector(vec![2.0, 3.0, 1.0]);
    let fault = eval(
        vec![cell(
            TRANSFER_EVAL,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[num, den.clone(), Value::F64(0.0)],
    )
    .expect_err("NaN numerator coefficient");
    assert_eq!(refused_code(&fault), "E-CONTROL-001");
    let num = Value::Vector(vec![6.0, 1.0]);
    let fault = eval(
        vec![cell(
            TRANSFER_EVAL,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[num, den, Value::F64(f64::INFINITY)],
    )
    .expect_err("non-finite evaluation point");
    assert_eq!(refused_code(&fault), "E-CONTROL-001");
}

/// Cross-representation consistency: the system 1/(s² + 3s + 2) as a
/// transfer function ([1], [2, 3, 1]) and in companion-like state-space
/// form (A = [[0, 1], [−2, −3]], b = [0, 1], c = [1, 0], D = 0) MUST
/// agree BIT-FOR-BIT at s = 0: dc gain = c·(−A)⁻¹·b = 0.5 = H(0). A
/// wrong Faddeev–LeVerrier sign, a transposed solve, or a dropped
/// stability gate breaks the bit parity or the refused unstable twin.
#[test]
fn dc_gain_matches_transfer_function() {
    let stable_a = matrix(2, 2, &[0.0, 1.0, -2.0, -3.0]);
    let b = Value::Vector(vec![0.0, 1.0]);
    let c = Value::Vector(vec![1.0, 0.0]);
    let dc = eval(
        vec![cell(
            DC_GAIN,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[stable_a, b.clone(), c.clone()],
    )
    .expect("stable carrier has a DC gain");
    assert_eq!(f64_of(&dc), 0.5, "c·(−A)⁻¹·b for the companion pair");
    let h0 = eval(
        vec![cell(
            TRANSFER_EVAL,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[
            Value::Vector(vec![1.0]),
            Value::Vector(vec![2.0, 3.0, 1.0]),
            Value::F64(0.0),
        ],
    )
    .expect("transfer evaluates at s = 0");
    assert_eq!(
        f64_of(&h0).to_bits(),
        f64_of(&dc).to_bits(),
        "bit-exact cross-form parity"
    );
    // The unstable twin (poles +1 and −4) refuses: its DC gain does
    // not exist (E-CONTROL-003) — the honesty gate, not a garbage 0.5.
    let unstable_a = matrix(2, 2, &[0.0, 1.0, 2.0, -3.0]);
    let fault = eval(
        vec![cell(
            DC_GAIN,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[unstable_a, b, c],
    )
    .expect_err("poles at +1 and -4: no DC gain exists");
    assert_eq!(refused_code(&fault), "E-CONTROL-003");
}

/// A non-square A, or b/c whose length differs from the state
/// dimension, refuses `E-CONTROL-004` — shape is part of the carrier
/// law, never silently truncated or broadcast.
#[test]
fn dc_gain_shape_refusals() {
    let nonsquare = matrix(2, 3, &[0.0, 1.0, 0.0, -2.0, -3.0, 0.0]);
    let b = Value::Vector(vec![0.0, 1.0]);
    let c = Value::Vector(vec![1.0, 0.0]);
    let fault = eval(
        vec![cell(
            DC_GAIN,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[nonsquare, b.clone(), c.clone()],
    )
    .expect_err("A is not square");
    assert_eq!(refused_code(&fault), "E-CONTROL-004");
    let short_b = Value::Vector(vec![1.0]);
    let square = matrix(2, 2, &[0.0, 1.0, -2.0, -3.0]);
    let fault = eval(
        vec![cell(
            DC_GAIN,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[square, short_b, c],
    )
    .expect_err("b has the wrong length");
    assert_eq!(refused_code(&fault), "E-CONTROL-004");
}

/// Routh–Hurwitz laws: (s+1)(s+2) and (s+1)³ are strictly stable
/// (TRUE); the cubic s³ + ½s² + s + 2 has two right-half-plane roots
/// (two first-column sign changes → FALSE); the marginal s² + 1 (zero
/// first-column entry) refuses `E-CONTROL-005`; the zero polynomial
/// refuses `E-CONTROL-002`; non-finite coefficients refuse
/// `E-CONTROL-001`. ASCENDING carriers, same as B28.
#[test]
fn poles_and_stability_laws_hold() {
    let stable = |den: Value, label: &str| {
        eval(vec![cell(POLES_STABLE, vec![EmirValue(0)])], &[den])
            .unwrap_or_else(|fault| panic!("{label}: stable-pole predicate computes: {fault:?}"))
    };
    assert!(
        bool_of(&stable(Value::Vector(vec![2.0, 3.0, 1.0]), "quadratic")),
        "(s+1)(s+2) is strictly stable"
    );
    assert!(
        bool_of(&stable(Value::Vector(vec![1.0, 3.0, 3.0, 1.0]), "cubic")),
        "(s+1)³ is strictly stable"
    );
    let unstable = stable(Value::Vector(vec![2.0, 1.0, 0.5, 1.0]), "unstable cubic");
    assert!(!bool_of(&unstable), "s³ + ½s² + s + 2 has two RHP roots");
    let fault = eval(
        vec![cell(POLES_STABLE, vec![EmirValue(0)])],
        &[Value::Vector(vec![1.0, 0.0, 1.0])],
    )
    .expect_err("s² + 1 is marginal (zero first-column entry)");
    assert_eq!(refused_code(&fault), "E-CONTROL-005");
    for degenerate in [Value::Vector(vec![]), Value::Vector(vec![0.0])] {
        let fault = eval(vec![cell(POLES_STABLE, vec![EmirValue(0)])], &[degenerate])
            .expect_err("the zero polynomial has no pole set");
        assert_eq!(refused_code(&fault), "E-CONTROL-002");
    }
    let fault = eval(
        vec![cell(POLES_STABLE, vec![EmirValue(0)])],
        &[Value::Vector(vec![2.0, f64::NAN, 1.0])],
    )
    .expect_err("NaN coefficient");
    assert_eq!(refused_code(&fault), "E-CONTROL-001");
}

/// Capsule parity (the anti-LOC law): the three control capsules
/// answer with the exact closed-form values of record —
/// `std.control.transfer_eval` H(0) = 3, `std.control.dc_gain` = 1/2,
/// `std.control.poles_stable` = TRUE — and the kernels' own carrier
/// checks keep a NaN denominator out (`E-CONTROL-001`; the former
/// `E-CELL-006` all-finite cell guard surfaces one layer lower now).
#[test]
fn control_cells_preserve_parity_and_guards() {
    let num = Value::Vector(vec![6.0, 1.0]);
    let den = Value::Vector(vec![2.0, 3.0, 1.0]);
    let cell_value = eval(
        vec![cell(
            TRANSFER_EVAL,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[num.clone(), den.clone(), Value::F64(0.0)],
    )
    .expect("transfer cell computes");
    assert_eq!(f64_of(&cell_value), 3.0, "H(0) = 6/2 through the seam");
    let a = matrix(2, 2, &[0.0, 1.0, -2.0, -3.0]);
    let b = Value::Vector(vec![0.0, 1.0]);
    let c = Value::Vector(vec![1.0, 0.0]);
    let cell_dc = eval(
        vec![cell(
            DC_GAIN,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[a, b, c],
    )
    .expect("dc-gain cell computes");
    assert_eq!(f64_of(&cell_dc), 0.5, "c·(−A)⁻¹·b through the seam");
    let cell_stable = eval(vec![cell(POLES_STABLE, vec![EmirValue(0)])], &[den.clone()])
        .expect("stability cell computes");
    assert!(bool_of(&cell_stable), "(s+1)(s+2) is strictly stable");
    let fault = eval(
        vec![cell(
            TRANSFER_EVAL,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[
            num,
            Value::Vector(vec![2.0, f64::NAN, 1.0]),
            Value::F64(0.0),
        ],
    )
    .expect_err("the kernel's carrier checks keep NaN out of the seam");
    assert_eq!(refused_code(&fault), "E-CONTROL-001");
}

/// Shape law at the capsule carrier: a scalar where a vector is needed
/// (and a vector where a matrix is needed) refuses through the
/// capsule's carrier law — never a runtime surprise. The former
/// compile-time `ShapeMismatch` check rode the retired per-op
/// compiler; the carrier law itself is capsule data now.
#[test]
fn control_shape_refusals() {
    let scalar_num = Value::F64(6.0);
    let den = Value::Vector(vec![2.0, 3.0, 1.0]);
    let fault = eval(
        vec![cell(
            TRANSFER_EVAL,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[scalar_num, den.clone(), Value::F64(0.0)],
    )
    .expect_err("a scalar numerator is not a transfer carrier");
    assert!(
        refused_code(&fault).starts_with("E-CONTROL"),
        "the transfer capsule refuses the wrong carrier typed: {fault:?}"
    );
    let vector_a = Value::Vector(vec![0.0, 1.0]);
    let b = Value::Vector(vec![0.0, 1.0]);
    let c = Value::Vector(vec![1.0, 0.0]);
    let fault = eval(
        vec![cell(
            DC_GAIN,
            vec![EmirValue(0), EmirValue(1), EmirValue(2)],
        )],
        &[vector_a, b, c],
    )
    .expect_err("a vector A is not a state-space carrier");
    assert_eq!(refused_code(&fault), "E-CONTROL-004");
}

/// The kernel ABI exposes exactly the three control kernels (the
/// cohort count law lives in the dedicated tests; this pins the
/// signature).
#[test]
fn control_registry_exposes_cells() {
    install_language();
    for name in [TRANSFER_EVAL, DC_GAIN, POLES_STABLE] {
        assert!(
            native_kernel(name).is_some(),
            "missing control kernel binding {name}"
        );
    }
}

fn matrix(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}
