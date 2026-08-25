//! Optimizer conformance: constant folding and dead-register elimination
//! preserve evaluation results bit-exactly, strict eager fault semantics
//! included (unused faulting ops are never dropped).

use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate};
use emath_exec_ir::optimize::optimize_program;
use emath_exec_ir::{BuiltinId, EmirOp, EmirProgram, EmirValue};

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

/// Constant arithmetic chains collapse to a single `ConstF64`, and the
/// folded value is bit-identical to evaluating the original program.
#[test]
fn constant_arithmetic_collapses_bit_exactly() {
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
    assert_eq!(optimized.ops.len(), 1, "expected a single folded constant");
    assert!(matches!(optimized.ops[0].0, EmirOp::ConstF64(_)));
    assert_eq!(evaluate(&optimized, &[], &[]).unwrap(), expected);
}

/// Division by zero folds to ±inf exactly like evaluating the op, and
/// `ln` of a negative folds to NaN (no faults; IEEE semantics, bit
/// identical to the unfolded evaluation).
#[test]
fn ieee_edge_values_fold_identically() {
    let original = program(vec![
        c(1.0),
        c(0.0),
        EmirOp::F64Div(EmirValue(0), EmirValue(1)),
    ]);
    let expected = evaluate(&original, &[], &[]).unwrap();
    let Value::F64(inf) = expected else {
        panic!("expected f64 result");
    };
    assert!(inf.is_infinite());

    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    assert_eq!(evaluate(&optimized, &[], &[]).unwrap(), expected);

    let nan_source = program(vec![
        c(-1.0),
        EmirOp::UnaryBuiltin(BuiltinId::Ln, EmirValue(0)),
    ]);
    let expected = evaluate(&nan_source, &[], &[]).unwrap();
    let Value::F64(nan) = expected else {
        panic!("expected f64 result");
    };
    assert!(nan.is_nan());
    let mut optimized = nan_source.clone();
    optimize_program(&mut optimized);
    assert!(matches!(evaluate(&optimized, &[], &[]).unwrap(), Value::F64(v) if v.is_nan()));
}

/// Dead chains are removed and surviving registers are renumbered, without
/// changing the result for any inputs.
#[test]
fn dead_registers_are_eliminated_and_renumbered() {
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
        // LoadInput, c3, Mul, c99, Add: the sin register is gone.
        assert_eq!(optimized.ops.len(), 5);
        assert_eq!(
            optimized.ops[0].0,
            EmirOp::LoadInput(0),
            "register numbering must be compacted in order"
        );
        assert_eq!(evaluate(&optimized, &inputs, &[]).unwrap(), expected);
    }
}

/// Strict eager semantics: an op whose result is unused but which can
/// fault at runtime (factorial of a negative) is preserved, and its
/// operands stay alive, so the program still faults.
#[test]
fn unused_faulting_op_is_preserved() {
    let mut p = program(vec![
        EmirOp::ConstI64(-1),
        EmirOp::Factorial(EmirValue(0)),
        c(42.0),
    ]);
    optimize_program(&mut p);
    assert_eq!(p.ops.len(), 3, "factorial and its operand must survive DCE");
    let result = evaluate(&p, &[], &[]);
    assert!(
        matches!(result, Err(EvalFault::Arithmetic { .. })),
        "strict eager evaluation must still fault"
    );

    // Same for a dynamic out-of-range load (input_count is 0).
    let mut q = program(vec![
        EmirOp::LoadInput(0),
        c(7.0),
        EmirOp::F64Add(EmirValue(0), EmirValue(1)),
    ]);
    optimize_program(&mut q);
    assert_eq!(q.ops.len(), 3, "out-of-range load must survive DCE");
    assert!(evaluate(&q, &[], &[]).is_err());
}

/// Nested sub-programs (solver/derivative bodies) are optimized too, and
/// the outer op still evaluates identically.
#[test]
fn nested_body_is_optimized() {
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
    let original = program(vec![EmirOp::Differentiate {
        body,
        var_index: 0,
    }]);
    let expected = evaluate(&original, &[Value::F64(5.0)], &[]).unwrap();
    assert_eq!(expected, Value::F64(3.0), "d/dx 3x = 3");

    let mut optimized = original.clone();
    optimize_program(&mut optimized);
    let EmirOp::Differentiate { body, .. } = &optimized.ops[0].0 else {
        panic!("outer op must survive");
    };
    assert_eq!(
        body.ops.len(),
        3,
        "dead constant inside the derivative body must be removed"
    );
    assert_eq!(evaluate(&optimized, &[Value::F64(5.0)], &[]).unwrap(), expected);
}

/// Fold/Select-free programs combining all of the above: optimized and
/// original agree across a range of inputs including NaN/inf.
#[test]
fn mixed_program_matches_on_adversarial_inputs() {
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
                assert!(
                    values_equivalent(a, b),
                    "mismatch for {inputs:?}: {a:?} vs {b:?}"
                );
            }
            _ => assert_eq!(after.map(|v| format!("{v:?}")), before.map(|v| format!("{v:?}"))),
        }
    }
}

fn values_equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::F64(x), Value::F64(y)) => x.is_nan() && y.is_nan() || x == y,
        _ => a == b,
    }
}
