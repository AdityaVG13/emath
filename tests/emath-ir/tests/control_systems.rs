//! emath-r3-sde-control-zxkl (thin B43 control surface): transfer
//! functions, state-space DC gain, and stability — stacked on B28
//! polynomials, B34 linear solves, B14 complex.
//!
//! The bead's law, sliced to the numeric-kernel + EMIR seam (B37 SDE is
//! WORLD-DEPENDENT and needs the AmberHarbor vnqo seed/stream
//! coordination — the named deferral; controller DESIGN (pole
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
//!   refuses at COMPILE (the closed vocabulary's shape law). A
//!   non-finite coefficient/entry/point refuses `E-CONTROL-001`.
//! - Non-finite cell parameters refuse one layer earlier at the seam
//!   (`E-CELL-006`, the all-finite guard).

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::{
    compile_reference, std_cell_registry, ParamShape, TermCompileError,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_term::{Signature, SymbolId, Term, VariableId};

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    // The .14 seam law: LoadInput per input, result = last register.
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

/// Registry-path evaluation of a fixed-shape cell.
fn cell_seval(
    name: &str,
    operator: &str,
    arity: usize,
    params: Vec<(String, ParamShape)>,
    inputs: &[Value],
) -> Result<Value, EvalFault> {
    let term = Term::Apply {
        operator: SymbolId(operator.into()),
        arguments: params
            .iter()
            .map(|(name, _)| Term::Variable(VariableId(name.clone())))
            .collect(),
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId(operator.into()), arity)
        .expect("single-operator signature is conflict-free");
    let _cell = compile_reference(&term, &signature, &params, Vec::new(), name)
        .expect("control cell compiles through the call surface");
    let count = inputs.len();
    let mut ops: Vec<(EmirOp, Span)> = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    ops.push((
        EmirOp::ApplyCapability {
            capability: name.to_string(),
            class: CellClass::Pure,
            args: (0..count as u32).map(EmirValue).collect(),
        },
        Span::default(),
    ));
    let program = EmirProgram {
        ops,
        result: EmirValue(count as u32),
        input_count: count as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

fn matrix(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}

/// H(s) = (s + 6)/(s² + 3s + 2) = (s + 6)/((s + 1)(s + 2)): poles at
/// −1 and −2, H(0) = 3, H(−3) = (−3 + 6)/(9 − 9 + 2) = 3/2 — exact
/// integer/half-integer anchors (no floating-point slack to hide a
/// wrong Horner or a wrong division).
#[test]
fn transfer_function_evaluates_known_rational() {
    let num = Value::Vector(vec![6.0, 1.0]);
    let den = Value::Vector(vec![2.0, 3.0, 1.0]);
    let at_0 = eval(
        vec![EmirOp::ControlTransferEval(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[num.clone(), den.clone(), Value::F64(0.0)],
    )
    .expect("transfer evaluates at s = 0");
    assert_eq!(f64_of(&at_0), 3.0, "H(0) = 6/2");
    let at_minus_3 = eval(
        vec![EmirOp::ControlTransferEval(EmirValue(0), EmirValue(1), EmirValue(2))],
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
        vec![EmirOp::ControlTransferEval(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[num.clone(), den.clone(), Value::F64(-1.0)],
    )
    .expect_err("s = -1 is a pole of the denominator");
    assert_eq!(refused_code(&fault), "E-CONTROL-002");
    for zero_den in [Value::Vector(vec![]), Value::Vector(vec![0.0, 0.0])] {
        let fault = eval(
            vec![EmirOp::ControlTransferEval(EmirValue(0), EmirValue(1), EmirValue(2))],
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
        vec![EmirOp::ControlTransferEval(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[num, den.clone(), Value::F64(0.0)],
    )
    .expect_err("NaN numerator coefficient");
    assert_eq!(refused_code(&fault), "E-CONTROL-001");
    let num = Value::Vector(vec![6.0, 1.0]);
    let fault = eval(
        vec![EmirOp::ControlTransferEval(EmirValue(0), EmirValue(1), EmirValue(2))],
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
        vec![EmirOp::ControlDcGain(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[stable_a, b.clone(), c.clone()],
    )
    .expect("stable carrier has a DC gain");
    assert_eq!(f64_of(&dc), 0.5, "c·(−A)⁻¹·b for the companion pair");
    let h0 = eval(
        vec![EmirOp::ControlTransferEval(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[Value::Vector(vec![1.0]), Value::Vector(vec![2.0, 3.0, 1.0]), Value::F64(0.0)],
    )
    .expect("transfer evaluates at s = 0");
    assert_eq!(f64_of(&h0).to_bits(), f64_of(&dc).to_bits(), "bit-exact cross-form parity");
    // The unstable twin (poles +1 and −4) refuses: its DC gain does
    // not exist (E-CONTROL-003) — the honesty gate, not a garbage 0.5.
    let unstable_a = matrix(2, 2, &[0.0, 1.0, 2.0, -3.0]);
    let fault = eval(
        vec![EmirOp::ControlDcGain(EmirValue(0), EmirValue(1), EmirValue(2))],
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
        vec![EmirOp::ControlDcGain(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[nonsquare, b.clone(), c.clone()],
    )
    .expect_err("A is not square");
    assert_eq!(refused_code(&fault), "E-CONTROL-004");
    let short_b = Value::Vector(vec![1.0]);
    let square = matrix(2, 2, &[0.0, 1.0, -2.0, -3.0]);
    let fault = eval(
        vec![EmirOp::ControlDcGain(EmirValue(0), EmirValue(1), EmirValue(2))],
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
        eval(
            vec![EmirOp::ControlPolesStable(EmirValue(0))],
            &[den],
        )
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
        vec![EmirOp::ControlPolesStable(EmirValue(0))],
        &[Value::Vector(vec![1.0, 0.0, 1.0])],
    )
    .expect_err("s² + 1 is marginal (zero first-column entry)");
    assert_eq!(refused_code(&fault), "E-CONTROL-005");
    for degenerate in [Value::Vector(vec![]), Value::Vector(vec![0.0])] {
        let fault = eval(
            vec![EmirOp::ControlPolesStable(EmirValue(0))],
            &[degenerate],
        )
        .expect_err("the zero polynomial has no pole set");
        assert_eq!(refused_code(&fault), "E-CONTROL-002");
    }
    let fault = eval(
        vec![EmirOp::ControlPolesStable(EmirValue(0))],
        &[Value::Vector(vec![2.0, f64::NAN, 1.0])],
    )
    .expect_err("NaN coefficient");
    assert_eq!(refused_code(&fault), "E-CONTROL-001");
}

/// Registry cells (the anti-LOC law): `std.control.transfer_eval`,
/// `std.control.dc_gain`, and `std.control.poles_stable` agree
/// BIT-FOR-BIT with the bare ops, and the all-finite guard refuses a
/// NaN parameter one layer earlier (`E-CELL-006`).
#[test]
fn control_cell_preserves_parity_and_guards() {
    let num = Value::Vector(vec![6.0, 1.0]);
    let den = Value::Vector(vec![2.0, 3.0, 1.0]);
    let cell_value = cell_seval(
        "std.control.transfer_eval",
        "transfer_eval",
        3,
        vec![
            ("num".to_string(), ParamShape::Vector),
            ("den".to_string(), ParamShape::Vector),
            ("x".to_string(), ParamShape::Scalar),
        ],
        &[num.clone(), den.clone(), Value::F64(0.0)],
    )
    .expect("transfer cell computes");
    let bare_value = eval(
        vec![EmirOp::ControlTransferEval(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[num.clone(), den.clone(), Value::F64(0.0)],
    )
    .expect("bare transfer op computes");
    assert_eq!(
        f64_of(&cell_value).to_bits(),
        f64_of(&bare_value).to_bits(),
        "cell and bare op agree bit-for-bit"
    );
    let a = matrix(2, 2, &[0.0, 1.0, -2.0, -3.0]);
    let b = Value::Vector(vec![0.0, 1.0]);
    let c = Value::Vector(vec![1.0, 0.0]);
    let cell_dc = cell_seval(
        "std.control.dc_gain",
        "dc_gain",
        3,
        vec![
            ("A".to_string(), ParamShape::Matrix),
            ("b".to_string(), ParamShape::Vector),
            ("c".to_string(), ParamShape::Vector),
        ],
        &[a.clone(), b.clone(), c.clone()],
    )
    .expect("dc-gain cell computes");
    let bare_dc = eval(
        vec![EmirOp::ControlDcGain(EmirValue(0), EmirValue(1), EmirValue(2))],
        &[a.clone(), b.clone(), c.clone()],
    )
    .expect("bare dc-gain op computes");
    assert_eq!(
        f64_of(&cell_dc).to_bits(),
        f64_of(&bare_dc).to_bits(),
        "dc-gain cell and bare op agree bit-for-bit"
    );
    let cell_stable = cell_seval(
        "std.control.poles_stable",
        "poles_stable",
        1,
        vec![("den".to_string(), ParamShape::Vector)],
        &[den],
    )
    .expect("stability cell computes");
    let bare_stable = eval(
        vec![EmirOp::ControlPolesStable(EmirValue(0))],
        &[Value::Vector(vec![2.0, 3.0, 1.0])],
    )
    .expect("bare stability op computes");
    assert_eq!(bool_of(&cell_stable), bool_of(&bare_stable));
    let fault = cell_seval(
        "std.control.transfer_eval",
        "transfer_eval",
        3,
        vec![
            ("num".to_string(), ParamShape::Vector),
            ("den".to_string(), ParamShape::Vector),
            ("x".to_string(), ParamShape::Scalar),
        ],
        &[num, Value::Vector(vec![2.0, f64::NAN, 1.0]), Value::F64(0.0)],
    )
    .expect_err("the all-finite guard keeps NaN out of the cell seam");
    assert_eq!(refused_code(&fault), "E-CELL-006");
}

/// Shape law at COMPILE: a scalar where a vector is needed (and a
/// vector where a matrix is needed) refuses through the closed
/// vocabulary's shape law — never a runtime surprise.
#[test]
fn control_compile_shape_refusals() {
    let scalar_params = vec![
        ("num".to_string(), ParamShape::Scalar),
        ("den".to_string(), ParamShape::Vector),
        ("x".to_string(), ParamShape::Scalar),
    ];
    let term = Term::Apply {
        operator: SymbolId("transfer_eval".into()),
        arguments: vec![
            Term::Variable(VariableId("num".into())),
            Term::Variable(VariableId("den".into())),
            Term::Variable(VariableId("x".into())),
        ],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("transfer_eval".into()), 3)
        .expect("single-operator signature is conflict-free");
    let error = compile_reference(&term, &signature, &scalar_params, Vec::new(), "std.control.transfer_eval")
        .expect_err("a scalar numerator is not a transfer carrier");
    assert!(
        matches!(error, TermCompileError::ShapeMismatch { .. }),
        "unexpected refusal: {error:?}"
    );
    let vector_params = vec![
        ("A".to_string(), ParamShape::Vector),
        ("b".to_string(), ParamShape::Vector),
        ("c".to_string(), ParamShape::Vector),
    ];
    let term = Term::Apply {
        operator: SymbolId("dc_gain".into()),
        arguments: vec![
            Term::Variable(VariableId("A".into())),
            Term::Variable(VariableId("b".into())),
            Term::Variable(VariableId("c".into())),
        ],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("dc_gain".into()), 3)
        .expect("single-operator signature is conflict-free");
    let error = compile_reference(&term, &signature, &vector_params, Vec::new(), "std.control.dc_gain")
        .expect_err("a vector A is not a state-space carrier");
    assert!(
        matches!(error, TermCompileError::ShapeMismatch { .. }),
        "unexpected refusal: {error:?}"
    );
}

/// The registry exposes exactly the three control cells (the cohort
/// count law lives in the fjxh_14 suite; this pins the zxkl slice).
#[test]
fn control_registry_exposes_cells() {
    let registry = std_cell_registry();
    for name in [
        "std.control.transfer_eval",
        "std.control.dc_gain",
        "std.control.poles_stable",
    ] {
        assert!(
            registry.contains_key(name),
            "missing control cell {name}: {:?}",
            registry.keys().collect::<Vec<_>>()
        );
    }
}
