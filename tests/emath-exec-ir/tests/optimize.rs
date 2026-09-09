//! Optimizer conformance under the final no-speculation contract.
//!
//! The universal machine performs no speculative rewrites: authored
//! reference bytecode and kernel bindings are the only authority
//! (crates/emath-exec-ir/src/optimize.rs). `optimize_program` must
//! therefore leave every program structurally unchanged and preserve
//! evaluation results bit-exactly, strict eager fault semantics included
//! (unused faulting ops are never dropped). The retired constant-folding
//! and dead-register-elimination behavior is legacy; these tests pin the
//! contract that replaced it.

use std::path::Path;

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::install_language_distribution;
use emath_exec_ir::optimize::optimize_program;
use emath_exec_ir::{BuiltinId, CellClass, EmirOp, EmirProgram, EmirValue};

use emath_test_harness::Probe;

fn program(ops: Vec<EmirOp>) -> EmirProgram {
    let last = u32::try_from(ops.len().saturating_sub(1)).unwrap_or(0);
    EmirProgram {
        ops: ops.into_iter().map(|op| (op, Span::default())).collect(),
        result: EmirValue(last),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

fn c(value: f64) -> EmirOp {
    EmirOp::ConstF64(value.to_bits())
}

/// Executing through the capsule seam requires the checked-in distribution:
/// capability FeatureIDs resolve to public kernel ABI bindings only after
/// `install_language_distribution` (no injection API, no static table).
fn install_checked_in_distribution() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = load_language_distribution(&root).expect("load capsule distribution");
    install_language_distribution(&distribution).expect("install capsule-active kernels");
}

/// The clean caller shape: one `ApplyCapability` naming the real capsule
/// FeatureID; no retired domain op, no handwritten dispatch.
fn capability(name: &str, args: Vec<EmirValue>) -> EmirOp {
    EmirOp::ApplyCapability {
        capability: name.to_string(),
        class: CellClass::Pure,
        args,
    }
}

fn derivative_program(body: EmirProgram, reverse: bool) -> EmirProgram {
    install_checked_in_distribution();
    let mut ops = vec![
        EmirOp::ProgramLiteral { body, captures: Vec::new(), vector_input: false },
        EmirOp::LoadInput(0),
        EmirOp::VectorCreate(vec![EmirValue(1)]),
    ];
    if reverse {
        ops.push(c(0.0));
        ops.push(EmirOp::VectorCreate(vec![EmirValue(3)]));
        ops.push(capability(
            "std.capability.calculus.reverse-gradient",
            vec![EmirValue(0), EmirValue(2), EmirValue(4)],
        ));
    } else {
        ops.push(EmirOp::ConstI64(0));
        ops.push(capability(
            "std.capability.calculus.forward-difference",
            vec![EmirValue(0), EmirValue(2), EmirValue(3)],
        ));
    }
    let mut program = program(ops);
    program.input_count = 1;
    program
}

/// Constant arithmetic chains are NOT folded (no speculative rewrites);
/// the optimizer leaves the program untouched and evaluation stays
/// bit-identical to the original program.

/// Division by zero folds to ±inf exactly like evaluating the op, and
/// `ln` of a negative folds to NaN (no faults; IEEE semantics, bit
/// identical to the unfolded evaluation).

/// Dead chains are NOT eliminated: the no-speculation optimizer keeps
/// every register, and evaluation agrees with the original for any inputs.

/// Strict eager semantics: an op whose result is unused but which can
/// fault at runtime (factorial of a negative through the
/// `std.capability.exact.factorial` capsule seam) is preserved, and its
/// operands stay alive, so the program still faults.

/// Nested authored programs remain opaque, and the outer capability evaluates
/// identically across the optimizer boundary.

/// Fold/Select-free programs combining all of the above: optimized and
/// original agree across a range of inputs including NaN/inf.

fn values_equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        // Any NaN matches any NaN (payloads may differ); otherwise bit-exact
        // so signed-zero fold/interp divergence is not IEEE-hidden.
        (Value::F64(x), Value::F64(y)) => (x.is_nan() && y.is_nan()) || x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

/// Comparisons over constants and `Select` stay unfolded under the
/// no-speculation contract, with interpreter-equal results.

/// Boolean op chains (And/Or/Not/Imply) over F64 constants are a strict
/// typed fault: `bool_of` admits only `Value::Bool`, so with no folding
/// anywhere both original and optimized fault identically.

/// `IsFinite` over constants stays unfolded: finite → true, infinities
/// and NaN → false, identically before and after the optimizer.

/// I64 add/mul stay unfolded (2^53+1 + 0 must stay exact through the
/// interpreter). Overflow is left unfolded so interp still faults.

/// Mixed I64×F64 `==`/`>` evaluate with exact compare, not `n as f64`,
/// and the optimizer must not rewrite them.

/// `min`/`max` ignore NaN (Rust/`minNum` semantics) and preserve signed
/// zero; the optimizer must not rewrite them, and evaluation stays
/// bit-identical.

/// `Select` over an F64 condition (NaN or -0) is a strict typed fault:
/// `bool_of` admits only `Value::Bool`. The legacy truthiness behavior
/// existed only in the retired folder; with no folding, original and
/// optimized fault identically. NaN comparisons remain IEEE (all
/// orderings false, `==` false, `!=` true) and unfoldable.

/// F64÷0 evaluates to ±Inf (IEEE, not a fault) and stays unfolded;
/// I64÷0 and non-exact I64÷I64 are checked exact-arithmetic faults in
/// the interpreter (legacy folding hid them behind folded constants);
/// exact I64÷I64 stays type-preserving. Bool never widens to F64.

/// Imply/Iff differentiation used to depend on folding to ConstBool
/// (dual-encodable) while the unfolded ops were unsupported in dual eval.
/// With no folding anywhere, both paths return tangent 0 through the
/// capsule seam.

/// Reverse-mode `/0` and `ln(-1)` used to disagree with folded constants
/// (fault vs Inf/NaN success). Under the no-speculation contract there is
/// only the interpreter path: IEEE Inf/NaN, zero gradient, success.

/// The optimizer never rewrites a typed fault: `And` over an I64 constant
/// is a `bool_of` type confusion in the interpreter, and the op must
/// survive — optimized and original fault identically.


/// assert_eq/assert_ne semantics over references: borrows both operands
/// (like the macros) and allows PartialEq between distinct types.
fn eq_ref<T: ?Sized + std::fmt::Debug, U: ?Sized + std::fmt::Debug>(
    ph: &mut Probe,
    name: impl Into<String>,
    actual: &T,
    expected: &U,
) where
    T: PartialEq<U>,
{
    ph.demand(name, actual == expected, format!("expected {expected:?}, got {actual:?}"));
}

fn ne_ref<T: ?Sized + std::fmt::Debug, U: ?Sized + std::fmt::Debug>(
    ph: &mut Probe,
    name: impl Into<String>,
    actual: &T,
    unexpected: &U,
) where
    T: PartialEq<U>,
{
    ph.demand(name, actual != unexpected, format!("got forbidden value {unexpected:?}"));
}

fn constant_arithmetic_collapses_bit_exactly(ph: &mut Probe) {
    ph.case("constant_arithmetic_collapses_bit_exactly", |ph| {
    let original = program(vec![
        c(2.0),
        c(3.0),
        EmirOp::F64Add(EmirValue(0), EmirValue(1)),
        c(10.0),
        EmirOp::F64Mul(EmirValue(2), EmirValue(3)),
        EmirOp::UnaryBuiltin(BuiltinId::Sin, EmirValue(4)),
    ]);
    let expected = evaluate(&original, &[], &[]).unwrap();

    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, format!(
        "no-speculation contract: constants must not be folded"
    ), &(
        optimized.ops.len()), &(
        original.ops.len()));
    eq_ref(ph, "2", &(evaluate(&optimized, &[], &[]).unwrap()), &( expected));

    });
}

fn ieee_edge_values_fold_identically(ph: &mut Probe) {
    ph.case("ieee_edge_values_fold_identically", |ph| {
    let original = program(vec![
        c(1.0),
        c(0.0),
        EmirOp::F64Div(EmirValue(0), EmirValue(1)),
    ]);
    let expected = evaluate(&original, &[], &[]).unwrap();
    let Value::F64(inf) = expected else {
        panic!("expected f64 result");
    };
    ph.demand("1", inf.is_infinite(), "assertion failed: inf.is_infinite()");

    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "2", &(evaluate(&optimized, &[], &[]).unwrap()), &( expected));

    let nan_source = program(vec![
        c(-1.0),
        EmirOp::UnaryBuiltin(BuiltinId::Ln, EmirValue(0)),
    ]);
    let expected = evaluate(&nan_source, &[], &[]).unwrap();
    let Value::F64(nan) = expected else {
        panic!("expected f64 result");
    };
    ph.demand("3", nan.is_nan(), "assertion failed: nan.is_nan()");
    let mut optimized = nan_source.clone();
    optimize_program(&mut optimized);
    ph.demand("4", matches!(evaluate(&optimized, &[], &[]).unwrap(), Value::F64(v) if v.is_nan()), "assertion failed: matches!(evaluate(&optimized, &[], &[]).unwrap(), Value::F64(v) if v.is_nan())");

    });
}

fn dead_registers_are_eliminated_and_renumbered(ph: &mut Probe) {
    ph.case("dead_registers_are_eliminated_and_renumbered", |ph| {
    // reg4 (sin of a const) is dead; c99 stays (used by the final add).
    let original = program(vec![
        EmirOp::LoadInput(0),
        c(3.0),
        EmirOp::F64Mul(EmirValue(0), EmirValue(1)),
        c(99.0),
        EmirOp::UnaryBuiltin(BuiltinId::Sin, EmirValue(3)),
        EmirOp::F64Add(EmirValue(2), EmirValue(3)),
    ]);
    for x in [0.0f64, 1.0, -2.5] {
        let inputs = vec![Value::F64(x)];
        let expected = evaluate(&original, &inputs, &[]).unwrap();

        let mut optimized = original.clone();
        optimize_program(&mut optimized);
        eq_ref(ph, format!(
            "no-speculation contract: dead registers must survive"
        ), &(
            optimized.ops.len()), &(
            original.ops.len()));
        eq_ref(ph, format!(
            "register numbering must be untouched"
        ), &(
            optimized.ops[0].0), &(
            EmirOp::LoadInput(0)));
        eq_ref(ph, "3", &(evaluate(&optimized, &inputs, &[]).unwrap()), &( expected));
    }

    });
}

fn unused_faulting_op_is_preserved(ph: &mut Probe) {
    ph.case("unused_faulting_op_is_preserved", |ph| {
    install_checked_in_distribution();
    let mut p = program(vec![
        EmirOp::ConstI64(-1),
        capability("std.capability.exact.factorial", vec![EmirValue(0)]),
        c(42.0),
    ]);
    optimize_program(&mut p);
    eq_ref(ph, format!( "factorial and its operand must survive DCE"), &(p.ops.len()), &( 3));
    let result = evaluate(&p, &[], &[]);
    ph.demand("2", 
        matches!(
            result,
            Err(EvalFault::CapabilityRefused { ref capability, .. })
                if capability == "std.capability.exact.factorial"
        ), format!(
        "strict eager evaluation must still fault through the capsule seam: {result:?}"
    ));

    // Same for a dynamic out-of-range load (input_count is 0).
    let mut q = program(vec![
        EmirOp::LoadInput(0),
        c(7.0),
        EmirOp::F64Add(EmirValue(0), EmirValue(1)),
    ]);
    optimize_program(&mut q);
    eq_ref(ph, format!( "out-of-range load must survive DCE"), &(q.ops.len()), &( 3));
    ph.demand("4", evaluate(&q, &[], &[]).is_err(), "assertion failed: evaluate(&q, &[], &[]).is_err()");

    });
}

fn nested_body_is_optimized(ph: &mut Probe) {
    ph.case("nested_body_is_optimized", |ph| {
    let body = EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()),
            (c(3.0), Span::default()),
            (c(99.0), Span::default()),
            (EmirOp::F64Mul(EmirValue(0), EmirValue(1)), Span::default()),
        ],
        result: EmirValue(3),
        input_count: 2,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let original = derivative_program(body, false);
    let expected = evaluate(&original, &[Value::F64(5.0)], &[]).unwrap();
    eq_ref(ph, format!( "d/dx 3x = 3"), &(expected), &( Value::F64(3.0)));

    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    let EmirOp::ProgramLiteral { body, .. } = &optimized.ops[0].0 else {
        panic!("program carrier must survive");
    };
    eq_ref(ph, format!(
        "the generic optimizer must not rewrite inside an authored program carrier"
    ), &(
        body.ops.len()), &(
        4));
    eq_ref(ph, "3", &(
        evaluate(&optimized, &[Value::F64(5.0)], &[]).unwrap()), &(
        expected
    ));

    });
}

fn mixed_program_matches_on_adversarial_inputs(ph: &mut Probe) {
    ph.case("mixed_program_matches_on_adversarial_inputs", |ph| {
    let original = program(vec![
        EmirOp::LoadInput(0),
        EmirOp::LoadInput(1),
        EmirOp::F64Add(EmirValue(0), EmirValue(1)),
        c(2.5),
        EmirOp::F64Mul(EmirValue(2), EmirValue(3)),
        EmirOp::UnaryBuiltin(BuiltinId::Tanh, EmirValue(2)),
        EmirOp::F64Sub(EmirValue(4), EmirValue(5)),
        EmirOp::BinaryBuiltin(BuiltinId::Hypot, EmirValue(6), EmirValue(1)),
    ]);
    let cases: Vec<Vec<Value>> = [
        vec![1.0, 2.0],
        vec![-3.5, 0.0],
        vec![f64::NAN, 1.0],
        vec![f64::INFINITY, -f64::INFINITY],
        vec![0.0, 0.0],
    ]
    .into_iter()
    .map(|pair| pair.into_iter().map(Value::F64).collect())
    .collect();

    let mut optimized = original.clone();
    optimize_program(&mut optimized);

    for inputs in &cases {
        let before = evaluate(&original, inputs, &[]);
        let after = evaluate(&optimized, inputs, &[]);
        match (&before, &after) {
            (Ok(a), Ok(b)) => {
                // NaN payloads may differ; compare NaN-ness, else exactly.
                ph.demand("1", 
                    values_equivalent(a, b), format!(
                    "mismatch for {inputs:?}: {a:?} vs {b:?}"
                ));
            }
            _ => { eq_ref(ph, "2", &(
                after.map(|v| format!("{v:?}"))), &(
                before.map(|v| format!("{v:?}"))
            )); },
        }
    }

    });
}

fn comparisons_and_select_fold_to_const_bool(ph: &mut Probe) {
    ph.case("comparisons_and_select_fold_to_const_bool", |ph| {
    let original = program(vec![
        c(2.0),
        c(3.0),
        EmirOp::Lt(EmirValue(0), EmirValue(1)),
        c(7.0),
        EmirOp::Select {
            condition: EmirValue(2),
            then_value: EmirValue(0),
            else_value: EmirValue(3),
        },
    ]);
    let expected = evaluate(&original, &[], &[]).unwrap();
    eq_ref(ph, "1", &(expected), &( Value::F64(2.0)));

    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    // No folding: Lt and Select survive untouched, evaluation matches.
    eq_ref(ph, format!(
        "no-speculation contract: Select must not fold"
    ), &(
        optimized.ops.len()), &(
        original.ops.len()));
    eq_ref(ph, "3", &(evaluate(&optimized, &[], &[]).unwrap()), &( expected));

    });
}

fn boolean_chains_fold_to_const_bool(ph: &mut Probe) {
    ph.case("boolean_chains_fold_to_const_bool", |ph| {
    let original = program(vec![
        c(1.0),
        c(0.0),
        EmirOp::Or(EmirValue(0), EmirValue(1)),
        EmirOp::Not(EmirValue(2)),
        EmirOp::And(EmirValue(2), EmirValue(0)),
        EmirOp::Imply(EmirValue(1), EmirValue(0)),
    ]);
    let expected = evaluate(&original, &[], &[]);
    ph.demand("1", 
        matches!(expected, Err(EvalFault::TypeConfusion { op: "or", .. })), format!(
        "strict bool_of must refuse F64 operands: {expected:?}"
    ));

    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, format!(
        "no-speculation contract: boolean chains must not fold"
    ), &(
        optimized.ops.len()), &(
        original.ops.len()));
    eq_ref(ph, format!(
        "optimized program must fault identically"
    ), &(
        evaluate(&optimized, &[], &[])), &(
        expected));

    });
}

fn is_finite_folds(ph: &mut Probe) {
    ph.case("is_finite_folds", |ph| {
    for value in [3.0, f64::INFINITY, f64::NAN] {
        let original = program(vec![c(value), EmirOp::IsFinite(EmirValue(0))]);
        let expected = evaluate(&original, &[], &[]).unwrap();
        let mut optimized = original.clone();
        optimize_program(&mut optimized);
        eq_ref(ph, format!(
            "no-speculation contract: IsFinite must not fold for {value}"
        ), &(
            optimized.ops.len()), &(
            original.ops.len()));
        eq_ref(ph, "2", &(evaluate(&optimized, &[], &[]).unwrap()), &( expected));
    }

    });
}

fn i64_arithmetic_folds_exactly(ph: &mut Probe) {
    ph.case("i64_arithmetic_folds_exactly", |ph| {
    let a = (1i64 << 53) + 1;
    let original = program(vec![
        EmirOp::ConstI64(a),
        EmirOp::ConstI64(0),
        EmirOp::F64Add(EmirValue(0), EmirValue(1)),
    ]);
    let expected = evaluate(&original, &[], &[]).unwrap();
    eq_ref(ph, "1", &(expected), &( Value::I64(a)));
    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, format!(
        "no-speculation contract: i64 add must not fold"
    ), &(
        optimized.ops.len()), &(
        original.ops.len()));
    eq_ref(ph, "3", &(evaluate(&optimized, &[], &[]).unwrap()), &( expected));

    let overflow = program(vec![
        EmirOp::ConstI64(i64::MAX),
        EmirOp::ConstI64(1),
        EmirOp::F64Add(EmirValue(0), EmirValue(1)),
    ]);
    let mut optimized = overflow.clone();
    optimize_program(&mut optimized);
    ph.demand("4", 
        matches!(
            optimized.ops.last().map(|(op, _)| op),
            Some(EmirOp::F64Add(..))
        ), format!(
        "overflowing i64 add must stay unfolded, got {:?}", 
        optimized.ops.last()
    ));
    ph.demand("5", evaluate(&optimized, &[], &[]).is_err(), "assertion failed: evaluate(&optimized, &[], &[]).is_err()");

    });
}

fn mixed_i64_f64_eq_folds_exactly(ph: &mut Probe) {
    ph.case("mixed_i64_f64_eq_folds_exactly", |ph| {
    let past = (1i64 << 53) + 1;
    let two53 = (1i64 << 53) as f64;
    let original = program(vec![
        EmirOp::ConstI64(past),
        c(two53),
        EmirOp::Eq(EmirValue(0), EmirValue(1)),
    ]);
    let expected = evaluate(&original, &[], &[]).unwrap();
    eq_ref(ph, "1", &(expected), &( Value::Bool(false)));
    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, format!(
        "no-speculation contract: mixed eq must not fold"
    ), &(
        optimized.ops.len()), &(
        original.ops.len()));
    eq_ref(ph, "3", &(evaluate(&optimized, &[], &[]).unwrap()), &( expected));

    let gt = program(vec![
        EmirOp::ConstI64(past),
        c(two53),
        EmirOp::Gt(EmirValue(0), EmirValue(1)),
    ]);
    let expected = evaluate(&gt, &[], &[]).unwrap();
    eq_ref(ph, "4", &(expected), &( Value::Bool(true)));
    let mut optimized = gt.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "5", &(optimized.ops.len()), &( gt.ops.len()));
    eq_ref(ph, "6", &(evaluate(&optimized, &[], &[]).unwrap()), &( expected));

    let z = program(vec![
        EmirOp::ConstI64(0),
        c(-0.0),
        EmirOp::Eq(EmirValue(0), EmirValue(1)),
    ]);
    let expected = evaluate(&z, &[], &[]).unwrap();
    eq_ref(ph, "7", &(expected), &( Value::Bool(true)));
    let mut optimized = z.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "8", &(optimized.ops.len()), &( z.ops.len()));
    eq_ref(ph, "9", &(evaluate(&optimized, &[], &[]).unwrap()), &( expected));

    });
}

fn min_max_nan_and_signed_zero_fold_identically(ph: &mut Probe) {
    ph.case("min_max_nan_and_signed_zero_fold_identically", |ph| {
    let cases: &[(f64, f64, BuiltinId)] = &[
        (f64::NAN, 1.0, BuiltinId::Min),
        (1.0, f64::NAN, BuiltinId::Min),
        (f64::NAN, f64::NAN, BuiltinId::Min),
        (f64::NAN, 1.0, BuiltinId::Max),
        (1.0, f64::NAN, BuiltinId::Max),
        (-0.0, 0.0, BuiltinId::Min),
        (0.0, -0.0, BuiltinId::Min),
        (-0.0, 0.0, BuiltinId::Max),
        (0.0, -0.0, BuiltinId::Max),
    ];
    for &(a, b, id) in cases {
        let original = program(vec![
            c(a),
            c(b),
            EmirOp::BinaryBuiltin(id, EmirValue(0), EmirValue(1)),
        ]);
        let expected = evaluate(&original, &[], &[]).unwrap();
        let mut optimized = original.clone();
        optimize_program(&mut optimized);
        eq_ref(ph, format!(
            "no-speculation contract: {id:?}({a:?},{b:?}) must not fold"
        ), &(
            optimized.ops.len()), &(
            original.ops.len()));
        eq_ref(ph, format!(
            "optimizer/interp mismatch for {id:?}({a:?},{b:?})"
        ), &(
            evaluate(&optimized, &[], &[]).unwrap()), &(
            expected));
    }

    });
}

fn nan_and_neg_zero_select_and_cmp_fold(ph: &mut Probe) {
    ph.case("nan_and_neg_zero_select_and_cmp_fold", |ph| {
    let nan = f64::NAN;
    for condition in [nan, -0.0] {
        let original = program(vec![
            c(condition),
            c(1.0),
            c(2.0),
            EmirOp::Select {
                condition: EmirValue(0),
                then_value: EmirValue(1),
                else_value: EmirValue(2),
            },
        ]);
        let expected = evaluate(&original, &[], &[]);
        ph.demand("1", 
            matches!(expected, Err(EvalFault::TypeConfusion { op: "select", .. })), format!(
            "strict bool_of must refuse F64 select condition {condition:?}: {expected:?}"
        ));
        let mut optimized = original.clone();
        optimize_program(&mut optimized);
        eq_ref(ph, "2", &(optimized.ops.len()), &( original.ops.len()));
        eq_ref(ph, format!(
            "optimized program must fault identically"
        ), &(
            evaluate(&optimized, &[], &[])), &(
            expected));
    }

    for (op, want) in [
        (EmirOp::Lt(EmirValue(0), EmirValue(1)), false),
        (EmirOp::Le(EmirValue(0), EmirValue(1)), false),
        (EmirOp::Gt(EmirValue(0), EmirValue(1)), false),
        (EmirOp::Ge(EmirValue(0), EmirValue(1)), false),
        (EmirOp::Eq(EmirValue(0), EmirValue(1)), false),
        (EmirOp::Ne(EmirValue(0), EmirValue(1)), true),
        (EmirOp::Eq(EmirValue(0), EmirValue(0)), false),
        (EmirOp::Ne(EmirValue(0), EmirValue(0)), true),
    ] {
        let original = program(vec![c(nan), c(1.0), op]);
        let expected = evaluate(&original, &[], &[]).unwrap();
        eq_ref(ph, "4", &(expected), &( Value::Bool(want)));
        let mut optimized = original.clone();
        optimize_program(&mut optimized);
        eq_ref(ph, format!(
            "no-speculation contract: NaN comparisons must not fold"
        ), &(
            optimized.ops.len()), &(
            original.ops.len()));
        eq_ref(ph, "6", &(evaluate(&optimized, &[], &[]).unwrap()), &( expected));
    }

    });
}

fn div_by_zero_and_kind_widening(ph: &mut Probe) {
    ph.case("div_by_zero_and_kind_widening", |ph| {
    let original = program(vec![
        c(1.0),
        c(0.0),
        EmirOp::F64Div(EmirValue(0), EmirValue(1)),
    ]);
    let expected = evaluate(&original, &[], &[]).unwrap();
    ph.demand("1", matches!(expected, Value::F64(v) if v.is_infinite() && v.is_sign_positive()), "assertion failed: matches!(expected, Value::F64(v) if v.is_infinite() && v.is_sign_positive())");
    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "2", &(evaluate(&optimized, &[], &[]).unwrap()), &( expected));
    ph.demand("3", 
        matches!(optimized.ops.last().map(|(op, _)| op), Some(EmirOp::F64Div(..))), format!(
        "no-speculation contract: division must not fold"
    ));

    // Checked I64 arithmetic: 1/0 faults, identically before and after
    // the no-speculation optimizer.
    let original = program(vec![
        EmirOp::ConstI64(1),
        EmirOp::ConstI64(0),
        EmirOp::F64Div(EmirValue(0), EmirValue(1)),
    ]);
    let expected = evaluate(&original, &[], &[]);
    ph.demand("4", 
        matches!(expected, Err(EvalFault::Arithmetic { .. })), format!(
        "checked i64 div-by-zero must fault: {expected:?}"
    ));
    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "5", &(evaluate(&optimized, &[], &[])), &( expected));

    // Exact I64÷I64 stays type-preserving (6/2 = I64 3); a non-exact
    // quotient (7/2) is a checked refusal, never a silent F64 widening.
    let original = program(vec![
        EmirOp::ConstI64(6),
        EmirOp::ConstI64(2),
        EmirOp::F64Div(EmirValue(0), EmirValue(1)),
    ]);
    let expected = evaluate(&original, &[], &[]).unwrap();
    eq_ref(ph, "6", &(expected), &( Value::I64(3)));
    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "7", &(evaluate(&optimized, &[], &[]).unwrap()), &( expected));

    let original = program(vec![
        EmirOp::ConstI64(7),
        EmirOp::ConstI64(2),
        EmirOp::F64Div(EmirValue(0), EmirValue(1)),
    ]);
    let expected = evaluate(&original, &[], &[]);
    ph.demand("8", 
        matches!(expected, Err(EvalFault::Arithmetic { .. })), format!(
        "non-exact i64 division must refuse: {expected:?}"
    ));
    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "9", &(evaluate(&optimized, &[], &[])), &( expected));

    // Mixed I64/F64 arithmetic is a strict type confusion (never a
    // silent coercion), identical before and after the optimizer.
    let original = program(vec![
        EmirOp::ConstI64(7),
        c(2.0),
        EmirOp::F64Div(EmirValue(0), EmirValue(1)),
    ]);
    let expected = evaluate(&original, &[], &[]);
    ph.demand("10", 
        matches!(expected, Err(EvalFault::TypeConfusion { .. })), format!(
        "mixed i64/f64 arithmetic must refuse: {expected:?}"
    ));
    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "11", &(evaluate(&optimized, &[], &[])), &( expected));

    // Comparison → ConstBool, then F64Add must still type-fault, not
    // widen true to 1.0.
    let p = program(vec![
        c(2.0),
        c(3.0),
        EmirOp::Lt(EmirValue(0), EmirValue(1)),
        c(1.0),
        EmirOp::F64Add(EmirValue(2), EmirValue(3)),
    ]);
    let mut optimized = p.clone();
    optimize_program(&mut optimized);
    ph.demand("12", evaluate(&p, &[], &[]).is_err(), "assertion failed: evaluate(&p, &[], &[]).is_err()");
    ph.demand("13", evaluate(&optimized, &[], &[]).is_err(), "assertion failed: evaluate(&optimized, &[], &[]).is_err()");

    });
}

fn imply_iff_differentiate_agrees_after_fold(ph: &mut Probe) {
    ph.case("imply_iff_differentiate_agrees_after_fold", |ph| {
    let imply_body = EmirProgram {
        ops: vec![
            (c(1.0), Span::default()),
            (c(1.0), Span::default()),
            (EmirOp::Imply(EmirValue(0), EmirValue(1)), Span::default()),
        ],
        result: EmirValue(2),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let original = derivative_program(imply_body, false);
    let expected = evaluate(&original, &[Value::F64(3.0)], &[]).unwrap();
    eq_ref(ph, "1", &(expected), &( Value::F64(0.0)));
    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "2", &(
        evaluate(&optimized, &[Value::F64(3.0)], &[]).unwrap()), &(
        expected
    ));

    let iff_body = EmirProgram {
        ops: vec![
            (c(0.0), Span::default()),
            (c(2.0), Span::default()),
            (EmirOp::Iff(EmirValue(0), EmirValue(1)), Span::default()),
        ],
        result: EmirValue(2),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let original = derivative_program(iff_body, false);
    let expected = evaluate(&original, &[Value::F64(3.0)], &[]).unwrap();
    eq_ref(ph, "3", &(expected), &( Value::F64(0.0)));
    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "4", &(
        evaluate(&optimized, &[Value::F64(3.0)], &[]).unwrap()), &(
        expected
    ));

    });
}

fn reverse_div0_and_ln_neg_agree_after_fold(ph: &mut Probe) {
    ph.case("reverse_div0_and_ln_neg_agree_after_fold", |ph| {
    let div_body = EmirProgram {
        ops: vec![
            (c(1.0), Span::default()),
            (c(0.0), Span::default()),
            (EmirOp::F64Div(EmirValue(0), EmirValue(1)), Span::default()),
        ],
        result: EmirValue(2),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let original = derivative_program(div_body, true);
    let expected = evaluate(&original, &[Value::F64(4.0)], &[]).unwrap();
    eq_ref(ph, "1", &(expected), &( Value::Vector(vec![0.0])));
    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "2", &(
        evaluate(&optimized, &[Value::F64(4.0)], &[]).unwrap()), &(
        expected
    ));

    let ln_body = EmirProgram {
        ops: vec![
            (c(-1.0), Span::default()),
            (
                EmirOp::UnaryBuiltin(BuiltinId::Ln, EmirValue(0)),
                Span::default(),
            ),
        ],
        result: EmirValue(1),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let original = derivative_program(ln_body, true);
    let expected = evaluate(&original, &[Value::F64(4.0)], &[]).unwrap();
    eq_ref(ph, "3", &(expected), &( Value::Vector(vec![0.0])));
    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    eq_ref(ph, "4", &(
        evaluate(&optimized, &[Value::F64(4.0)], &[]).unwrap()), &(
        expected
    ));

    });
}

fn bool_fold_preserves_typed_faults(ph: &mut Probe) {
    ph.case("bool_fold_preserves_typed_faults", |ph| {
    let p = program(vec![
        EmirOp::ConstI64(3),
        c(1.0),
        EmirOp::And(EmirValue(0), EmirValue(1)),
    ]);
    let mut optimized = p.clone();
    optimize_program(&mut optimized);
    // ConstI64 stays (no fold), And survives because folding would change
    // a typed fault into a value; DCE keeps it (fault-capable).
    eq_ref(ph, "1", &(optimized.ops.len()), &( 3));
    ph.demand("2", matches!(optimized.ops[2].0, EmirOp::And(..)), "assertion failed: matches!(optimized.ops[2].0, EmirOp::And(..))");
    ph.demand("3", evaluate(&optimized, &[], &[]).is_err(), "assertion failed: evaluate(&optimized, &[], &[]).is_err()");
    ph.demand("4", evaluate(&p, &[], &[]).is_err(), "assertion failed: evaluate(&p, &[], &[]).is_err()");

    });
}

#[test]
fn optimizer_contracts() {
    let mut ph = Probe::new("no-speculation optimizer leaves programs unchanged and evaluation bit-exact with strict eager faults preserved");
    constant_arithmetic_collapses_bit_exactly(&mut ph);
    ieee_edge_values_fold_identically(&mut ph);
    dead_registers_are_eliminated_and_renumbered(&mut ph);
    unused_faulting_op_is_preserved(&mut ph);
    nested_body_is_optimized(&mut ph);
    mixed_program_matches_on_adversarial_inputs(&mut ph);
    comparisons_and_select_fold_to_const_bool(&mut ph);
    boolean_chains_fold_to_const_bool(&mut ph);
    is_finite_folds(&mut ph);
    i64_arithmetic_folds_exactly(&mut ph);
    mixed_i64_f64_eq_folds_exactly(&mut ph);
    min_max_nan_and_signed_zero_fold_identically(&mut ph);
    nan_and_neg_zero_select_and_cmp_fold(&mut ph);
    div_by_zero_and_kind_widening(&mut ph);
    imply_iff_differentiate_agrees_after_fold(&mut ph);
    reverse_div0_and_ln_neg_agree_after_fold(&mut ph);
    bool_fold_preserves_typed_faults(&mut ph);
    ph.finish();
}
