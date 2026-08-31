//! emath-r3-funcspaces-poly-hjor (slice 1): the B28 compute layer —
//! polynomials as dense coefficient vectors.
//!
//! The bead's law, sliced to the numeric-kernel + EMIR seam (the
//! FORMAL-vs-evaluated type split and `Lp<2, Omega, mu>` are the named
//! design deferrals — B28's type question and C10's value-generics):
//! - **Representation law**: a polynomial is a dense coefficient
//!   vector, ASCENDING order (index i = coefficient of xⁱ). The EMPTY
//!   vector is the zero polynomial (additive identity) — `poly_mul`
//!   with it yields empty, `poly_eval` of it is 0.0 (documented
//!   algebra, never a shape error).
//! - **Addition** is coefficientwise: `poly_add` binds to the EXISTING
//!   generic `VectorAdd` op (the 4wj0 norm precedent — a name binding,
//!   zero new op).
//! - **Multiplication** is the Cauchy convolution (`PolyMul`, new op):
//!   `c[i+j] += a[i]·b[j]` — deterministic ascending-index order.
//! - **Evaluation** is Horner (`PolyEval`, new op): deterministic,
//!   strict-f64, one pass.
//! - Non-finite coefficients refuse `E-POLY-001`; a non-finite
//!   evaluation point refuses `E-POLY-002` — never a silently
//!   corrupted result (the negative seed's shape).

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

fn vector_of(value: &Value) -> Vec<f64> {
    let Value::Vector(v) = value else {
        panic!("expected a vector, got {value:?}")
    };
    v.clone()
}

fn f64_of(value: &Value) -> f64 {
    let Value::F64(x) = value else {
        panic!("expected a scalar, got {value:?}")
    };
    *x
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
    let cell = compile_reference(&term, &signature, &params, Vec::new(), name)
        .expect("poly cell compiles through the call surface");
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

#[test]
fn polynomial_multiplication_returns_known_products() {
    // (1 + x)(1 + x) = 1 + 2x + x² and (1 + x)(1 − x) = 1 − x² — the
    // convolution law with exact integer-valued coefficients (no
    // floating-point slack to hide a wrong accumulation order).
    let one_plus_x = Value::Vector(vec![1.0, 1.0]);
    let product = eval(
        vec![EmirOp::PolyMul(EmirValue(0), EmirValue(1))],
        &[one_plus_x.clone(), one_plus_x.clone()],
    )
    .expect("poly mul computes");
    assert_eq!(vector_of(&product), vec![1.0, 2.0, 1.0]);
    let one_minus_x = Value::Vector(vec![1.0, -1.0]);
    let product = eval(
        vec![EmirOp::PolyMul(EmirValue(0), EmirValue(1))],
        &[one_plus_x, one_minus_x],
    )
    .expect("poly mul computes");
    assert_eq!(vector_of(&product), vec![1.0, 0.0, -1.0]);
}

#[test]
fn polynomial_multiplication_obeys_identity_and_zero_laws() {
    // [1] (the constant 1) is the multiplicative identity; the EMPTY
    // carrier is the zero polynomial (the documented representation
    // law): mul yields empty, eval yields 0.
    let p = Value::Vector(vec![2.0, 3.0]);
    let one = Value::Vector(vec![1.0]);
    let product = eval(
        vec![EmirOp::PolyMul(EmirValue(0), EmirValue(1))],
        &[p.clone(), one],
    )
    .expect("identity law");
    assert_eq!(vector_of(&product), vec![2.0, 3.0]);
    let zero = Value::Vector(vec![]);
    let product = eval(
        vec![EmirOp::PolyMul(EmirValue(0), EmirValue(1))],
        &[p.clone(), zero.clone()],
    )
    .expect("zero law");
    assert_eq!(vector_of(&product), Vec::<f64>::new());
    let value = eval(
        vec![EmirOp::PolyEval(EmirValue(0), EmirValue(1))],
        &[zero, Value::F64(5.0)],
    )
    .expect("zero polynomial evaluates to 0");
    assert_eq!(f64_of(&value), 0.0);
}

#[test]
fn horner_evaluation_returns_known_values() {
    // p = 2 + 3x + 4x²: at x=2 → 24, at x=0 → 2 (the constant-term
    // law), at x=−1 → 3. A mutant that evaluates coefficients in
    // DESCENDING order fails the value set.
    let p = Value::Vector(vec![2.0, 3.0, 4.0]);
    for (point, want) in [(2.0, 24.0), (0.0, 2.0), (-1.0, 3.0)] {
        let value = eval(
            vec![EmirOp::PolyEval(EmirValue(0), EmirValue(1))],
            &[p.clone(), Value::F64(point)],
        )
        .expect("poly eval computes");
        assert!(
            (f64_of(&value) - want).abs() < 1e-12,
            "p({point}) = {want}, got {value:?}"
        );
    }
}

#[test]
fn non_finite_polynomial_coefficient_refuses_typed() {
    // E-POLY-001: a NaN coefficient must never silently propagate
    // through the convolution (the negative seed's shape).
    let nan_poly = Value::Vector(vec![1.0, f64::NAN]);
    let error = eval(
        vec![EmirOp::PolyMul(EmirValue(0), EmirValue(1))],
        &[nan_poly, Value::Vector(vec![1.0])],
    )
    .expect_err("non-finite coefficient refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-POLY-001"),
        "non-finite coefficients must name E-POLY-001, got {fault}"
    );
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/polynomial_domain.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-POLY-001"),
        "seed expects the non-finite refusal, found: {expect_line}"
    );
}

#[test]
fn non_finite_polynomial_point_refuses_typed() {
    // E-POLY-002: a NaN evaluation point refuses — never a silent NaN
    // result masquerading as a value.
    let error = eval(
        vec![EmirOp::PolyEval(EmirValue(0), EmirValue(1))],
        &[Value::Vector(vec![1.0, 2.0]), Value::F64(f64::NAN)],
    )
    .expect_err("non-finite point refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-POLY-002"),
        "non-finite point must name E-POLY-002, got {fault}"
    );
}

#[test]
fn polynomial_addition_binds_generic_vector_add() {
    // poly_add is a NAME BINDING to the existing VectorAdd op (the
    // 4wj0 norm precedent): coefficientwise addition, no new op. The
    // short-carry law: (1 + 2x) + (3 − 2x) = 4 + 0x.
    let sum = eval(
        vec![EmirOp::VectorAdd(EmirValue(0), EmirValue(1))],
        &[Value::Vector(vec![1.0, 2.0]), Value::Vector(vec![3.0, -2.0])],
    )
    .expect("poly add computes");
    assert_eq!(vector_of(&sum), vec![4.0, 0.0]);
}

#[test]
fn polynomial_registry_cells_enforce_shape_law() {
    // std.poly.mul / std.poly.eval: the same kernels as registry DATA
    // (the anti-LOC law). A MATRIX in a coefficient slot refuses at
    // COMPILE (the closed vocabulary's shape law).
    let registry = std_cell_registry();
    assert!(registry.contains_key("std.poly.mul"), "mul cell registered");
    assert!(registry.contains_key("std.poly.eval"), "eval cell registered");
    let product = cell_seval(
        "std.poly.mul",
        "poly_mul",
        2,
        vec![
            ("a".to_string(), ParamShape::Vector),
            ("b".to_string(), ParamShape::Vector),
        ],
        &[Value::Vector(vec![1.0, 1.0]), Value::Vector(vec![1.0, 1.0])],
    )
    .expect("registry poly mul computes");
    assert_eq!(vector_of(&product), vec![1.0, 2.0, 1.0]);
    // Shape law at compile: a matrix in the coefficient slot.
    let term = Term::Apply {
        operator: SymbolId("poly_mul".into()),
        arguments: vec![
            Term::Variable(VariableId("a".into())),
            Term::Variable(VariableId("b".into())),
        ],
    };
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("poly_mul".into()), 2)
        .expect("signature is conflict-free");
    let error = compile_reference(
        &term,
        &signature,
        &[
            ("a".to_string(), ParamShape::Matrix),
            ("b".to_string(), ParamShape::Vector),
        ],
        Vec::new(),
        "surface.shape-law-poly",
    )
    .expect_err("a matrix in the coefficient slot refuses at compile");
    assert!(
        matches!(error, TermCompileError::ShapeMismatch { .. }),
        "shape law must refuse, got {error:?}"
    );
}
