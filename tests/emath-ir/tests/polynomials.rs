//! Polynomials (slice 1): the B28 compute layer —
//! polynomials as dense coefficient vectors.
//!
//! The law, sliced to the capability seam (the
//! FORMAL-vs-evaluated type split and `Lp<2, Omega, mu>` are the named
//! design deferrals — B28's type question and C10's value-generics):
//! - **Representation law**: a polynomial is a dense coefficient
//!   vector, ASCENDING order (index i = coefficient of xⁱ). The EMPTY
//!   vector is the zero polynomial (additive identity) — `poly_mul`
//!   with it yields empty, `poly_eval` of it is 0.0 (documented
//!   algebra, never a shape error).
//! - **Addition** is coefficientwise: `poly_add` binds to the generic
//!   dense vector-add capability `std.capability.linear.vector-add`
//!   (the norm precedent — a name binding, zero new op).
//! - **Multiplication** is the Cauchy convolution
//!   (`std.capability.poly.mul`, kernel `polynomial-multiply`):
//!   `c[i+j] += a[i]·b[j]` — deterministic ascending-index order.
//! - **Evaluation** is Horner (`std.capability.poly.eval`, kernel
//!   `polynomial-evaluate`): deterministic, strict-f64, one pass.
//! - Non-finite coefficients refuse `E-POLY-001`; a non-finite
//!   evaluation point refuses `E-POLY-002` — never a silently
//!   corrupted result (the negative seed's shape).

use std::path::{Path, PathBuf};

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::{KernelArity, install_language_distribution, native_kernel};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};

fn language_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

/// Capability cells resolve against the installed Language Image
/// (thread-local bindings); every evaluating test installs the
/// checked-in distribution first.
fn install_language() {
    let distribution = load_language_distribution(&language_root()).expect("language distribution");
    install_language_distribution(&distribution).expect("algebra carrier kernels install");
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

#[test]
fn polynomial_multiplication_returns_known_products() {
    install_language();
    // (1 + x)(1 + x) = 1 + 2x + x² and (1 + x)(1 − x) = 1 − x² — the
    // convolution law with exact integer-valued coefficients (no
    // floating-point slack to hide a wrong accumulation order).
    let one_plus_x = Value::Vector(vec![1.0, 1.0]);
    let product = eval(
        vec![cell(
            "std.capability.poly.mul",
            vec![EmirValue(0), EmirValue(1)],
        )],
        &[one_plus_x.clone(), one_plus_x.clone()],
    )
    .expect("poly mul computes");
    assert_eq!(vector_of(&product), vec![1.0, 2.0, 1.0]);
    let one_minus_x = Value::Vector(vec![1.0, -1.0]);
    let product = eval(
        vec![cell(
            "std.capability.poly.mul",
            vec![EmirValue(0), EmirValue(1)],
        )],
        &[one_plus_x, one_minus_x],
    )
    .expect("poly mul computes");
    assert_eq!(vector_of(&product), vec![1.0, 0.0, -1.0]);
}

#[test]
fn polynomial_multiplication_obeys_identity_and_zero_laws() {
    install_language();
    // [1] (the constant 1) is the multiplicative identity; the EMPTY
    // carrier is the zero polynomial (the documented representation
    // law): mul yields empty, eval yields 0.
    let p = Value::Vector(vec![2.0, 3.0]);
    let one = Value::Vector(vec![1.0]);
    let product = eval(
        vec![cell(
            "std.capability.poly.mul",
            vec![EmirValue(0), EmirValue(1)],
        )],
        &[p.clone(), one],
    )
    .expect("identity law");
    assert_eq!(vector_of(&product), vec![2.0, 3.0]);
    let zero = Value::Vector(vec![]);
    let product = eval(
        vec![cell(
            "std.capability.poly.mul",
            vec![EmirValue(0), EmirValue(1)],
        )],
        &[p.clone(), zero.clone()],
    )
    .expect("zero law");
    assert_eq!(vector_of(&product), Vec::<f64>::new());
    let value = eval(
        vec![cell(
            "std.capability.poly.eval",
            vec![EmirValue(0), EmirValue(1)],
        )],
        &[zero, Value::F64(5.0)],
    )
    .expect("zero polynomial evaluates to 0");
    assert_eq!(f64_of(&value), 0.0);
}

#[test]
fn horner_evaluation_returns_known_values() {
    install_language();
    // p = 2 + 3x + 4x²: at x=2 → 24, at x=0 → 2 (the constant-term
    // law), at x=−1 → 3. A mutant that evaluates coefficients in
    // DESCENDING order fails the value set.
    let p = Value::Vector(vec![2.0, 3.0, 4.0]);
    for (point, want) in [(2.0, 24.0), (0.0, 2.0), (-1.0, 3.0)] {
        let value = eval(
            vec![cell(
                "std.capability.poly.eval",
                vec![EmirValue(0), EmirValue(1)],
            )],
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
    install_language();
    // E-POLY-001: a NaN coefficient must never silently propagate
    // through the convolution (the negative seed's shape).
    let nan_poly = Value::Vector(vec![1.0, f64::NAN]);
    let error = eval(
        vec![cell(
            "std.capability.poly.mul",
            vec![EmirValue(0), EmirValue(1)],
        )],
        &[nan_poly, Value::Vector(vec![1.0])],
    )
    .expect_err("non-finite coefficient refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-POLY-001"),
        "non-finite coefficients must name E-POLY-001, got {fault}"
    );
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/polynomial_domain.emath");
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
    install_language();
    // E-POLY-002: a NaN evaluation point refuses — never a silent NaN
    // result masquerading as a value.
    let error = eval(
        vec![cell(
            "std.capability.poly.eval",
            vec![EmirValue(0), EmirValue(1)],
        )],
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
    install_language();
    // poly_add is a NAME BINDING to the generic dense vector-add
    // capability (the norm precedent): coefficientwise addition, no
    // new op. The short-carry law: (1 + 2x) + (3 − 2x) = 4 + 0x.
    let sum = eval(
        vec![cell(
            "std.capability.linear.vector-add",
            vec![EmirValue(0), EmirValue(1)],
        )],
        &[
            Value::Vector(vec![1.0, 2.0]),
            Value::Vector(vec![3.0, -2.0]),
        ],
    )
    .expect("poly add computes");
    assert_eq!(vector_of(&sum), vec![4.0, 0.0]);
}

#[test]
fn polynomial_capsules_bind_public_kernels_and_enforce_shape_law() {
    install_language();
    let multiply = native_kernel("std.capability.poly.mul").expect("multiply kernel bound");
    let evaluate = native_kernel("std.capability.poly.eval").expect("evaluate kernel bound");
    assert_eq!(multiply.kernel_id, "polynomial-multiply");
    assert_eq!(evaluate.kernel_id, "polynomial-evaluate");
    assert_eq!(multiply.arity_contract(), KernelArity::Exact(2));
    assert_eq!(evaluate.arity_contract(), KernelArity::Exact(2));

    let product = eval(
        vec![cell(
            "std.capability.poly.mul",
            vec![EmirValue(0), EmirValue(1)],
        )],
        &[Value::Vector(vec![1.0, 1.0]), Value::Vector(vec![1.0, 1.0])],
    )
    .expect("capsule poly mul computes");
    assert_eq!(vector_of(&product), vec![1.0, 2.0, 1.0]);

    let error = eval(
        vec![cell(
            "std.capability.poly.mul",
            vec![EmirValue(0), EmirValue(1)],
        )],
        &[
            Value::Matrix {
                rows: 1,
                cols: 1,
                data: vec![1.0],
            },
            Value::Vector(vec![1.0]),
        ],
    )
    .expect_err("a matrix in the coefficient slot refuses at the kernel ABI");
    assert!(
        matches!(error, EvalFault::CapabilityRefused { ref code, .. } if code.contains("E-TYPE-012")),
        "carrier shape law must refuse typed, got {error:?}"
    );
}
