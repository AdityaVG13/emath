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
            EmirOp::Differentiate {
                body,
                var_index: 0,
            },
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
            EmirOp::Differentiate {
                body,
                var_index: 0,
            },
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
    let program = program(vec![const_bits(1.0), EmirOp::IsFinite(EmirValue(0))]);
    assert_eq!(evaluate(&program, &[], &[]).unwrap(), Value::Bool(true));
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

#[test]
fn optimize_refuses_max_iter_without_stationarity() {
    // Constant nonzero gradient: f(x) = x, df = 1. Tiny learning rate
    // and tight tolerance ensure max_iter exhausts without |grad| < tol.
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
        learning_rate: 1e-12,
        tolerance: 1e-8,
        max_iter: 3,
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
        EdgePolicy::Dirichlet { left: 5.0, right: 5.0 },
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
        EdgePolicy::Dirichlet { left: 0.0, right: 0.0 },
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
fn gradient_linear_field_is_one_interior() {
    // u = [0,1,2,3,4] (slope 1). The central-difference gradient is 1 on
    // the interior and 0.5 at the clamped (one-sided) boundaries.
    let weights = vec![-0.5, 0.0, 0.5];
    let prog = stencil_prog(weights, 1, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    assert_eq!(
        evaluate(&prog, &[], &[]).unwrap(),
        Value::Vector(vec![0.5, 1.0, 1.0, 1.0, 0.5])
    );
}

#[test]
fn gradient_2d_x_linear_in_columns() {
    // u[r][c] = c (increasing along columns). du/dc is 1 on the interior
    // and 0.5 at the clamped left/right edges; constant along rows.
    let data = vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0];
    let weights = vec![0.0, 0.0, 0.0, -0.5, 0.0, 0.5, 0.0, 0.0, 0.0];
    let prog = stencil2d_prog(weights, (1, 1), 3, 3, data, EdgePolicy::Clamp);
    let expected = vec![0.5, 1.0, 0.5, 0.5, 1.0, 0.5, 0.5, 1.0, 0.5];
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
    // u[r][c] = r (increasing along rows). du/dr is 1 on the interior and
    // 0.5 at the clamped top/bottom edges; constant along columns.
    let data = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0];
    let weights = vec![0.0, -0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0];
    let prog = stencil2d_prog(weights, (1, 1), 3, 3, data, EdgePolicy::Clamp);
    let expected = vec![0.5, 0.5, 0.5, 1.0, 1.0, 1.0, 0.5, 0.5, 0.5];
    assert_eq!(
        evaluate(&prog, &[], &[]).unwrap(),
        Value::Matrix {
            rows: 3,
            cols: 3,
            data: expected
        }
    );
}

// ---- B12: logic connectives evaluation ----------------------------------

#[test]
fn b12_imply_truth_table() {
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
fn b12_iff_truth_table() {
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
        const_bits(1.0), // 0
        const_bits(2.0), // 1
        const_bits(3.0), // 2
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3: a
        const_bits(4.0), // 4
        const_bits(5.0), // 5
        const_bits(6.0), // 6
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
                EmirValue(0), EmirValue(1), EmirValue(2),
                EmirValue(3), EmirValue(4), EmirValue(5),
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

// ─── Modular arithmetic (consolidated) ───

#[test]
fn modular_arithmetic_evaluates() {
    // Factorial, modular inverse, and congruence all exercise the
    // i64 integer path. One test covers the happy paths.
    let prog = program(vec![
        EmirOp::ConstI64(0),              // 0
        EmirOp::Factorial(EmirValue(0)),  // 1: 0! = 1
        EmirOp::ConstI64(5),              // 2
        EmirOp::Factorial(EmirValue(2)),  // 3: 5! = 120
        EmirOp::ConstI64(3),              // 4: a
        EmirOp::ConstI64(7),              // 5: m
        EmirOp::ModInv(EmirValue(4), EmirValue(5)), // 6: 3^(-1) mod 7 = 5
        EmirOp::ConstI64(-1),             // 7: a
        EmirOp::ConstI64(6),              // 8: b
        EmirOp::ConstI64(7),              // 9: m
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
        EmirOp::ConstI64(3), EmirOp::ConstI64(7),
        EmirOp::ModInv(EmirValue(0), EmirValue(1)),
    ]);
    assert_eq!(evaluate(&pinv, &[], &[]).unwrap(), Value::I64(5));
}

#[test]
fn factorial_overflow_guard() {
    let program = program(vec![
        EmirOp::ConstI64(21),
        EmirOp::Factorial(EmirValue(0)),
    ]);
    assert!(evaluate(&program, &[], &[]).is_err());
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

// ─── Complex arithmetic (consolidated) ───

#[test]
fn complex_arithmetic_evaluates() {
    // i² = -1 (fundamental identity), multiplication, division, and
    // F64×Complex coercion all in one test.
    let prog = program(vec![
        EmirOp::ConstComplex(0.0, 1.0),                    // 0: i
        EmirOp::F64Mul(EmirValue(0), EmirValue(0)),        // 1: i*i = -1
        EmirOp::ConstComplex(1.0, 2.0),                    // 2
        EmirOp::ConstComplex(3.0, 4.0),                    // 3
        EmirOp::F64Mul(EmirValue(2), EmirValue(3)),        // 4: (1+2i)(3+4i) = -5+10i
        EmirOp::ConstComplex(1.0, 2.0),                    // 5
        EmirOp::ConstComplex(1.0, 1.0),                    // 6
        EmirOp::F64Div(EmirValue(5), EmirValue(6)),        // 7: (1+2i)/(1+i) = 1.5+0.5i
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
        const_bits(1.0), const_bits(2.0), const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3: coeffs
        const_bits(2.0), // 4: x
        const_bits(7.0), // 5: p
        EmirOp::PolyEvalMod(EmirValue(3), EmirValue(4), EmirValue(5)),        // 6: f(2) mod 7

        // rs_encode: same poly, n=7, p=7
        const_bits(7.0), // 7: n
        EmirOp::RSEncode(EmirValue(3), EmirValue(7), EmirValue(5)),           // 8: codeword

        // hamming_distance: codeword vs itself → 0
        EmirOp::HammingDistance(EmirValue(8), EmirValue(8)),                  // 9: dist=0
    ]);
    let result = evaluate(&prog, &[], &[]).unwrap();
    assert_eq!(result, Value::I64(0));

    // Verify poly_eval_mod result
    let p_pe = program(vec![
        const_bits(1.0), const_bits(2.0), const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]),
        const_bits(2.0), const_bits(7.0),
        EmirOp::PolyEvalMod(EmirValue(3), EmirValue(4), EmirValue(5)),
    ]);
    assert_eq!(evaluate(&p_pe, &[], &[]).unwrap(), Value::I64(3));

    // Singleton bound: two distinct degree-2 polynomials over GF(7)
    // agree on at most 2 points, so distance >= n-k+1 = 5.
    let p_singleton = program(vec![
        const_bits(1.0), const_bits(2.0), const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // f1
        const_bits(2.0), const_bits(3.0), const_bits(1.0),
        EmirOp::VectorCreate(vec![EmirValue(4), EmirValue(5), EmirValue(6)]), // f2
        const_bits(7.0), const_bits(7.0),
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
            (EmirOp::UnaryBuiltin(BuiltinId::Sin, EmirValue(0)), Span::default()),
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
        const_bits(0.0),                // 0: target = 0
        const_bits(0.0),                // 1: direction = two-sided
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
        const_bits(0.0),                // target = 0
        const_bits(1.0),                // direction = from above
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
    assert!(
        val > 1e5,
        "1/x as x->0+ should be very large, got {val}"
    );
}

// ─── reverse-mode AD (emath-xx0x.1) ───

#[test]
fn reverse_mode_quadratic_gradient() {
    // f(x, y) = x*y + y*y
    // df/dx = y, df/dy = x + 2*y
    // At x=3, y=2: df/dx = 2, df/dy = 7
    let body = EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), Span::default()),      // 0: x
            (EmirOp::LoadInput(1), Span::default()),      // 1: y
            (EmirOp::F64Mul(EmirValue(0), EmirValue(1)), Span::default()), // 2: x*y
            (EmirOp::LoadInput(1), Span::default()),      // 3: y
            (EmirOp::LoadInput(1), Span::default()),      // 4: y
            (EmirOp::F64Mul(EmirValue(3), EmirValue(4)), Span::default()), // 5: y*y
            (EmirOp::F64Add(EmirValue(2), EmirValue(5)), Span::default()), // 6: x*y + y*y
        ],
        result: EmirValue(6),
        input_count: 2,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = program(vec![
        EmirOp::ReverseMode {
            body,
            var_indices: vec![0, 1],
        },
    ]);
    let result = evaluate(&prog, &[Value::F64(3.0), Value::F64(2.0)], &[]).unwrap();
    let grads = match result {
        Value::Vector(v) => v,
        other => panic!("expected Vector, got {other:?}"),
    };
    assert_eq!(grads.len(), 2, "should have 2 gradients");
    assert!(
        (grads[0] - 2.0).abs() < 1e-10,
        "df/dx should be 2.0, got {}", grads[0]
    );
    assert!(
        (grads[1] - 7.0).abs() < 1e-10,
        "df/dy should be 7.0, got {}", grads[1]
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
        ops.push((EmirOp::F64Mul(
            EmirValue(2 * i as u32),
            EmirValue(2 * i as u32),
        ), Span::default()));
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
    let prog = program(vec![
        EmirOp::ReverseMode { body, var_indices },
    ]);
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
            "df/dx{} should be {expected}, got {}", i + 1, grads[i]
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
            (EmirOp::LoadInput(0), Span::default()),      // 0: x
            (EmirOp::UnaryBuiltin(BuiltinId::Sin, EmirValue(0)), Span::default()),  // 1: sin(x)
            (EmirOp::LoadInput(1), Span::default()),      // 2: y
            (EmirOp::UnaryBuiltin(BuiltinId::Exp, EmirValue(2)), Span::default()),  // 3: exp(y)
            (EmirOp::F64Mul(EmirValue(1), EmirValue(3)), Span::default()), // 4: sin(x)*exp(y)
        ],
        result: EmirValue(4),
        input_count: 2,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    let prog = program(vec![
        EmirOp::ReverseMode {
            body,
            var_indices: vec![0, 1],
        },
    ]);
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
        "df/dx should be {expected_dx}, got {}", grads[0]
    );
    assert!(
        (grads[1] - expected_dy).abs() < 1e-10,
        "df/dy should be {expected_dy}, got {}", grads[1]
    );
}
