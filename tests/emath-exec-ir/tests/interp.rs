use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate};
use emath_exec_ir::{BuiltinId, EdgePolicy, EmirOp, EmirProgram, EmirValue, FoldCombine};

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

fn const_bits(value: f64) -> EmirOp {
    EmirOp::ConstF64(value.to_bits())
}

#[test]
fn tensor_create_and_slice_spot() {
    let program = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        const_bits(3.0),
        const_bits(4.0),
        const_bits(5.0),
        const_bits(6.0),
        const_bits(7.0),
        const_bits(8.0),
        EmirOp::TensorCreate {
            shape: vec![2, 2, 2],
            elements: (0..8).map(EmirValue).collect(),
        },
        const_bits(0.0),
        const_bits(2.0),
        const_bits(1.0),
        EmirOp::TensorSlice {
            tensor: EmirValue(8),
            axes: vec![
                emath_exec_ir::EmirSliceAxis::Point(EmirValue(9)),
                emath_exec_ir::EmirSliceAxis::Range {
                    start: EmirValue(9),
                    end: EmirValue(10),
                },
                emath_exec_ir::EmirSliceAxis::Point(EmirValue(11)),
            ],
        },
    ]);
    assert_eq!(
        evaluate(&program, &[], &[]).unwrap(),
        Value::Vector(vec![2.0, 4.0])
    );
}

#[test]
fn add_spot() {
    let program = program(vec![
        const_bits(2.0),
        const_bits(3.0),
        EmirOp::F64Add(EmirValue(0), EmirValue(1)),
    ]);
    assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::F64(5.0));
}

/// I64 add/mul stay exact past 2^53 (where f64 rounding would break the
/// ring laws). Identity, commutativity, associativity, and order.
#[test]
fn i64_add_mul_ring_laws() {
    let a = (1i64 << 53) + 1; // 2^53+1, not an f64 integer
    let add = |x: i64, y: i64| {
        program(vec![
            EmirOp::ConstI64(x),
            EmirOp::ConstI64(y),
            EmirOp::F64Add(EmirValue(0), EmirValue(1)),
        ])
    };
    let mul = |x: i64, y: i64| {
        program(vec![
            EmirOp::ConstI64(x),
            EmirOp::ConstI64(y),
            EmirOp::F64Mul(EmirValue(0), EmirValue(1)),
        ])
    };
    let eval = |p: &EmirProgram| match evaluate(p, &[], &[]).unwrap() {
        Value::I64(v) => v,
        other => panic!("expected I64, got {other:?}"),
    };

    // x+0 = x = 0+x; x*1 = x = 1*x
    assert_eq!(eval(&add(a, 0)), a);
    assert_eq!(eval(&add(0, a)), a);
    assert_eq!(eval(&mul(a, 1)), a);
    assert_eq!(eval(&mul(1, a)), a);

    // a+b = b+a
    assert_eq!(eval(&add(a, 3)), eval(&add(3, a)));
    assert_eq!(eval(&mul(a, 3)), eval(&mul(3, a)));

    // (a+1)+1 = a+(1+1) — f64 would give 2^53 vs 2^53+2
    let left_assoc = program(vec![
        EmirOp::ConstI64(a),
        EmirOp::ConstI64(1),
        EmirOp::F64Add(EmirValue(0), EmirValue(1)),
        EmirOp::ConstI64(1),
        EmirOp::F64Add(EmirValue(2), EmirValue(3)),
    ]);
    let right_assoc = program(vec![
        EmirOp::ConstI64(a),
        EmirOp::ConstI64(1),
        EmirOp::ConstI64(1),
        EmirOp::F64Add(EmirValue(1), EmirValue(2)),
        EmirOp::F64Add(EmirValue(0), EmirValue(3)),
    ]);
    assert_eq!(eval(&left_assoc), a + 2);
    assert_eq!(eval(&right_assoc), a + 2);

    // 2^53+1 < 2^53+2 (both collapse to 2^53 as f64)
    let cmp = program(vec![
        EmirOp::ConstI64(a),
        EmirOp::ConstI64(a + 1),
        EmirOp::Lt(EmirValue(0), EmirValue(1)),
    ]);
    assert_eq!(evaluate(&cmp, &[], &[]).unwrap(), Value::Bool(true));

    // overflow is a fault, not wrap
    let overflow = program(vec![
        EmirOp::ConstI64(i64::MAX),
        EmirOp::ConstI64(1),
        EmirOp::F64Add(EmirValue(0), EmirValue(1)),
    ]);
    assert_eq!(
        evaluate(&overflow, &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "f64-add",
            detail: "i64 overflow",
        }
    );
}

/// Mixed Int/Float64 `==` used to widen (`n as f64 == x`), so 2^53+1
/// compared equal to 2^53.0 — a type-affinity false positive hiding true
/// divergence. Exact compare; IEEE signed-zero still equals integer 0.
#[test]
fn mixed_i64_f64_equality_is_exact() {
    let two53 = 1i64 << 53;
    let past = two53 + 1;
    let mixed = |n: i64, x: f64, op: fn(EmirValue, EmirValue) -> EmirOp| {
        program(vec![
            EmirOp::ConstI64(n),
            const_bits(x),
            op(EmirValue(0), EmirValue(1)),
        ])
    };
    let as_bool = |p: &EmirProgram| match evaluate(p, &[], &[]).unwrap() {
        Value::Bool(b) => b,
        other => panic!("expected Bool, got {other:?}"),
    };
    assert!(
        !as_bool(&mixed(past, two53 as f64, EmirOp::Eq)),
        "2^53+1 == 2^53.0 must be false (exact mixed compare)"
    );
    assert!(as_bool(&mixed(past, two53 as f64, EmirOp::Ne)));
    assert!(as_bool(&mixed(past, two53 as f64, EmirOp::Gt)));
    assert!(!as_bool(&mixed(past, two53 as f64, EmirOp::Lt)));
    assert!(as_bool(&mixed(0, -0.0, EmirOp::Eq)));
    assert!(as_bool(&mixed(8, 8.0, EmirOp::Eq)));
    let cx_eq = program(vec![
        EmirOp::ConstI64(past),
        EmirOp::ConstComplex(two53 as f64, 0.0),
        EmirOp::Eq(EmirValue(0), EmirValue(1)),
    ]);
    assert!(!as_bool(&cx_eq));
    let cx_zero = program(vec![
        EmirOp::ConstI64(0),
        EmirOp::ConstComplex(-0.0, -0.0),
        EmirOp::Eq(EmirValue(0), EmirValue(1)),
    ]);
    assert!(as_bool(&cx_zero));
    assert!(Value::I64(0) == Value::F64(-0.0));
    assert!(Value::I64(past) != Value::F64(two53 as f64));
    assert!(Value::I64(0) == Value::Complex { re: -0.0, im: -0.0 });
}

#[test]
fn pow_spot() {
    let program = program(vec![
        const_bits(2.0),
        const_bits(3.0),
        EmirOp::F64Pow(EmirValue(0), EmirValue(1)),
    ]);
    assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::F64(8.0));
}

#[test]
fn differentiate_pow_variable_exponent() {
    // d/dx[2^x] at x=3 = 2^3 * ln(2). Constant-base variable-exponent must
    // include the ln term; the constant-exponent-only rule yields 0 here.
    let body = EmirProgram {
        ops: vec![
            (const_bits(2.0), Span::default()),
            (EmirOp::LoadInput(0), Span::default()),
            (EmirOp::F64Pow(EmirValue(0), EmirValue(1)), Span::default()),
        ],
        result: EmirValue(2),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = EmirProgram {
        ops: vec![(
            EmirOp::Differentiate { body, var_index: 0 },
            Span::default(),
        )],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let got = match evaluate(&prog, &[Value::F64(3.0)], &[]).unwrap() {
        Value::F64(value) => value,
        other => panic!("expected F64, got {other:?}"),
    };
    let expected = 8.0 * 2.0_f64.ln();
    assert!(
        (got - expected).abs() < 1e-12,
        "got={got} expected={expected}"
    );
}

#[test]
fn differentiate_pow_constant_exponent() {
    // d/dx[x^3] at x=2 = 3*2^2 = 12 (constant-exponent fast path).
    let body = EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()),
            (const_bits(3.0), Span::default()),
            (EmirOp::F64Pow(EmirValue(0), EmirValue(1)), Span::default()),
        ],
        result: EmirValue(2),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = EmirProgram {
        ops: vec![(
            EmirOp::Differentiate { body, var_index: 0 },
            Span::default(),
        )],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let got = match evaluate(&prog, &[Value::F64(2.0)], &[]).unwrap() {
        Value::F64(value) => value,
        other => panic!("expected F64, got {other:?}"),
    };
    assert!((got - 12.0).abs() < 1e-12, "got={got}");
}

#[test]
fn select_spot() {
    let program = program(vec![
        const_bits(1.0),
        const_bits(0.0),
        const_bits(2.0),
        const_bits(1.0),
        EmirOp::Gt(EmirValue(0), EmirValue(1)),
        EmirOp::Select {
            condition: EmirValue(4),
            then_value: EmirValue(2),
            else_value: EmirValue(3),
        },
    ]);
    assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::F64(2.0));
}

#[test]
fn is_finite_spot() {
    let prog = program(vec![const_bits(1.0), EmirOp::IsFinite(EmirValue(0))]);
    assert_eq!(evaluate(&prog, &[], &[]).unwrap(), Value::Bool(true));
    for (value, want) in [
        (f64::INFINITY, false),
        (f64::NEG_INFINITY, false),
        (f64::NAN, false),
        (f64::from_bits(1), true), // subnormal
    ] {
        let prog = program(vec![
            EmirOp::ConstF64(value.to_bits()),
            EmirOp::IsFinite(EmirValue(0)),
        ]);
        assert_eq!(
            evaluate(&prog, &[], &[]).unwrap(),
            Value::Bool(want),
            "is_finite({value:?})"
        );
    }
}

#[test]
fn zero_div_zero_is_nan() {
    let program = program(vec![
        const_bits(0.0),
        const_bits(0.0),
        EmirOp::F64Div(EmirValue(0), EmirValue(1)),
    ]);
    match evaluate(&program, &[], &[]).unwrap() {
        Value::F64(value) => assert!(value.is_nan(), "0/0 must be NaN, got {value}"),
        other => panic!("expected NaN, got {other:?}"),
    }
}

#[test]
fn subnormal_arithmetic_is_not_flushed() {
    let tiny = f64::from_bits(1);
    assert!(tiny.is_subnormal());
    let program = program(vec![
        EmirOp::ConstF64(tiny.to_bits()),
        EmirOp::ConstF64(tiny.to_bits()),
        EmirOp::F64Add(EmirValue(0), EmirValue(1)),
    ]);
    match evaluate(&program, &[], &[]).unwrap() {
        Value::F64(value) => {
            assert_eq!(
                value.to_bits(),
                2,
                "subnormal+subnormal must not flush to 0"
            );
            assert!(value.is_subnormal());
        }
        other => panic!("expected subnormal f64, got {other:?}"),
    }
}

#[test]
fn div_by_zero_is_inf() {
    let program = program(vec![
        const_bits(1.0),
        const_bits(0.0),
        EmirOp::F64Div(EmirValue(0), EmirValue(1)),
    ]);
    match evaluate(&program, &[], &[]).unwrap() {
        Value::F64(value) => assert!(value.is_infinite() && value.is_sign_positive()),
        other => panic!("expected +inf, got {other:?}"),
    }
}

#[test]
fn eq_nan_is_false() {
    let nan = f64::NAN.to_bits();
    let program = program(vec![
        EmirOp::ConstF64(nan),
        EmirOp::ConstF64(nan),
        EmirOp::Eq(EmirValue(0), EmirValue(1)),
    ]);
    assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::Bool(false));
}

#[test]
fn type_confusion_and_on_vector() {
    // Bool operands take truthy coercion from scalars (F64/I64), matching
    // the Rust backend; non-scalar operands are type confusion.
    let program = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1)]),
        const_bits(0.0),
        const_bits(4.0),
        EmirOp::VectorCreate(vec![EmirValue(3), EmirValue(4)]),
        EmirOp::And(EmirValue(2), EmirValue(5)),
    ]);
    assert_eq!(
        evaluate(&program, &[], &[]).unwrap_err(),
        EvalFault::TypeConfusion {
            register: 2,
            op: "and",
        }
    );
}

#[test]
fn vector_and_matrix_ops_spot() {
    // [1.0, 2.0] + [3.0, 4.0] = [4.0, 6.0]
    let v1_ops = vec![
        const_bits(1.0),
        const_bits(2.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1)]),
        const_bits(3.0),
        const_bits(4.0),
        EmirOp::VectorCreate(vec![EmirValue(3), EmirValue(4)]),
        EmirOp::VectorAdd(EmirValue(2), EmirValue(5)),
    ];
    let prog = program(v1_ops);
    assert_eq!(
        evaluate(&prog, &[], &[]).unwrap(),
        Value::Vector(vec![4.0, 6.0])
    );

    // Matrix mul vector: [[1, 2], [3, 4]] * [2, 1] = [4, 10]
    let mv_ops = vec![
        const_bits(1.0),
        const_bits(2.0),
        const_bits(3.0),
        const_bits(4.0),
        EmirOp::MatrixCreate {
            rows: 2,
            cols: 2,
            elements: vec![EmirValue(0), EmirValue(1), EmirValue(2), EmirValue(3)],
        },
        const_bits(2.0),
        const_bits(1.0),
        EmirOp::VectorCreate(vec![EmirValue(5), EmirValue(6)]),
        EmirOp::MatrixMulVector(EmirValue(4), EmirValue(7)),
    ];
    let prog2 = program(mv_ops);
    assert_eq!(
        evaluate(&prog2, &[], &[]).unwrap(),
        Value::Vector(vec![4.0, 10.0])
    );
}

#[test]
fn vector_index_out_of_bounds_is_a_fault() {
    let program = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1)]),
        const_bits(2.0),
        EmirOp::VectorIndex {
            vector: EmirValue(2),
            index: EmirValue(3),
        },
    ]);
    assert_eq!(
        evaluate(&program, &[], &[]).unwrap_err(),
        EvalFault::IndexOutOfBounds {
            op: "vec-index",
            index: 2,
            len: 2,
        }
    );
}

#[test]
fn vector_negative_index_is_a_fault() {
    let program = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1)]),
        const_bits(-1.0),
        EmirOp::VectorIndex {
            vector: EmirValue(2),
            index: EmirValue(3),
        },
    ]);
    assert_eq!(
        evaluate(&program, &[], &[]).unwrap_err(),
        EvalFault::IndexOutOfBounds {
            op: "vec-index",
            index: -1,
            len: 2,
        }
    );
}

#[test]
fn fold_rejects_non_whole_bounds() {
    let body = EmirProgram {
        ops: vec![(EmirOp::LoadInput(0), Span::default())],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let nan_start = program(vec![
        const_bits(f64::NAN),
        const_bits(3.0),
        EmirOp::ConstI64(0),
        EmirOp::Fold {
            start: EmirValue(0),
            end: EmirValue(1),
            init: EmirValue(2),
            combine: FoldCombine::Add,
            loop_var_index: 0,
            body: body.clone(),
        },
    ]);
    assert_eq!(
        evaluate(&nan_start, &[], &[]).unwrap_err(),
        EvalFault::TypeConfusion {
            register: 0,
            op: "fold",
        }
    );

    let fractional_end = program(vec![
        const_bits(0.0),
        const_bits(3.5),
        EmirOp::ConstI64(0),
        EmirOp::Fold {
            start: EmirValue(0),
            end: EmirValue(1),
            init: EmirValue(2),
            combine: FoldCombine::Add,
            loop_var_index: 0,
            body,
        },
    ]);
    assert_eq!(
        evaluate(&fractional_end, &[], &[]).unwrap_err(),
        EvalFault::TypeConfusion {
            register: 1,
            op: "fold",
        }
    );
}

#[test]
fn integral_rejects_zero_or_odd_steps() {
    let integrand = EmirProgram {
        ops: vec![(EmirOp::LoadInput(0), Span::default())],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let zero_steps = program(vec![
        const_bits(0.0),
        const_bits(1.0),
        EmirOp::Integral {
            start: EmirValue(0),
            end: EmirValue(1),
            steps: 0,
            loop_var_index: 0,
            integrand: integrand.clone(),
        },
    ]);
    assert_eq!(
        evaluate(&zero_steps, &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "integral",
            detail: "integral steps must be positive and even",
        }
    );

    let odd_steps = program(vec![
        const_bits(0.0),
        const_bits(1.0),
        EmirOp::Integral {
            start: EmirValue(0),
            end: EmirValue(1),
            steps: 3,
            loop_var_index: 0,
            integrand,
        },
    ]);
    assert_eq!(
        evaluate(&odd_steps, &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "integral",
            detail: "integral steps must be positive and even",
        }
    );
}

#[test]
fn vector_ops_refuse_length_mismatch() {
    let add = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1)]),
        const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(3)]),
        EmirOp::VectorAdd(EmirValue(2), EmirValue(4)),
    ]);
    assert_eq!(
        evaluate(&add, &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "vec-add",
            detail: "vector length mismatch",
        }
    );

    let dot = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1)]),
        const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(3)]),
        EmirOp::VectorDot(EmirValue(2), EmirValue(4)),
    ]);
    assert_eq!(
        evaluate(&dot, &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "vec-dot",
            detail: "vector length mismatch",
        }
    );
}

#[test]
fn matrix_ops_refuse_shape_mismatch() {
    // Same numel (6) but 2×3 vs 3×2 must not silently zip.
    let add = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        const_bits(3.0),
        const_bits(4.0),
        const_bits(5.0),
        const_bits(6.0),
        EmirOp::MatrixCreate {
            rows: 2,
            cols: 3,
            elements: (0..6).map(EmirValue).collect(),
        },
        EmirOp::MatrixCreate {
            rows: 3,
            cols: 2,
            elements: (0..6).map(EmirValue).collect(),
        },
        EmirOp::MatrixAdd(EmirValue(6), EmirValue(7)),
    ]);
    assert_eq!(
        evaluate(&add, &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "mat-add",
            detail: "matrix shape mismatch",
        }
    );

    let mul = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        const_bits(3.0),
        const_bits(4.0),
        EmirOp::MatrixCreate {
            rows: 2,
            cols: 2,
            elements: (0..4).map(EmirValue).collect(),
        },
        EmirOp::MatrixCreate {
            rows: 1,
            cols: 2,
            elements: vec![EmirValue(0), EmirValue(1)],
        },
        EmirOp::MatrixMulMatrix(EmirValue(4), EmirValue(5)),
    ]);
    assert_eq!(
        evaluate(&mul, &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "mat-mul-mat",
            detail: "matrix product inner dimensions mismatch",
        }
    );

    let mv = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        const_bits(3.0),
        const_bits(4.0),
        EmirOp::MatrixCreate {
            rows: 2,
            cols: 2,
            elements: (0..4).map(EmirValue).collect(),
        },
        const_bits(9.0),
        EmirOp::VectorCreate(vec![EmirValue(5)]),
        EmirOp::MatrixMulVector(EmirValue(4), EmirValue(6)),
    ]);
    assert_eq!(
        evaluate(&mv, &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "mat-mul-vec",
            detail: "matrix×vector width mismatch",
        }
    );
}

/// Build a `Solve` wrapper program over a one-input residual body with
/// the given seed and Newton budget (emath-9bj1 fallback tests).
fn solve_program(body: EmirProgram, seed: f64, max_iter: u32) -> EmirProgram {
    EmirProgram {
        ops: vec![(
            EmirOp::Solve {
                body,
                var_index: 0,
                tolerance: 1e-12,
                max_iter,
            },
            Span::default(),
        )],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

/// Wrap flat ops as a one-input residual program.
fn residual_program(ops: Vec<EmirOp>, result: EmirValue) -> EmirProgram {
    EmirProgram {
        ops: ops.into_iter().map(|op| (op, Span::default())).collect(),
        result,
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

#[test]
fn solve_falls_back_to_bisection_when_derivative_vanishes() {
    // f(x) = x*x - 2 with seed 0: Newton's derivative vanishes at the
    // seed (df = 2x = 0), so the deterministic bracket scan must find
    // sqrt(2) via bisection — and the run must be deterministic
    // (two evaluations, identical bits).
    let body = residual_program(
        vec![
            EmirOp::LoadInput(0),
            EmirOp::LoadInput(0),
            EmirOp::F64Mul(EmirValue(0), EmirValue(1)),
            const_bits(2.0),
            EmirOp::F64Sub(EmirValue(2), EmirValue(3)),
        ],
        EmirValue(4),
    );
    let prog = solve_program(body, 0.0, 8);
    let first = evaluate(&prog, &[Value::F64(0.0)], &[]).expect("bracketed fallback root");
    let second = evaluate(&prog, &[Value::F64(0.0)], &[]).expect("deterministic rerun");
    assert_eq!(first, second, "the fallback must be deterministic");
    let Value::F64(root) = first else {
        panic!("root must be scalar, got {first:?}");
    };
    assert!(
        root > 0.5 && (root * root - 2.0).abs() < 1e-6,
        "fallback must find sqrt(2) ~= 1.414, got {root}"
    );
}

#[test]
fn solve_falls_back_on_a_cubic_flat_at_the_seed() {
    // f(x) = x^3 - 8 with seed 0: df = 3x^2 vanishes at the seed; the
    // fallback must find the single real root x = 2.
    let body = residual_program(
        vec![
            EmirOp::LoadInput(0),
            EmirOp::LoadInput(0),
            EmirOp::F64Mul(EmirValue(0), EmirValue(1)),
            EmirOp::LoadInput(0),
            EmirOp::F64Mul(EmirValue(2), EmirValue(3)),
            const_bits(8.0),
            EmirOp::F64Sub(EmirValue(4), EmirValue(5)),
        ],
        EmirValue(6),
    );
    let prog = solve_program(body, 0.0, 8);
    let Value::F64(root) = evaluate(&prog, &[Value::F64(0.0)], &[])
        .expect("flat-seed cubic must fall back to the bracketed root")
    else {
        panic!("root must be scalar");
    };
    assert!(
        (root - 2.0).abs() < 1e-6,
        "fallback must find 2^(1/3) root x = 2, got {root}"
    );
}

#[test]
fn solve_without_a_real_root_still_refuses_after_the_fallback() {
    // f(x) = x*x + 1: Newton's derivative vanishes at the seed 0 AND
    // the deterministic scan finds no sign change (f > 0 everywhere).
    // The fallback must refuse with the pre-existing typed fault —
    // never a hang, never an invented root.
    let body = residual_program(
        vec![
            EmirOp::LoadInput(0),
            EmirOp::LoadInput(0),
            EmirOp::F64Mul(EmirValue(0), EmirValue(1)),
            const_bits(1.0),
            EmirOp::F64Add(EmirValue(2), EmirValue(3)),
        ],
        EmirValue(4),
    );
    let prog = solve_program(body, 0.0, 8);
    assert_eq!(
        evaluate(&prog, &[Value::F64(0.0)], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "solve",
            detail: "solve derivative vanished before convergence",
        }
    );
}

#[test]
fn solve_refuses_vanished_derivative() {
    // f(x) = 1 (constant); Newton has df=0 while |f| is not small.
    let body = EmirProgram {
        ops: vec![(const_bits(1.0), Span::default())],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = EmirProgram {
        ops: vec![(
            EmirOp::Solve {
                body,
                var_index: 0,
                tolerance: 1e-12,
                max_iter: 8,
            },
            Span::default(),
        )],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    assert_eq!(
        evaluate(&prog, &[Value::F64(0.0)], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "solve",
            detail: "solve derivative vanished before convergence",
        }
    );
}

#[test]
fn solve_refuses_max_iter_without_root() {
    // f(x) = x with max_iter=0: no Newton steps, residual stays 1.
    // Must refuse rather than return the initial guess as a fake root.
    let body = EmirProgram {
        ops: vec![(EmirOp::LoadInput(0), Span::default())],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = EmirProgram {
        ops: vec![(
            EmirOp::Solve {
                body,
                var_index: 0,
                tolerance: 1e-12,
                max_iter: 0,
            },
            Span::default(),
        )],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    assert_eq!(
        evaluate(&prog, &[Value::F64(1.0)], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "solve",
            detail: "solve did not converge within max_iter",
        }
    );
}

fn square_minus_four_body() -> EmirProgram {
    // f(x) = x*x - 4
    EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()),
            (EmirOp::F64Mul(EmirValue(0), EmirValue(0)), Span::default()),
            (const_bits(4.0), Span::default()),
            (EmirOp::F64Sub(EmirValue(1), EmirValue(2)), Span::default()),
        ],
        result: EmirValue(3),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

fn square_shift_body(shift: f64) -> EmirProgram {
    // f(x) = (x - shift)^2
    EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()),
            (const_bits(shift), Span::default()),
            (EmirOp::F64Sub(EmirValue(0), EmirValue(1)), Span::default()),
            (EmirOp::F64Mul(EmirValue(2), EmirValue(2)), Span::default()),
        ],
        result: EmirValue(3),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

#[test]
fn solve_root_has_near_zero_residual() {
    let prog = EmirProgram {
        ops: vec![(
            EmirOp::Solve {
                body: square_minus_four_body(),
                var_index: 0,
                tolerance: 1e-12,
                max_iter: 100,
            },
            Span::default(),
        )],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let root = match evaluate(&prog, &[Value::F64(1.0)], &[]).unwrap() {
        Value::F64(v) => v,
        other => panic!("expected F64 root, got {other:?}"),
    };
    assert!(
        (root * root - 4.0).abs() < 1e-12,
        "claimed root {root} has residual {}",
        root * root - 4.0
    );
    let neg = match evaluate(&prog, &[Value::F64(-1.0)], &[]).unwrap() {
        Value::F64(v) => v,
        other => panic!("expected F64 root, got {other:?}"),
    };
    assert!(
        (neg + 2.0).abs() < 1e-9 && (neg * neg - 4.0).abs() < 1e-12,
        "from x=-1 Newton must follow the negative basin, got {neg}"
    );
}

#[test]
fn optimize_min_is_stationary() {
    let prog = program(vec![EmirOp::Optimize {
        body: square_shift_body(3.0),
        var_indices: vec![0],
        maximize: false,
        learning_rate: 0.01,
        tolerance: 1e-6,
        max_iter: 8,
    }]);
    let min_x = match evaluate(&prog, &[Value::F64(0.0)], &[]).unwrap() {
        Value::F64(v) => v,
        other => panic!("expected F64 min, got {other:?}"),
    };
    let grad = 2.0 * (min_x - 3.0);
    assert!(
        grad.abs() < 1e-6,
        "claimed min {min_x} has gradient {grad}, not a stationary point"
    );
}

#[test]
fn optimize_refuses_max_iter_without_stationarity() {
    // f(x) = x with max_iter=0: no Newton steps, |grad| stays 1.
    // Must refuse rather than return the initial guess as a fake min.
    let body = EmirProgram {
        ops: vec![(EmirOp::LoadInput(0), Span::default())],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = program(vec![EmirOp::Optimize {
        body,
        var_indices: vec![0],
        maximize: false,
        learning_rate: 0.01,
        tolerance: 1e-8,
        max_iter: 0,
    }]);
    assert_eq!(
        evaluate(&prog, &[Value::F64(10.0)], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "optimize",
            detail: "optimize did not converge within max_iter",
        }
    );
}

#[test]
fn optimize_refuses_vanished_hessian() {
    // f(x) = x; Newton has H=0 while |∇f| is not small.
    let body = EmirProgram {
        ops: vec![(EmirOp::LoadInput(0), Span::default())],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = program(vec![EmirOp::Optimize {
        body,
        var_indices: vec![0],
        maximize: false,
        learning_rate: 0.01,
        tolerance: 1e-8,
        max_iter: 8,
    }]);
    assert_eq!(
        evaluate(&prog, &[Value::F64(10.0)], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "optimize",
            detail: "optimize hessian vanished before stationarity",
        }
    );
}

#[test]
fn optimize_refuses_min_as_a_max() {
    // maximize (x-3)^2 has a minimum at x=3, not a maximum. Newton must
    // refuse the wrong-curvature stationary point rather than return 3.
    let prog = program(vec![EmirOp::Optimize {
        body: square_shift_body(3.0),
        var_indices: vec![0],
        maximize: true,
        learning_rate: 0.01,
        tolerance: 1e-6,
        max_iter: 8,
    }]);
    assert_eq!(
        evaluate(&prog, &[Value::F64(0.0)], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "optimize",
            detail: "optimize hessian has the wrong curvature for maximize",
        }
    );
}

#[test]
fn optimize_refuses_empty_var_indices() {
    let body = EmirProgram {
        ops: vec![(const_bits(1.0), Span::default())],
        result: EmirValue(0),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = program(vec![EmirOp::Optimize {
        body,
        var_indices: Vec::new(),
        maximize: false,
        learning_rate: 0.1,
        tolerance: 1e-8,
        max_iter: 4,
    }]);
    assert_eq!(
        evaluate(&prog, &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "optimize",
            detail: "optimize requires at least one variable",
        }
    );
}

#[test]
fn fold_and_accepts_bool_init() {
    // Vacuous forall over an empty range with Bool true init → true.
    // Body is unused for an empty range but must still be well-formed.
    let body = EmirProgram {
        ops: vec![(EmirOp::LoadInput(0), Span::default())],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = program(vec![
        const_bits(1.0),
        const_bits(1.0),
        EmirOp::Eq(EmirValue(0), EmirValue(1)),
        const_bits(2.0),
        const_bits(2.0),
        EmirOp::Fold {
            start: EmirValue(3),
            end: EmirValue(4),
            init: EmirValue(2),
            combine: FoldCombine::And,
            loop_var_index: 0,
            body,
        },
    ]);
    assert_eq!(evaluate(&prog, &[], &[]).unwrap(), Value::Bool(true));
}

// Build a program that constructs `input` as a vector via VectorCreate,
// applies a 1D stencil, and returns the result. Mirrors how
// `laplacian(u, dx)` lowers to `EmirOp::Stencil1d`.
fn stencil_prog(weights: Vec<f64>, center: usize, input: Vec<f64>) -> EmirProgram {
    stencil_prog_edge(weights, center, input, EdgePolicy::Clamp)
}

fn stencil_prog_edge(
    weights: Vec<f64>,
    center: usize,
    input: Vec<f64>,
    edge: EdgePolicy,
) -> EmirProgram {
    let n = input.len();
    let mut ops: Vec<EmirOp> = input.iter().map(|x| const_bits(*x)).collect();
    let elems: Vec<EmirValue> = (0..n).map(|i| EmirValue(i as u32)).collect();
    ops.push(EmirOp::VectorCreate(elems));
    let vec_reg = u32::try_from(n).unwrap_or(0);
    ops.push(EmirOp::Stencil1d {
        input: EmirValue(vec_reg),
        weights,
        center,
        edge,
    });
    let last = u32::try_from(ops.len().saturating_sub(1)).unwrap_or(0);
    EmirProgram {
        ops: ops.into_iter().map(|op| (op, Span::default())).collect(),
        result: EmirValue(last),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

#[test]
fn stencil_laplacian_constant_is_zero() {
    // The laplacian of a constant field is zero everywhere, including the
    // clamped boundary cells (the replicated neighbor equals the cell).
    let prog = stencil_prog(vec![1.0, -2.0, 1.0], 1, vec![3.0; 5]);
    assert_eq!(
        evaluate(&prog, &[], &[]).unwrap(),
        Value::Vector(vec![0.0; 5])
    );
}

#[test]
fn stencil_laplacian_linear_is_zero_interior() {
    // The central second difference of a linear field is zero on interior.
    let prog = stencil_prog(vec![1.0, -2.0, 1.0], 1, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    let out = match evaluate(&prog, &[], &[]).unwrap() {
        Value::Vector(v) => v,
        _ => panic!("expected vector"),
    };
    assert_eq!(out[1], 0.0);
    assert_eq!(out[2], 0.0);
    assert_eq!(out[3], 0.0);
}

#[test]
fn stencil_laplacian_quadratic_is_two_interior() {
    // The central second difference is exact on quadratics: u[i-1] - 2u[i]
    // + u[i+1] of x^2 with dx = 1 equals 2 on the interior.
    let prog = stencil_prog(vec![1.0, -2.0, 1.0], 1, vec![0.0, 1.0, 4.0, 9.0, 16.0]);
    let out = match evaluate(&prog, &[], &[]).unwrap() {
        Value::Vector(v) => v,
        _ => panic!("expected vector"),
    };
    assert_eq!(out[1], 2.0);
    assert_eq!(out[2], 2.0);
    assert_eq!(out[3], 2.0);
}

#[test]
fn stencil_laplacian_sine_matches_continuous() {
    // u[i] = sin(x_i), x_i = i * dx, dx = 0.1. The continuous Laplacian
    // d^2/dx^2 sin(x) = -sin(x); the discrete stencil (weights
    // [1, -2, 1] / dx^2) approximates -u[i] on the interior.
    let dx = 0.1;
    let inv = 1.0 / (dx * dx);
    let n = 20;
    let input: Vec<f64> = (0..n).map(|i| ((i as f64) * dx).sin()).collect();
    let prog = stencil_prog(vec![inv, -2.0 * inv, inv], 1, input.clone());
    let out = match evaluate(&prog, &[], &[]).unwrap() {
        Value::Vector(v) => v,
        _ => panic!("expected vector"),
    };
    for i in 2..(n - 2) {
        let analytic = -input[i];
        assert!(
            (out[i] - analytic).abs() < 1e-2,
            "i={i}: discrete laplacian {} vs continuous {}",
            out[i],
            analytic
        );
    }
}

#[test]
fn stencil_clamped_edge_replicates_boundary() {
    // At i = 0 with Clamp the left neighbor is u[0] itself, so the stencil
    // collapses to u[1] - u[0]; symmetrically at the right edge.
    let prog = stencil_prog(vec![1.0, -2.0, 1.0], 1, vec![5.0, 7.0, 9.0]);
    let out = match evaluate(&prog, &[], &[]).unwrap() {
        Value::Vector(v) => v,
        _ => panic!("expected vector"),
    };
    assert_eq!(out[0], 7.0 - 5.0);
    assert_eq!(out[2], 7.0 - 9.0);
}

#[test]
fn stencil_dirichlet_matching_value_is_zero() {
    // Dirichlet boundaries held at the field's own constant value: the
    // ghost cells match the interior, so the laplacian is zero everywhere,
    // including the boundary cells.
    let prog = stencil_prog_edge(
        vec![1.0, -2.0, 1.0],
        1,
        vec![5.0; 5],
        EdgePolicy::Dirichlet {
            left: 5.0,
            right: 5.0,
        },
    );
    assert_eq!(
        evaluate(&prog, &[], &[]).unwrap(),
        Value::Vector(vec![0.0; 5])
    );
}

#[test]
fn stencil_dirichlet_mismatched_value_shifts_boundary() {
    // Constant field c = 5 with Dirichlet boundaries held at 0: only the
    // boundary cells see the ghost value, so L[0] = L[4] = (0 - 5) = -5
    // and the interior stays zero.
    let prog = stencil_prog_edge(
        vec![1.0, -2.0, 1.0],
        1,
        vec![5.0; 5],
        EdgePolicy::Dirichlet {
            left: 0.0,
            right: 0.0,
        },
    );
    let out = match evaluate(&prog, &[], &[]).unwrap() {
        Value::Vector(v) => v,
        _ => panic!("expected vector"),
    };
    assert_eq!(out[0], -5.0);
    assert_eq!(out[4], -5.0);
    assert_eq!(out[1], 0.0);
    assert_eq!(out[2], 0.0);
    assert_eq!(out[3], 0.0);
}

#[test]
fn stencil_neumann_mirror_reflects_linear_field() {
    // Neumann mirrors the next interior cell across the boundary
    // (u[-1] = u[1], u[n] = u[n-2]). For a linear field the interior
    // second difference is 0; the mirrored ghost creates a kink, giving
    // L[0] = 2*(u[1]-u[0]) = 2 and L[4] = 2*(u[3]-u[4]) = -2.
    let prog = stencil_prog_edge(
        vec![1.0, -2.0, 1.0],
        1,
        vec![0.0, 1.0, 2.0, 3.0, 4.0],
        EdgePolicy::Neumann,
    );
    let out = match evaluate(&prog, &[], &[]).unwrap() {
        Value::Vector(v) => v,
        _ => panic!("expected vector"),
    };
    assert_eq!(out[0], 2.0);
    assert_eq!(out[1], 0.0);
    assert_eq!(out[2], 0.0);
    assert_eq!(out[3], 0.0);
    assert_eq!(out[4], -2.0);
}

// Build a program that constructs `data` as a row-major matrix via
// MatrixCreate, applies a 2D 3x3 stencil, and returns the result.
fn stencil2d_prog(
    weights: Vec<f64>,
    center: (usize, usize),
    rows: usize,
    cols: usize,
    data: Vec<f64>,
    edge: EdgePolicy,
) -> EmirProgram {
    let n = data.len();
    let mut ops: Vec<EmirOp> = data.iter().map(|x| const_bits(*x)).collect();
    let elems: Vec<EmirValue> = (0..n).map(|i| EmirValue(i as u32)).collect();
    ops.push(EmirOp::MatrixCreate {
        rows,
        cols,
        elements: elems,
    });
    let mat_reg = u32::try_from(n).unwrap_or(0);
    ops.push(EmirOp::Stencil2d {
        input: EmirValue(mat_reg),
        weights,
        center,
        edge,
    });
    let last = u32::try_from(ops.len().saturating_sub(1)).unwrap_or(0);
    EmirProgram {
        ops: ops.into_iter().map(|op| (op, Span::default())).collect(),
        result: EmirValue(last),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

fn stencil3d_prog(
    weights: Vec<f64>,
    shape: [usize; 3],
    data: Vec<f64>,
    edge: EdgePolicy,
) -> EmirProgram {
    let n = data.len();
    let mut ops: Vec<EmirOp> = data.iter().map(|value| const_bits(*value)).collect();
    let elements = (0..n).map(|index| EmirValue(index as u32)).collect();
    ops.push(EmirOp::TensorCreate {
        shape: shape.to_vec(),
        elements,
    });
    ops.push(EmirOp::Stencil3d {
        input: EmirValue(n as u32),
        weights,
        center: (1, 1, 1),
        edge,
    });
    EmirProgram {
        result: EmirValue(ops.len() as u32 - 1),
        ops: ops.into_iter().map(|op| (op, Span::default())).collect(),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

fn derivative3d_weights(axis: usize, spacing: f64) -> Vec<f64> {
    let mut weights = vec![0.0; 27];
    let inv = 1.0 / (2.0 * spacing);
    let (negative, positive) = [(4, 22), (10, 16), (12, 14)][axis];
    weights[negative] = -inv;
    weights[positive] = inv;
    weights
}

#[test]
fn stencil2d_laplacian_constant_is_zero() {
    // The 5-point laplacian of a constant field is zero everywhere,
    // including the clamped boundary cells.
    let weights = vec![0.0, 1.0, 0.0, 1.0, -4.0, 1.0, 0.0, 1.0, 0.0];
    let prog = stencil2d_prog(weights, (1, 1), 3, 3, vec![7.0; 9], EdgePolicy::Clamp);
    assert_eq!(
        evaluate(&prog, &[], &[]).unwrap(),
        Value::Matrix {
            rows: 3,
            cols: 3,
            data: vec![0.0; 9]
        }
    );
}

#[test]
fn stencil2d_laplacian_quadratic_is_four_interior() {
    // u[r][c] = r^2 + c^2; the continuous laplacian is 4, and the
    // 5-point stencil recovers it exactly on the interior (dx = 1).
    let rows = 5;
    let cols = 5;
    let data: Vec<f64> = (0..rows)
        .flat_map(|r| (0..cols).map(move |c| (r as f64).powi(2) + (c as f64).powi(2)))
        .collect();
    let weights = vec![0.0, 1.0, 0.0, 1.0, -4.0, 1.0, 0.0, 1.0, 0.0];
    let prog = stencil2d_prog(weights, (1, 1), rows, cols, data, EdgePolicy::Clamp);
    let out = match evaluate(&prog, &[], &[]).unwrap() {
        Value::Matrix { data, .. } => data,
        _ => panic!("expected matrix"),
    };
    for r in 1..(rows - 1) {
        for c in 1..(cols - 1) {
            assert_eq!(out[r * cols + c], 4.0, "interior ({r},{c})");
        }
    }
}

#[test]
fn gradient_constant_field_is_zero() {
    // du/dx of a constant field is zero everywhere (dx = 1, inv = 1/2).
    let weights = vec![-0.5, 0.0, 0.5];
    let prog = stencil_prog(weights, 1, vec![5.0; 5]);
    assert_eq!(
        evaluate(&prog, &[], &[]).unwrap(),
        Value::Vector(vec![0.0; 5])
    );
}

#[test]
fn gradient_linear_field_is_one_everywhere() {
    // u = [0,1,2,3,4] (slope 1). Central interior + one-sided edges
    // (linear ghost) is 1 everywhere. Clamp on this stencil would
    // return 0.5 at the boundary — that is not the derivative.
    let weights = vec![-0.5, 0.0, 0.5];
    let prog = stencil_prog_edge(
        weights,
        1,
        vec![0.0, 1.0, 2.0, 3.0, 4.0],
        EdgePolicy::OneSided,
    );
    assert_eq!(
        evaluate(&prog, &[], &[]).unwrap(),
        Value::Vector(vec![1.0, 1.0, 1.0, 1.0, 1.0])
    );
}

#[test]
fn gradient_2d_x_linear_in_columns() {
    // u[r][c] = c (increasing along columns). du/dc is 1 everywhere
    // under one-sided edges, constant along rows.
    let data = vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0];
    let weights = vec![0.0, 0.0, 0.0, -0.5, 0.0, 0.5, 0.0, 0.0, 0.0];
    let prog = stencil2d_prog(weights, (1, 1), 3, 3, data, EdgePolicy::OneSided);
    let expected = vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
    assert_eq!(
        evaluate(&prog, &[], &[]).unwrap(),
        Value::Matrix {
            rows: 3,
            cols: 3,
            data: expected
        }
    );
}

#[test]
fn gradient_2d_y_linear_in_rows() {
    // u[r][c] = r (increasing along rows). du/dr is 1 everywhere under
    // one-sided edges, constant along columns.
    let data = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0];
    let weights = vec![0.0, -0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0];
    let prog = stencil2d_prog(weights, (1, 1), 3, 3, data, EdgePolicy::OneSided);
    let expected = vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
    assert_eq!(
        evaluate(&prog, &[], &[]).unwrap(),
        Value::Matrix {
            rows: 3,
            cols: 3,
            data: expected
        }
    );
}

#[test]
fn stencil3d_laplacian_recovers_quadratic_interior() {
    let data = (0..3)
        .flat_map(|x| (0..3).flat_map(move |y| (0..3).map(move |z| (x * x + y * y + z * z) as f64)))
        .collect();
    let mut weights = vec![0.0; 27];
    for index in [4, 22, 10, 16, 12, 14] {
        weights[index] = 1.0;
    }
    weights[13] = -6.0;
    let output = evaluate(
        &stencil3d_prog(weights, [3, 3, 3], data, EdgePolicy::Clamp),
        &[],
        &[],
    )
    .unwrap();
    let Value::Tensor { data, .. } = output else {
        panic!("expected rank-3 tensor");
    };
    assert_eq!(data[13], 6.0);
}

#[test]
fn gradient3d_axes_are_exact_on_linear_ramps() {
    for axis in 0..3 {
        let data = (0..3)
            .flat_map(|x| {
                (0..3).flat_map(move |y| (0..3).map(move |z| [x as f64, y as f64, z as f64][axis]))
            })
            .collect();
        let output = evaluate(
            &stencil3d_prog(
                derivative3d_weights(axis, 1.0),
                [3, 3, 3],
                data,
                EdgePolicy::OneSided,
            ),
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(
            output,
            Value::Tensor {
                shape: vec![3, 3, 3],
                data: vec![1.0; 27],
            },
            "axis {axis}"
        );
    }
}

#[test]
fn divergence3d_sums_axis_derivatives() {
    let fields: Vec<Vec<f64>> = (0..3)
        .map(|axis| {
            (0..3)
                .flat_map(|x| {
                    (0..3).flat_map(move |y| {
                        (0..3).map(move |z| [x as f64, 2.0 * y as f64, 3.0 * z as f64][axis])
                    })
                })
                .collect()
        })
        .collect();
    let mut ops = Vec::new();
    let mut derivatives = Vec::new();
    for (axis, field) in fields.iter().enumerate() {
        let elements = field
            .iter()
            .map(|value| {
                let register = EmirValue(ops.len() as u32);
                ops.push((const_bits(*value), Span::default()));
                register
            })
            .collect();
        let tensor = EmirValue(ops.len() as u32);
        ops.push((
            EmirOp::TensorCreate {
                shape: vec![3, 3, 3],
                elements,
            },
            Span::default(),
        ));
        let derivative = EmirValue(ops.len() as u32);
        ops.push((
            EmirOp::Stencil3d {
                input: tensor,
                weights: derivative3d_weights(axis, 1.0),
                center: (1, 1, 1),
                edge: EdgePolicy::OneSided,
            },
            Span::default(),
        ));
        derivatives.push(derivative);
    }
    let xy = EmirValue(ops.len() as u32);
    ops.push((
        EmirOp::TensorAdd(derivatives[0], derivatives[1]),
        Span::default(),
    ));
    let result = EmirValue(ops.len() as u32);
    ops.push((EmirOp::TensorAdd(xy, derivatives[2]), Span::default()));
    let program = EmirProgram {
        ops,
        result,
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    assert_eq!(
        evaluate(&program, &[], &[]).unwrap(),
        Value::Tensor {
            shape: vec![3, 3, 3],
            data: vec![6.0; 27],
        }
    );
}

#[test]
fn stencil3d_refuses_non_tensor_input() {
    let program = EmirProgram {
        ops: vec![
            (EmirOp::VectorCreate(Vec::new()), Span::default()),
            (
                EmirOp::Stencil3d {
                    input: EmirValue(0),
                    weights: vec![0.0; 27],
                    center: (1, 1, 1),
                    edge: EdgePolicy::Clamp,
                },
                Span::default(),
            ),
        ],
        result: EmirValue(1),
        input_count: 0,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    assert!(matches!(
        evaluate(&program, &[], &[]),
        Err(EvalFault::TypeConfusion { .. })
    ));
}

// ---- B12: logic connectives evaluation ----------------------------------

#[test]
fn imply_truth_table() {
    // Imply: !a || b
    let cases = [
        (false, false, true),
        (false, true, true),
        (true, false, false),
        (true, true, true),
    ];
    for (a, b, expected) in cases {
        let prog = EmirProgram {
            ops: vec![
                (EmirOp::LoadInput(0), Span::default()),
                (EmirOp::LoadInput(1), Span::default()),
                (EmirOp::Imply(EmirValue(0), EmirValue(1)), Span::default()),
            ],
            result: EmirValue(2),
            input_count: 2,
            state_count: 0,
            domain_obligations: Vec::new(),
        };
        let inputs = vec![Value::Bool(a), Value::Bool(b)];
        let result = evaluate(&prog, &inputs, &[]).unwrap();
        assert_eq!(
            result,
            Value::Bool(expected),
            "Imply({a}, {b}) should be {expected}"
        );
    }
}

#[test]
fn iff_truth_table() {
    // Iff: a == b for Bool
    let cases = [
        (false, false, true),
        (false, true, false),
        (true, false, false),
        (true, true, true),
    ];
    for (a, b, expected) in cases {
        let prog = EmirProgram {
            ops: vec![
                (EmirOp::LoadInput(0), Span::default()),
                (EmirOp::LoadInput(1), Span::default()),
                (EmirOp::Iff(EmirValue(0), EmirValue(1)), Span::default()),
            ],
            result: EmirValue(2),
            input_count: 2,
            state_count: 0,
            domain_obligations: Vec::new(),
        };
        let inputs = vec![Value::Bool(a), Value::Bool(b)];
        let result = evaluate(&prog, &inputs, &[]).unwrap();
        assert_eq!(
            result,
            Value::Bool(expected),
            "Iff({a}, {b}) should be {expected}"
        );
    }
}

// ---- einsum tests (B08) ---------------------------------------------------

#[test]
fn einsum_matrix_multiply() {
    // A = [[1, 2], [3, 4]], B = [[5, 6], [7, 8]]
    // C = einsum("ik,kj->ij", A, B) = [[19, 22], [43, 50]]
    let program = program(vec![
        const_bits(1.0), // 0
        const_bits(2.0), // 1
        const_bits(3.0), // 2
        const_bits(4.0), // 3
        EmirOp::MatrixCreate {
            rows: 2,
            cols: 2,
            elements: vec![EmirValue(0), EmirValue(1), EmirValue(2), EmirValue(3)],
        }, // 4: A
        const_bits(5.0), // 5
        const_bits(6.0), // 6
        const_bits(7.0), // 7
        const_bits(8.0), // 8
        EmirOp::MatrixCreate {
            rows: 2,
            cols: 2,
            elements: vec![EmirValue(5), EmirValue(6), EmirValue(7), EmirValue(8)],
        }, // 9: B
        EmirOp::Einsum {
            subscripts: "ik,kj->ij".to_string(),
            inputs: vec![EmirValue(4), EmirValue(9)],
        }, // 10: C
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(
        result,
        Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![19.0, 22.0, 43.0, 50.0],
        }
    );
}

#[test]
fn einsum_vector_dot_product() {
    // a = [1, 2, 3], b = [4, 5, 6]
    // einsum("i,i->", a, b) = 1*4 + 2*5 + 3*6 = 32
    let program = program(vec![
        const_bits(1.0),                                                      // 0
        const_bits(2.0),                                                      // 1
        const_bits(3.0),                                                      // 2
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3: a
        const_bits(4.0),                                                      // 4
        const_bits(5.0),                                                      // 5
        const_bits(6.0),                                                      // 6
        EmirOp::VectorCreate(vec![EmirValue(4), EmirValue(5), EmirValue(6)]), // 7: b
        EmirOp::Einsum {
            subscripts: "i,i->".to_string(),
            inputs: vec![EmirValue(3), EmirValue(7)],
        }, // 8: scalar
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::F64(32.0));
}

#[test]
fn einsum_transpose() {
    // A = [[1, 2, 3], [4, 5, 6]] (2x3)
    // einsum("ij->ji", A) = [[1, 4], [2, 5], [3, 6]] (3x2)
    let program = program(vec![
        const_bits(1.0), // 0
        const_bits(2.0), // 1
        const_bits(3.0), // 2
        const_bits(4.0), // 3
        const_bits(5.0), // 4
        const_bits(6.0), // 5
        EmirOp::MatrixCreate {
            rows: 2,
            cols: 3,
            elements: vec![
                EmirValue(0),
                EmirValue(1),
                EmirValue(2),
                EmirValue(3),
                EmirValue(4),
                EmirValue(5),
            ],
        }, // 6: A
        EmirOp::Einsum {
            subscripts: "ij->ji".to_string(),
            inputs: vec![EmirValue(6)],
        }, // 7: A^T
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(
        result,
        Value::Matrix {
            rows: 3,
            cols: 2,
            data: vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0],
        }
    );
}

fn push_matrix(ops: &mut Vec<EmirOp>, rows: usize, cols: usize, data: &[f64]) -> u32 {
    let start = ops.len() as u32;
    ops.extend(data.iter().copied().map(const_bits));
    ops.push(EmirOp::MatrixCreate {
        rows,
        cols,
        elements: (start..start + data.len() as u32).map(EmirValue).collect(),
    });
    ops.len() as u32 - 1
}

fn push_vector(ops: &mut Vec<EmirOp>, data: &[f64]) -> u32 {
    let start = ops.len() as u32;
    ops.extend(data.iter().copied().map(const_bits));
    ops.push(EmirOp::VectorCreate(
        (start..start + data.len() as u32).map(EmirValue).collect(),
    ));
    ops.len() as u32 - 1
}

fn eval_ops(ops: Vec<EmirOp>) -> Value {
    evaluate(&program(ops), &[], &[]).unwrap()
}

/// `einsum("ik,kj->ij")` == matmul, including rectangular; implicit
/// `"ik,kj"` is deterministic (alphabetical free indices, not HashSet
/// iteration order); `"i,i->"` == `dot`.
#[test]
fn einsum_matches_matmul_and_dot() {
    // A 2×3, B 3×2: C = A @ B = [[58, 64], [139, 154]]
    let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let b = [7.0, 8.0, 9.0, 10.0, 11.0, 12.0];
    let expected = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![58.0, 64.0, 139.0, 154.0],
    };

    let mut mul = Vec::new();
    let a_reg = push_matrix(&mut mul, 2, 3, &a);
    let b_reg = push_matrix(&mut mul, 3, 2, &b);
    mul.push(EmirOp::MatrixMulMatrix(EmirValue(a_reg), EmirValue(b_reg)));
    assert_eq!(eval_ops(mul), expected);

    for subscripts in ["ik,kj->ij", "ik,kj", "i k, k j -> i j"] {
        let mut ops = Vec::new();
        let a_reg = push_matrix(&mut ops, 2, 3, &a);
        let b_reg = push_matrix(&mut ops, 3, 2, &b);
        ops.push(EmirOp::Einsum {
            subscripts: subscripts.to_string(),
            inputs: vec![EmirValue(a_reg), EmirValue(b_reg)],
        });
        assert_eq!(eval_ops(ops), expected, "subscripts {subscripts:?}");
    }

    let mut dot = Vec::new();
    let u = push_vector(&mut dot, &[1.0, 2.0, 3.0]);
    let v = push_vector(&mut dot, &[4.0, 5.0, 6.0]);
    let mut ein = dot.clone();
    dot.push(EmirOp::VectorDot(EmirValue(u), EmirValue(v)));
    ein.push(EmirOp::Einsum {
        subscripts: "i,i->".to_string(),
        inputs: vec![EmirValue(u), EmirValue(v)],
    });
    assert_eq!(eval_ops(dot), Value::F64(32.0));
    assert_eq!(eval_ops(ein), Value::F64(32.0));
}

/// Implicit `"ji"` is alphabetical `"ij"` (numpy): a transpose, not identity.
/// `transpose(transpose(A)) == A` for a rectangular matrix.
#[test]
fn einsum_implicit_ji_and_transpose_involution() {
    let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let a = Value::Matrix {
        rows: 2,
        cols: 3,
        data: data.to_vec(),
    };
    let at = Value::Matrix {
        rows: 3,
        cols: 2,
        data: vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0],
    };

    let mut implicit = Vec::new();
    let a_reg = push_matrix(&mut implicit, 2, 3, &data);
    implicit.push(EmirOp::Einsum {
        subscripts: "ji".to_string(),
        inputs: vec![EmirValue(a_reg)],
    });
    assert_eq!(eval_ops(implicit), at);

    let mut twice = Vec::new();
    let a_reg = push_matrix(&mut twice, 2, 3, &data);
    twice.push(EmirOp::MatrixTranspose(EmirValue(a_reg)));
    twice.push(EmirOp::MatrixTranspose(EmirValue(a_reg + 1)));
    assert_eq!(eval_ops(twice), a);
}

/// `einsum("i->ii", v)` is diag(v), not a row-broadcast (last-write-wins
/// used to write v[j] into every column of row-major output).
#[test]
fn einsum_diagonal_embed_and_broadcast() {
    let mut diag = Vec::new();
    let v = push_vector(&mut diag, &[1.0, 2.0, 3.0]);
    diag.push(EmirOp::Einsum {
        subscripts: "i->ii".to_string(),
        inputs: vec![EmirValue(v)],
    });
    assert_eq!(
        eval_ops(diag),
        Value::Matrix {
            rows: 3,
            cols: 3,
            data: vec![1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0],
        }
    );

    // Size-1 axis broadcasts: (2×3) ⊙ (1×3) elementwise.
    let mut bc = Vec::new();
    let a = push_matrix(&mut bc, 2, 3, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let b = push_matrix(&mut bc, 1, 3, &[10.0, 20.0, 30.0]);
    bc.push(EmirOp::Einsum {
        subscripts: "ij,ij->ij".to_string(),
        inputs: vec![EmirValue(a), EmirValue(b)],
    });
    assert_eq!(
        eval_ops(bc),
        Value::Matrix {
            rows: 2,
            cols: 3,
            data: vec![10.0, 40.0, 90.0, 40.0, 100.0, 180.0],
        }
    );

    // Genuine k-extent mismatch: 2×3 × 2×2 must not take max(3,2) and panic.
    let mut bad = Vec::new();
    let a = push_matrix(&mut bad, 2, 3, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let b = push_matrix(&mut bad, 2, 2, &[1.0, 2.0, 3.0, 4.0]);
    bad.push(EmirOp::Einsum {
        subscripts: "ik,kj->ij".to_string(),
        inputs: vec![EmirValue(a), EmirValue(b)],
    });
    assert_eq!(
        evaluate(&program(bad), &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "einsum",
            detail: "einsum dimension mismatch",
        }
    );
}

/// Empty contraction is the sum identity (0), empty vector norm is 0,
/// empty VectorCreate does not panic. Language `Vector[0]` / `[]` are
/// named-refused at admit; these are the eval-side empty leaves.
#[test]
fn empty_vector_norm_and_einsum_are_identities() {
    let empty = program(vec![EmirOp::VectorCreate(vec![])]);
    assert_eq!(evaluate(&empty, &[], &[]).unwrap(), Value::Vector(vec![]));

    let mut norm = Vec::new();
    let v = push_vector(&mut norm, &[]);
    norm.push(EmirOp::VectorNorm(EmirValue(v)));
    match eval_ops(norm) {
        Value::F64(n) => assert_eq!(
            n.to_bits(),
            0.0f64.to_bits(),
            "||[]|| must be +0.0, got {n:?}"
        ),
        other => panic!("expected F64, got {other:?}"),
    }

    let mut ein = Vec::new();
    let a = push_vector(&mut ein, &[]);
    let b = push_vector(&mut ein, &[]);
    ein.push(EmirOp::Einsum {
        subscripts: "i,i->".to_string(),
        inputs: vec![EmirValue(a), EmirValue(b)],
    });
    assert_eq!(
        eval_ops(ein),
        Value::F64(0.0),
        "einsum empty contraction is the empty sum, not a panic"
    );
}

/// `t[0, :, :]` is the first 2×2 face (tensor-face.emath identity).
#[test]
fn tensor_face_slice_is_first_face() {
    let program = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        const_bits(3.0),
        const_bits(4.0),
        const_bits(5.0),
        const_bits(6.0),
        const_bits(7.0),
        const_bits(8.0),
        EmirOp::TensorCreate {
            shape: vec![2, 2, 2],
            elements: (0..8).map(EmirValue).collect(),
        },
        const_bits(0.0),
        const_bits(2.0),
        EmirOp::TensorSlice {
            tensor: EmirValue(8),
            axes: vec![
                emath_exec_ir::EmirSliceAxis::Point(EmirValue(9)),
                emath_exec_ir::EmirSliceAxis::Range {
                    start: EmirValue(9),
                    end: EmirValue(10),
                },
                emath_exec_ir::EmirSliceAxis::Range {
                    start: EmirValue(9),
                    end: EmirValue(10),
                },
            ],
        },
    ]);
    assert_eq!(
        evaluate(&program, &[], &[]).unwrap(),
        Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![1.0, 2.0, 3.0, 4.0],
        }
    );
}

#[test]
fn tensor_slice_out_of_bounds_is_a_fault() {
    let program = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        const_bits(3.0),
        const_bits(4.0),
        const_bits(5.0),
        const_bits(6.0),
        const_bits(7.0),
        const_bits(8.0),
        EmirOp::TensorCreate {
            shape: vec![2, 2, 2],
            elements: (0..8).map(EmirValue).collect(),
        },
        const_bits(2.0),
        EmirOp::TensorSlice {
            tensor: EmirValue(8),
            axes: vec![
                emath_exec_ir::EmirSliceAxis::Point(EmirValue(9)),
                emath_exec_ir::EmirSliceAxis::Range {
                    start: EmirValue(9),
                    end: EmirValue(9),
                },
                emath_exec_ir::EmirSliceAxis::Range {
                    start: EmirValue(9),
                    end: EmirValue(9),
                },
            ],
        },
    ]);
    assert_eq!(
        evaluate(&program, &[], &[]).unwrap_err(),
        EvalFault::IndexOutOfBounds {
            op: "tensor-slice",
            index: 2,
            len: 2,
        }
    );
}

// ─── Modular arithmetic (consolidated) ───

#[test]
fn modular_arithmetic_evaluates() {
    // Factorial, modular inverse, and congruence all exercise the
    // i64 integer path. One test covers the happy paths.
    let prog = program(vec![
        EmirOp::ConstI64(0),                                          // 0
        EmirOp::Factorial(EmirValue(0)),                              // 1: 0! = 1
        EmirOp::ConstI64(5),                                          // 2
        EmirOp::Factorial(EmirValue(2)),                              // 3: 5! = 120
        EmirOp::ConstI64(3),                                          // 4: a
        EmirOp::ConstI64(7),                                          // 5: m
        EmirOp::ModInv(EmirValue(4), EmirValue(5)),                   // 6: 3^(-1) mod 7 = 5
        EmirOp::ConstI64(-1),                                         // 7: a
        EmirOp::ConstI64(6),                                          // 8: b
        EmirOp::ConstI64(7),                                          // 9: m
        EmirOp::Congruence(EmirValue(7), EmirValue(8), EmirValue(9)), // 10: -1 ≡ 6 (mod 7)
    ]);
    let result = evaluate(&prog, &[], &[]).unwrap();
    // result is the last op (congruence) → Bool
    assert_eq!(result, Value::Bool(true));

    // Verify factorial and mod_inv individually
    let p0 = program(vec![EmirOp::ConstI64(0), EmirOp::Factorial(EmirValue(0))]);
    assert_eq!(evaluate(&p0, &[], &[]).unwrap(), Value::I64(1));

    let p5 = program(vec![EmirOp::ConstI64(5), EmirOp::Factorial(EmirValue(0))]);
    assert_eq!(evaluate(&p5, &[], &[]).unwrap(), Value::I64(120));

    let pinv = program(vec![
        EmirOp::ConstI64(3),
        EmirOp::ConstI64(7),
        EmirOp::ModInv(EmirValue(0), EmirValue(1)),
    ]);
    assert_eq!(evaluate(&pinv, &[], &[]).unwrap(), Value::I64(5));
}

#[test]
fn factorial_overflow_guard() {
    let program = program(vec![EmirOp::ConstI64(21), EmirOp::Factorial(EmirValue(0))]);
    assert!(evaluate(&program, &[], &[]).is_err());
}

/// `as i64` maps NaN→0 / Inf→sat / subnormal→0, so factorial would
/// silently return 0! = 1. Whole finite F64 still converts.
#[test]
fn factorial_refuses_nan_inf_subnormal() {
    let whole = program(vec![const_bits(5.0), EmirOp::Factorial(EmirValue(0))]);
    assert_eq!(evaluate(&whole, &[], &[]).unwrap(), Value::I64(120));

    for value in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(1),
    ] {
        let program = program(vec![
            EmirOp::ConstF64(value.to_bits()),
            EmirOp::Factorial(EmirValue(0)),
        ]);
        assert!(
            evaluate(&program, &[], &[]).is_err(),
            "factorial({value:?}) must not become a silent finite i64"
        );
    }
}

#[test]
fn mod_inv_no_inverse_errors() {
    // gcd(2,4)=2, not 1 → no modular inverse exists
    let program = program(vec![
        EmirOp::ConstI64(2),
        EmirOp::ConstI64(4),
        EmirOp::ModInv(EmirValue(0), EmirValue(1)),
    ]);
    assert!(evaluate(&program, &[], &[]).is_err());
}

/// Remaining domain edges after Float64 `sqrt(-1)`/`ln(-1)`/`log(0)`/
/// `factorial(21)`/`mod_inv(0,n)`. Spec: IEEE for libm; GF builtins
/// refuse `p <= 0`; `mod` is the floating remainder.
#[test]
fn remaining_domain_edges_match_documented_policy() {
    let mod0 = program(vec![
        const_bits(1.0),
        const_bits(0.0),
        EmirOp::BinaryBuiltin(BuiltinId::Mod, EmirValue(0), EmirValue(1)),
    ]);
    match evaluate(&mod0, &[], &[]).unwrap() {
        Value::F64(v) => assert!(v.is_nan(), "mod(1,0) must be IEEE NaN, got {v}"),
        other => panic!("mod(1,0) must be F64, got {other:?}"),
    }

    let tan_half_pi = program(vec![
        const_bits(std::f64::consts::FRAC_PI_2),
        EmirOp::UnaryBuiltin(BuiltinId::Tan, EmirValue(0)),
    ]);
    match evaluate(&tan_half_pi, &[], &[]).unwrap() {
        Value::F64(v) => {
            assert!(
                v.is_finite(),
                "tan(π/2) as f64 is IEEE huge-finite (π/2 not exact), got {v}"
            );
            assert!(
                v.abs() > 1e15,
                "tan(π/2) must be the IEEE pole-near huge finite, got {v}"
            );
        }
        other => panic!("tan(π/2) must be F64, got {other:?}"),
    }

    let p0 = program(vec![
        const_bits(1.0),
        EmirOp::VectorCreate(vec![EmirValue(0)]),
        EmirOp::ConstI64(0),
        EmirOp::ConstI64(0),
        EmirOp::PolyEvalMod(EmirValue(1), EmirValue(2), EmirValue(3)),
    ]);
    match evaluate(&p0, &[], &[]) {
        Err(EvalFault::Arithmetic { detail, .. }) => {
            assert!(
                detail.contains("positive"),
                "poly_eval_mod p=0 must name the modulus, got {detail}"
            );
        }
        other => panic!("poly_eval_mod p=0 must named-refuse, got {other:?}"),
    }

    let p1 = program(vec![
        const_bits(5.0),
        EmirOp::VectorCreate(vec![EmirValue(0)]),
        EmirOp::ConstI64(3),
        EmirOp::ConstI64(1),
        EmirOp::PolyEvalMod(EmirValue(1), EmirValue(2), EmirValue(3)),
    ]);
    assert_eq!(
        evaluate(&p1, &[], &[]).unwrap(),
        Value::I64(0),
        "poly_eval_mod(_, _, 1) is the zero ring (everything ≡ 0 mod 1)"
    );

    let inv1 = program(vec![
        EmirOp::ConstI64(1),
        EmirOp::ConstI64(1),
        EmirOp::ModInv(EmirValue(0), EmirValue(1)),
    ]);
    assert_eq!(
        evaluate(&inv1, &[], &[]).unwrap(),
        Value::I64(0),
        "mod_inv(1,1): gcd(0,1)=1 so inverse exists in the zero ring"
    );

    let rs0 = program(vec![
        const_bits(1.0),
        EmirOp::VectorCreate(vec![EmirValue(0)]),
        EmirOp::ConstI64(1),
        EmirOp::ConstI64(0),
        EmirOp::RSEncode(EmirValue(1), EmirValue(2), EmirValue(3)),
    ]);
    match evaluate(&rs0, &[], &[]) {
        Err(EvalFault::Arithmetic { detail, .. }) => {
            assert!(
                detail.contains("positive"),
                "rs_encode p=0 must name the modulus, got {detail}"
            );
        }
        other => panic!("rs_encode p=0 must named-refuse, got {other:?}"),
    }

    let sqrt_i = program(vec![
        EmirOp::ConstComplex(0.0, 1.0),
        EmirOp::UnaryBuiltin(BuiltinId::Sqrt, EmirValue(0)),
    ]);
    let got = match evaluate(&sqrt_i, &[], &[]).unwrap() {
        Value::Complex { re, im } => (re, im),
        other => panic!("sqrt(i) must be Complex, got {other:?}"),
    };
    let s = 0.5_f64.sqrt();
    assert!((got.0 - s).abs() < 1e-12 && (got.1 - s).abs() < 1e-12);
}

// ─── Complex arithmetic (consolidated) ───

#[test]
fn complex_arithmetic_evaluates() {
    // i² = -1 (fundamental identity), multiplication, division, and
    // F64×Complex coercion all in one test.
    let prog = program(vec![
        EmirOp::ConstComplex(0.0, 1.0),             // 0: i
        EmirOp::F64Mul(EmirValue(0), EmirValue(0)), // 1: i*i = -1
        EmirOp::ConstComplex(1.0, 2.0),             // 2
        EmirOp::ConstComplex(3.0, 4.0),             // 3
        EmirOp::F64Mul(EmirValue(2), EmirValue(3)), // 4: (1+2i)(3+4i) = -5+10i
        EmirOp::ConstComplex(1.0, 2.0),             // 5
        EmirOp::ConstComplex(1.0, 1.0),             // 6
        EmirOp::F64Div(EmirValue(5), EmirValue(6)), // 7: (1+2i)/(1+i) = 1.5+0.5i
    ]);
    let result = evaluate(&prog, &[], &[]).unwrap();
    assert_eq!(result, Value::Complex { re: 1.5, im: 0.5 });

    // Verify i² = -1
    let p_isq = program(vec![
        EmirOp::ConstComplex(0.0, 1.0),
        EmirOp::F64Mul(EmirValue(0), EmirValue(0)),
    ]);
    assert_eq!(
        evaluate(&p_isq, &[], &[]).unwrap(),
        Value::Complex { re: -1.0, im: 0.0 }
    );

    // Verify complex multiplication
    let p_mul = program(vec![
        EmirOp::ConstComplex(1.0, 2.0),
        EmirOp::ConstComplex(3.0, 4.0),
        EmirOp::F64Mul(EmirValue(0), EmirValue(1)),
    ]);
    assert_eq!(
        evaluate(&p_mul, &[], &[]).unwrap(),
        Value::Complex { re: -5.0, im: 10.0 }
    );
}

// ─── RS code construction (consolidated) ───

#[test]
fn rs_code_pipeline_evaluates() {
    // Full pipeline: polynomial eval mod p → RS encode → hamming distance →
    // Singleton bound. If any stage is broken, this fails.
    let prog = program(vec![
        // poly_eval_mod: f(x) = 1 + 2x + 3x² over GF(7), f(2) = 17 mod 7 = 3
        const_bits(1.0),
        const_bits(2.0),
        const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3: coeffs
        const_bits(2.0),                                                      // 4: x
        const_bits(7.0),                                                      // 5: p
        EmirOp::PolyEvalMod(EmirValue(3), EmirValue(4), EmirValue(5)),        // 6: f(2) mod 7
        // rs_encode: same poly, n=7, p=7
        const_bits(7.0),                                            // 7: n
        EmirOp::RSEncode(EmirValue(3), EmirValue(7), EmirValue(5)), // 8: codeword
        // hamming_distance: codeword vs itself → 0
        EmirOp::HammingDistance(EmirValue(8), EmirValue(8)), // 9: dist=0
    ]);
    let result = evaluate(&prog, &[], &[]).unwrap();
    assert_eq!(result, Value::I64(0));

    // Verify poly_eval_mod result
    let p_pe = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]),
        const_bits(2.0),
        const_bits(7.0),
        EmirOp::PolyEvalMod(EmirValue(3), EmirValue(4), EmirValue(5)),
    ]);
    assert_eq!(evaluate(&p_pe, &[], &[]).unwrap(), Value::I64(3));

    // Singleton bound: two distinct degree-2 polynomials over GF(7)
    // agree on at most 2 points, so distance >= n-k+1 = 5.
    let p_singleton = program(vec![
        const_bits(1.0),
        const_bits(2.0),
        const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // f1
        const_bits(2.0),
        const_bits(3.0),
        const_bits(1.0),
        EmirOp::VectorCreate(vec![EmirValue(4), EmirValue(5), EmirValue(6)]), // f2
        const_bits(7.0),
        const_bits(7.0),
        EmirOp::RSEncode(EmirValue(3), EmirValue(8), EmirValue(9)), // cw1
        EmirOp::RSEncode(EmirValue(7), EmirValue(8), EmirValue(9)), // cw2
        EmirOp::HammingDistance(EmirValue(10), EmirValue(11)),
    ]);
    let dist = match evaluate(&p_singleton, &[], &[]).unwrap() {
        Value::I64(d) => d,
        other => panic!("expected I64, got {other:?}"),
    };
    assert!(dist >= 5, "RS min distance {} < 5 (Singleton bound)", dist);
}

// ─── sample_limit computation (B04) ───
#[test]
fn sample_limit_sin_x_over_x_approaches_one() {
    // Body sub-program: sin(x) / x where x is input 0.
    let body = EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()),
            (
                EmirOp::UnaryBuiltin(BuiltinId::Sin, EmirValue(0)),
                Span::default(),
            ),
            (EmirOp::LoadInput(0), Span::default()),
            (EmirOp::F64Div(EmirValue(1), EmirValue(2)), Span::default()),
        ],
        result: EmirValue(3),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    // Main program: target=0, direction=0 (two-sided), sample_limit.
    let prog = program(vec![
        const_bits(0.0), // 0: target = 0
        const_bits(0.0), // 1: direction = two-sided
        EmirOp::SampleLimit {
            body,
            var_index: 0,
            target: EmirValue(0),
            direction: EmirValue(1),
        },
    ]);
    let result = evaluate(&prog, &[], &[]).unwrap();
    let val = match result {
        Value::F64(v) => v,
        other => panic!("expected F64, got {other:?}"),
    };
    // sin(x)/x → 1 as x → 0. The numerical approximation should be
    // very close to 1.0 (within 1% tolerance).
    assert!(
        (val - 1.0).abs() < 0.01,
        "sample_limit sin(x)/x as x->0 should be ~1.0, got {val}"
    );
}

#[test]
fn sample_limit_one_sided_from_above() {
    // Body: 1/x where x is input 0. From above (direction=1), as x→0+,
    // 1/x → +inf. The sampler should produce large positive values.
    let body = EmirProgram {
        ops: vec![
            (EmirOp::ConstF64(1.0f64.to_bits()), Span::default()),
            (EmirOp::LoadInput(0), Span::default()),
            (EmirOp::F64Div(EmirValue(0), EmirValue(1)), Span::default()),
        ],
        result: EmirValue(2),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = program(vec![
        const_bits(0.0), // target = 0
        const_bits(1.0), // direction = from above
        EmirOp::SampleLimit {
            body,
            var_index: 0,
            target: EmirValue(0),
            direction: EmirValue(1),
        },
    ]);
    let result = evaluate(&prog, &[], &[]).unwrap();
    let val = match result {
        Value::F64(v) => v,
        other => panic!("expected F64, got {other:?}"),
    };
    // 1/x as x→0+ grows without bound. The sampler returns the last
    // finite value before convergence or the best estimate.
    // It should be a large positive number.
    assert!(val > 1e5, "1/x as x->0+ should be very large, got {val}");
}

// ─── reverse-mode AD ───

#[test]
fn reverse_mode_quadratic_gradient() {
    // f(x, y) = x*y + y*y
    // df/dx = y, df/dy = x + 2*y
    // At x=3, y=2: df/dx = 2, df/dy = 7
    let body = EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()), // 0: x
            (EmirOp::LoadInput(1), Span::default()), // 1: y
            (EmirOp::F64Mul(EmirValue(0), EmirValue(1)), Span::default()), // 2: x*y
            (EmirOp::LoadInput(1), Span::default()), // 3: y
            (EmirOp::LoadInput(1), Span::default()), // 4: y
            (EmirOp::F64Mul(EmirValue(3), EmirValue(4)), Span::default()), // 5: y*y
            (EmirOp::F64Add(EmirValue(2), EmirValue(5)), Span::default()), // 6: x*y + y*y
        ],
        result: EmirValue(6),
        input_count: 2,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = program(vec![EmirOp::ReverseMode {
        body,
        var_indices: vec![0, 1],
    }]);
    let result = evaluate(&prog, &[Value::F64(3.0), Value::F64(2.0)], &[]).unwrap();
    let grads = match result {
        Value::Vector(v) => v,
        other => panic!("expected Vector, got {other:?}"),
    };
    assert_eq!(grads.len(), 2, "should have 2 gradients");
    assert!(
        (grads[0] - 2.0).abs() < 1e-10,
        "df/dx should be 2.0, got {}",
        grads[0]
    );
    assert!(
        (grads[1] - 7.0).abs() < 1e-10,
        "df/dy should be 7.0, got {}",
        grads[1]
    );
}

#[test]
fn reverse_mode_ten_inputs_matches_forward() {
    // f(x1,...,x10) = sum(xi^2)
    // df/dxi = 2*xi
    // At xi = (i+1): gradients = [2, 4, 6, 8, 10, 12, 14, 16, 18, 20]
    let n: usize = 10;
    let mut ops = Vec::new();
    for i in 0..n {
        ops.push((EmirOp::LoadInput(i as u16), Span::default()));
        ops.push((
            EmirOp::F64Mul(EmirValue(2 * i as u32), EmirValue(2 * i as u32)),
            Span::default(),
        ));
    }
    // Sum: start with x0^2, then add each subsequent square.
    let mut acc = EmirValue(1); // first square at index 1
    for i in 1..n {
        let sq_idx = 2 * i as u32 + 1;
        ops.push((EmirOp::F64Add(acc, EmirValue(sq_idx)), Span::default()));
        acc = EmirValue(ops.len() as u32 - 1);
    }
    let body = EmirProgram {
        ops,
        result: acc,
        input_count: n as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let var_indices: Vec<u16> = (0..n as u16).collect();
    let prog = program(vec![EmirOp::ReverseMode { body, var_indices }]);
    let inputs: Vec<Value> = (1..=n).map(|i| Value::F64(i as f64)).collect();
    let result = evaluate(&prog, &inputs, &[]).unwrap();
    let grads = match result {
        Value::Vector(v) => v,
        other => panic!("expected Vector, got {other:?}"),
    };
    assert_eq!(grads.len(), n, "should have {n} gradients");
    for i in 0..n {
        let expected = 2.0 * (i + 1) as f64;
        assert!(
            (grads[i] - expected).abs() < 1e-10,
            "df/dx{} should be {expected}, got {}",
            i + 1,
            grads[i]
        );
    }
}

#[test]
fn reverse_mode_transcendental_gradient() {
    // f(x, y) = sin(x) * exp(y)
    // df/dx = cos(x) * exp(y)
    // df/dy = sin(x) * exp(y)
    // At x=1.0, y=0.5:
    //   df/dx = cos(1.0) * exp(0.5) ≈ 0.5403 * 1.6487 ≈ 0.8910
    //   df/dy = sin(1.0) * exp(0.5) ≈ 0.8415 * 1.6487 ≈ 1.3878
    let body = EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()), // 0: x
            (
                EmirOp::UnaryBuiltin(BuiltinId::Sin, EmirValue(0)),
                Span::default(),
            ), // 1: sin(x)
            (EmirOp::LoadInput(1), Span::default()), // 2: y
            (
                EmirOp::UnaryBuiltin(BuiltinId::Exp, EmirValue(2)),
                Span::default(),
            ), // 3: exp(y)
            (EmirOp::F64Mul(EmirValue(1), EmirValue(3)), Span::default()), // 4: sin(x)*exp(y)
        ],
        result: EmirValue(4),
        input_count: 2,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = program(vec![EmirOp::ReverseMode {
        body,
        var_indices: vec![0, 1],
    }]);
    let x = 1.0_f64;
    let y = 0.5_f64;
    let result = evaluate(&prog, &[Value::F64(x), Value::F64(y)], &[]).unwrap();
    let grads = match result {
        Value::Vector(v) => v,
        other => panic!("expected Vector, got {other:?}"),
    };
    let expected_dx = x.cos() * y.exp();
    let expected_dy = x.sin() * y.exp();
    assert!(
        (grads[0] - expected_dx).abs() < 1e-10,
        "df/dx should be {expected_dx}, got {}",
        grads[0]
    );
    assert!(
        (grads[1] - expected_dy).abs() < 1e-10,
        "df/dy should be {expected_dy}, got {}",
        grads[1]
    );
}

fn scalar_body(ops: Vec<EmirOp>, input_count: u16) -> EmirProgram {
    let last = u32::try_from(ops.len().saturating_sub(1)).unwrap_or(0);
    EmirProgram {
        ops: ops.into_iter().map(|op| (op, Span::default())).collect(),
        result: EmirValue(last),
        input_count,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

/// Forward-mode tangent and reverse-mode adjoint for the same scalar body.
fn adjoint_pair(body: EmirProgram, inputs: &[Value], var: u16) -> (f64, f64) {
    let dual_prog = program(vec![EmirOp::Differentiate {
        body: body.clone(),
        var_index: var,
    }]);
    let fwd = match evaluate(&dual_prog, inputs, &[]).unwrap() {
        Value::F64(v) => v,
        other => panic!("expected F64 tangent, got {other:?}"),
    };
    let rev_prog = program(vec![EmirOp::ReverseMode {
        body,
        var_indices: vec![var],
    }]);
    let rev = match evaluate(&rev_prog, inputs, &[]).unwrap() {
        Value::Vector(v) => v[0],
        other => panic!("expected Vector adjoint, got {other:?}"),
    };
    (fwd, rev)
}

fn assert_adjoint_eq(fwd: f64, rev: f64, expected: f64, label: &str) {
    assert!(
        (fwd == expected) || (fwd.is_nan() && expected.is_nan()),
        "{label}: dual {fwd} != closed form {expected}"
    );
    assert!(
        (rev == expected) || (rev.is_nan() && expected.is_nan()),
        "{label}: reverse {rev} != closed form {expected}"
    );
    assert!(
        (fwd == rev) || (fwd.is_nan() && rev.is_nan()),
        "{label}: dual {fwd} != reverse {rev}"
    );
}

/// d/dx[x^n] at x=0: reverse used to skip the base adjoint, so x^1
/// returned 0 instead of the closed form 1. x^0 is identically 1.
#[test]
fn adjoint_identity_pow_integer_exponent_at_zero() {
    let pow_body = |exp: f64| {
        scalar_body(
            vec![
                EmirOp::LoadInput(0),
                const_bits(exp),
                EmirOp::F64Pow(EmirValue(0), EmirValue(1)),
            ],
            1,
        )
    };
    let x0 = [Value::F64(0.0)];
    let (d1, r1) = adjoint_pair(pow_body(1.0), &x0, 0);
    assert_adjoint_eq(d1, r1, 1.0, "d/dx[x^1] at 0");
    let (d2, r2) = adjoint_pair(pow_body(2.0), &x0, 0);
    assert_adjoint_eq(d2, r2, 0.0, "d/dx[x^2] at 0");
    let (d0, r0) = adjoint_pair(pow_body(0.0), &x0, 0);
    assert_adjoint_eq(d0, r0, 0.0, "d/dx[x^0] at 0");
}

/// abs'(0) = sgn(0) = 0 in this crate, not IEEE signum(+0)=1.
#[test]
fn adjoint_identity_abs_at_zero() {
    let body = scalar_body(
        vec![
            EmirOp::LoadInput(0),
            EmirOp::UnaryBuiltin(BuiltinId::Abs, EmirValue(0)),
        ],
        1,
    );
    let (fwd, rev) = adjoint_pair(body, &[Value::F64(0.0)], 0);
    assert_adjoint_eq(fwd, rev, 0.0, "d/dx abs(x) at 0");
}

/// Dual atan2 used (1+(y/x)^2) which is 0/0 at x=0; closed form is
/// ∂/∂x atan2(y,x) = -y/(x²+y²) = -1 at (1,0).
#[test]
fn adjoint_identity_atan2_at_x_zero() {
    let body = scalar_body(
        vec![
            const_bits(1.0),
            EmirOp::LoadInput(0),
            EmirOp::BinaryBuiltin(BuiltinId::Atan2, EmirValue(0), EmirValue(1)),
        ],
        1,
    );
    let (fwd, rev) = adjoint_pair(body, &[Value::F64(0.0)], 0);
    assert_adjoint_eq(fwd, rev, -1.0, "d/dx atan2(1, x) at 0");
}

/// Reverse used to zero recip/sqrt at 0; dual and 1/x use IEEE Inf.
#[test]
fn adjoint_identity_recip_sqrt_at_zero() {
    let recip = scalar_body(
        vec![
            EmirOp::LoadInput(0),
            EmirOp::UnaryBuiltin(BuiltinId::Recip, EmirValue(0)),
        ],
        1,
    );
    let (df, rf) = adjoint_pair(recip, &[Value::F64(0.0)], 0);
    assert_adjoint_eq(df, rf, f64::NEG_INFINITY, "d/dx recip(x) at 0");

    let sqrt = scalar_body(
        vec![
            EmirOp::LoadInput(0),
            EmirOp::UnaryBuiltin(BuiltinId::Sqrt, EmirValue(0)),
        ],
        1,
    );
    let (ds, rs) = adjoint_pair(sqrt, &[Value::F64(0.0)], 0);
    assert_adjoint_eq(ds, rs, f64::INFINITY, "d/dx sqrt(x) at 0");
}

/// hypot and min at a kink: dual and reverse already shared a convention;
/// keep the identity pinned.
#[test]
fn adjoint_identity_hypot_and_min_kink() {
    let hypot = scalar_body(
        vec![
            EmirOp::LoadInput(0),
            const_bits(4.0),
            EmirOp::BinaryBuiltin(BuiltinId::Hypot, EmirValue(0), EmirValue(1)),
        ],
        1,
    );
    let (dh, rh) = adjoint_pair(hypot, &[Value::F64(3.0)], 0);
    assert_adjoint_eq(dh, rh, 3.0 / 5.0, "d/dx hypot(x, 4) at 3");

    let min_kink = scalar_body(
        vec![
            EmirOp::LoadInput(0),
            const_bits(5.0),
            EmirOp::BinaryBuiltin(BuiltinId::Min, EmirValue(0), EmirValue(1)),
        ],
        1,
    );
    let (dm, rm) = adjoint_pair(min_kink, &[Value::F64(5.0)], 0);
    assert_adjoint_eq(dm, rm, 1.0, "d/dx min(x, 5) at 5 (left)");
}

#[test]
fn adjoint_identity_select() {
    // if x > 0 then x*x else -x; at x=2, d/dx = 2x = 4.
    let body = scalar_body(
        vec![
            EmirOp::LoadInput(0),
            const_bits(0.0),
            EmirOp::Gt(EmirValue(0), EmirValue(1)),
            EmirOp::F64Mul(EmirValue(0), EmirValue(0)),
            EmirOp::Neg(EmirValue(0)),
            EmirOp::Select {
                condition: EmirValue(2),
                then_value: EmirValue(3),
                else_value: EmirValue(4),
            },
        ],
        1,
    );
    let (fwd, rev) = adjoint_pair(body, &[Value::F64(2.0)], 0);
    assert_adjoint_eq(fwd, rev, 4.0, "d/dx select(x>0, x*x, -x) at 2");
}

/// Metamorphic involution: `f(f⁻¹(x)) == x` where the inverse is defined.
/// `i64::MIN` negate must named-fault (two's-complement has no `−MIN`), not wrap.
#[test]
fn invertible_ops_are_involutions() {
    // Negate: −(−x) = x on I64 except MIN; MIN is a typed overflow.
    for x in [0i64, 1, -1, 42, i64::MAX, i64::MAX - 1, -i64::MAX] {
        let p = program(vec![
            EmirOp::ConstI64(x),
            EmirOp::Neg(EmirValue(0)),
            EmirOp::Neg(EmirValue(1)),
        ]);
        assert_eq!(
            evaluate(&p, &[], &[]).unwrap(),
            Value::I64(x),
            "-(-{x}) must be {x}"
        );
    }
    let min_neg = program(vec![EmirOp::ConstI64(i64::MIN), EmirOp::Neg(EmirValue(0))]);
    assert_eq!(
        evaluate(&min_neg, &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "neg",
            detail: "i64 overflow",
        },
        "-I64::MIN must named-fault, not wrap to itself"
    );
    let min_twice = program(vec![
        EmirOp::ConstI64(i64::MIN),
        EmirOp::Neg(EmirValue(0)),
        EmirOp::Neg(EmirValue(1)),
    ]);
    assert_eq!(
        evaluate(&min_twice, &[], &[]).unwrap_err(),
        EvalFault::Arithmetic {
            op: "neg",
            detail: "i64 overflow",
        },
        "-(-I64::MIN) must not wrap-succeed"
    );

    // recip(recip(x)) == x for finite x whose reciprocal is finite and exact.
    for x in [1.0, -1.0, 2.0, 0.5, 4.0, 0.25, 8.0, -4.0, 0.125] {
        let p = program(vec![
            const_bits(x),
            EmirOp::UnaryBuiltin(BuiltinId::Recip, EmirValue(0)),
            EmirOp::UnaryBuiltin(BuiltinId::Recip, EmirValue(1)),
        ]);
        match evaluate(&p, &[], &[]).unwrap() {
            Value::F64(y) => assert_eq!(y.to_bits(), x.to_bits(), "recip(recip({x})) bits"),
            other => panic!("expected F64, got {other:?}"),
        }
    }

    // transpose(transpose(A)) == A, including 0-width and 0-height.
    for (rows, cols, data) in [
        (1usize, 1usize, vec![7.0]),
        (2, 2, vec![1.0, 2.0, 3.0, 4.0]),
        (2, 3, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
        (3, 1, vec![1.0, 2.0, 3.0]),
        (1, 4, vec![1.0, 2.0, 3.0, 4.0]),
        (2, 0, vec![]),
        (0, 3, vec![]),
        (0, 0, vec![]),
    ] {
        let mut ops = Vec::new();
        let a = push_matrix(&mut ops, rows, cols, &data);
        ops.push(EmirOp::MatrixTranspose(EmirValue(a)));
        ops.push(EmirOp::MatrixTranspose(EmirValue(a + 1)));
        assert_eq!(
            eval_ops(ops),
            Value::Matrix {
                rows,
                cols,
                data: data.clone(),
            },
            "transpose² of {rows}x{cols}"
        );
    }

    // mod_inv(mod_inv(a, p), p) == a when gcd(a, p)=1 and a ∈ (0, p).
    for p in [2i64, 3, 7, 11, 13, 101, 1009] {
        for a in 1..p {
            let pinv = program(vec![
                EmirOp::ConstI64(a),
                EmirOp::ConstI64(p),
                EmirOp::ModInv(EmirValue(0), EmirValue(1)),
                EmirOp::ConstI64(p),
                EmirOp::ModInv(EmirValue(2), EmirValue(3)),
            ]);
            match evaluate(&pinv, &[], &[]) {
                Ok(Value::I64(back)) => {
                    assert_eq!(back, a, "mod_inv²({a}, {p})");
                }
                Ok(other) => panic!("expected I64, got {other:?}"),
                Err(_) => {
                    // gcd != 1: skip (not defined)
                }
            }
        }
    }
}

#[test]
fn complex_sqrt_ln_principal_branch() {
    let sqrt_neg1 = program(vec![
        EmirOp::ConstComplex(-1.0, 0.0),
        EmirOp::UnaryBuiltin(BuiltinId::Sqrt, EmirValue(0)),
    ]);
    match evaluate(&sqrt_neg1, &[], &[]).unwrap() {
        Value::Complex { re, im } => {
            assert!(re.abs() < 1e-12, "re={re}");
            assert!((im - 1.0).abs() < 1e-12, "im={im}");
        }
        other => panic!("{other:?}"),
    }
    let ln_neg1 = program(vec![
        EmirOp::ConstComplex(-1.0, 0.0),
        EmirOp::UnaryBuiltin(BuiltinId::Ln, EmirValue(0)),
    ]);
    match evaluate(&ln_neg1, &[], &[]).unwrap() {
        Value::Complex { re, im } => {
            assert!(re.abs() < 1e-12, "re={re}");
            assert!((im - std::f64::consts::PI).abs() < 1e-12, "im={im}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn vectordot_adjoint_identity() {
    // d/dx dot([x, 1], [1, x]) = d/dx (2x) = 2
    let body = scalar_body(
        vec![
            EmirOp::LoadInput(0),
            const_bits(1.0),
            EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1)]),
            EmirOp::VectorCreate(vec![EmirValue(1), EmirValue(0)]),
            EmirOp::VectorDot(EmirValue(2), EmirValue(3)),
        ],
        1,
    );
    let (fwd, rev) = adjoint_pair(body, &[Value::F64(3.0)], 0);
    assert_adjoint_eq(fwd, rev, 2.0, "d/dx dot([x,1],[1,x])");
}
