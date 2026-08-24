use emath_core::Span;
use emath_exec_ir::interp::{EvalFault, Value, evaluate};
use emath_exec_ir::{EdgePolicy, EmirOp, EmirProgram, EmirValue, FoldCombine};

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

// ─── Finite field / modular arithmetic tests (B15/B29/B40) ───

#[test]
fn factorial_basic() {
    let program = program(vec![
        EmirOp::ConstI64(5),        // 0: n = 5
        EmirOp::Factorial(EmirValue(0)), // 1: 5! = 120
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::I64(120));
}

#[test]
fn factorial_zero_and_one() {
    let p0 = program(vec![EmirOp::ConstI64(0), EmirOp::Factorial(EmirValue(0))]);
    assert_eq!(evaluate(&p0, &[], &[]).unwrap(), Value::I64(1));

    let p1 = program(vec![EmirOp::ConstI64(1), EmirOp::Factorial(EmirValue(0))]);
    assert_eq!(evaluate(&p1, &[], &[]).unwrap(), Value::I64(1));
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
fn mod_inv_basic() {
    // mod_inv(3, 7) = 5 because 3*5 = 15 ≡ 1 (mod 7)
    let program = program(vec![
        EmirOp::ConstI64(3),              // 0: a = 3
        EmirOp::ConstI64(7),              // 1: m = 7
        EmirOp::ModInv(EmirValue(0), EmirValue(1)), // 2: 3^(-1) mod 7 = 5
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::I64(5));
}

#[test]
fn mod_inv_no_inverse() {
    // mod_inv(2, 4) should error — gcd(2,4)=2, not 1
    let program = program(vec![
        EmirOp::ConstI64(2),
        EmirOp::ConstI64(4),
        EmirOp::ModInv(EmirValue(0), EmirValue(1)),
    ]);
    assert!(evaluate(&program, &[], &[]).is_err());
}

#[test]
fn congruence_true() {
    // cong(17, 5, 12) → (17-5) % 12 = 0 → true
    let program = program(vec![
        EmirOp::ConstI64(17),  // 0: a
        EmirOp::ConstI64(5),   // 1: b
        EmirOp::ConstI64(12),  // 2: m
        EmirOp::Congruence(EmirValue(0), EmirValue(1), EmirValue(2)), // 3
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn congruence_false() {
    // cong(17, 6, 12) → (17-6) % 12 = 11 ≠ 0 → false
    let program = program(vec![
        EmirOp::ConstI64(17),  // 0: a
        EmirOp::ConstI64(6),   // 1: b
        EmirOp::ConstI64(12),  // 2: m
        EmirOp::Congruence(EmirValue(0), EmirValue(1), EmirValue(2)), // 3
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn congruence_negative() {
    // cong(-1, 6, 7) → (-1-6) rem_euclid 7 = (-7) rem_euclid 7 = 0 → true
    let program = program(vec![
        EmirOp::ConstI64(-1),  // 0: a
        EmirOp::ConstI64(6),   // 1: b
        EmirOp::ConstI64(7),   // 2: m
        EmirOp::Congruence(EmirValue(0), EmirValue(1), EmirValue(2)), // 3
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn wilsons_theorem_prime_7() {
    // Wilson's theorem: (p-1)! ≡ -1 (mod p) for prime p
    // For p=7: 6! = 720, 720 mod 7 = 6 = -1 mod 7
    let program = program(vec![
        EmirOp::ConstI64(6),             // 0: n = 6
        EmirOp::Factorial(EmirValue(0)), // 1: 6! = 720
        EmirOp::ConstI64(7),             // 2: p = 7
        EmirOp::Mod(EmirValue(1), EmirValue(2)), // 3: 720 % 7 = 6
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::F64(6.0));
}

// ─── Complex number tests (B14) ───

#[test]
fn complex_imaginary_unit() {
    let program = program(vec![
        EmirOp::ConstComplex(0.0, 1.0), // 0: i
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Complex { re: 0.0, im: 1.0 });
}

#[test]
fn complex_scalar_mul() {
    // 2 * i = 2i
    let program = program(vec![
        const_bits(2.0),                    // 0: 2
        EmirOp::ConstComplex(0.0, 1.0),    // 1: i
        EmirOp::F64Mul(EmirValue(0), EmirValue(1)), // 2: 2 * i
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Complex { re: 0.0, im: 2.0 });
}

#[test]
fn complex_add() {
    // (1 + 0i) + (0 + 2i) = 1 + 2i
    let program = program(vec![
        const_bits(1.0),                    // 0: 1 (as f64)
        EmirOp::ConstComplex(0.0, 2.0),    // 1: 2i
        EmirOp::F64Add(EmirValue(0), EmirValue(1)), // 2: 1 + 2i
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Complex { re: 1.0, im: 2.0 });
}

#[test]
fn complex_mul() {
    // (1 + 2i) * (3 + 4i) = (3 - 8) + (4 + 6)i = -5 + 10i
    let program = program(vec![
        EmirOp::ConstComplex(1.0, 2.0),    // 0
        EmirOp::ConstComplex(3.0, 4.0),    // 1
        EmirOp::F64Mul(EmirValue(0), EmirValue(1)), // 2
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Complex { re: -5.0, im: 10.0 });
}

#[test]
fn complex_div() {
    // (1 + 2i) / (1 + 1i) = ((1+2) + (2-1)i) / (1+1) = 1.5 + 0.5i
    let program = program(vec![
        EmirOp::ConstComplex(1.0, 2.0),    // 0
        EmirOp::ConstComplex(1.0, 1.0),    // 1
        EmirOp::F64Div(EmirValue(0), EmirValue(1)), // 2
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Complex { re: 1.5, im: 0.5 });
}

#[test]
fn complex_neg() {
    let program = program(vec![
        EmirOp::ConstComplex(3.0, 4.0),    // 0
        EmirOp::Neg(EmirValue(0)),         // 1: -(3+4i) = -3-4i
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Complex { re: -3.0, im: -4.0 });
}

#[test]
fn complex_eq() {
    let program = program(vec![
        EmirOp::ConstComplex(1.0, 2.0),    // 0
        EmirOp::ConstComplex(1.0, 2.0),    // 1
        EmirOp::Eq(EmirValue(0), EmirValue(1)), // 2
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn complex_ne() {
    let program = program(vec![
        EmirOp::ConstComplex(1.0, 2.0),    // 0
        EmirOp::ConstComplex(1.0, 3.0),    // 1
        EmirOp::Ne(EmirValue(0), EmirValue(1)), // 2
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn complex_i_squared() {
    // i * i = -1
    let program = program(vec![
        EmirOp::ConstComplex(0.0, 1.0),    // 0: i
        EmirOp::F64Mul(EmirValue(0), EmirValue(0)), // 1: i * i = -1
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::Complex { re: -1.0, im: 0.0 });
}

// ─── Reed-Solomon code construction tests ───

#[test]
fn poly_eval_mod_basic() {
    // f(x) = 1 + 2x + 3x^2 over GF(7)
    // f(2) = 1 + 4 + 12 = 17 mod 7 = 3
    let program = program(vec![
        const_bits(1.0), // 0: c0
        const_bits(2.0), // 1: c1
        const_bits(3.0), // 2: c2
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3: coeffs
        const_bits(2.0), // 4: x = 2
        const_bits(7.0), // 5: p = 7
        EmirOp::PolyEvalMod(EmirValue(3), EmirValue(4), EmirValue(5)), // 6
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::I64(3));
}

#[test]
fn poly_eval_mod_horner() {
    // f(x) = 3 + x + 2x^2 over GF(5)
    // f(3) = 3 + 3 + 18 = 24 mod 5 = 4
    let program = program(vec![
        const_bits(3.0), // 0
        const_bits(1.0), // 1
        const_bits(2.0), // 2
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3
        const_bits(3.0), // 4: x
        const_bits(5.0), // 5: p
        EmirOp::PolyEvalMod(EmirValue(3), EmirValue(4), EmirValue(5)), // 6
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::I64(4));
}

#[test]
fn rs_encode_basic() {
    // RS(7, 3) over GF(7): f(x) = 1 + 2x + 3x^2
    // Codeword = [f(0), f(1), f(2), f(3), f(4), f(5), f(6)] mod 7
    // f(0) = 1, f(1) = 6, f(2) = 3, f(3) = 6, f(4) = 4, f(5) = 1, f(6) = 1
    let program = program(vec![
        const_bits(1.0), // 0
        const_bits(2.0), // 1
        const_bits(3.0), // 2
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3: coeffs
        const_bits(7.0), // 4: n = 7
        const_bits(7.0), // 5: p = 7
        EmirOp::RSEncode(EmirValue(3), EmirValue(4), EmirValue(5)), // 6
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    // f(x) = 1 + 2x + 3x^2 mod 7:
    // f(0)=1, f(1)=6, f(2)=17%7=3, f(3)=34%7=6, f(4)=57%7=1, f(5)=86%7=2, f(6)=121%7=2
    assert_eq!(result, Value::Vector(vec![1.0, 6.0, 3.0, 6.0, 1.0, 2.0, 2.0]));
}

// ─── RS proximity testing (hamming distance) ───

#[test]
fn hamming_distance_identical() {
    let program = program(vec![
        const_bits(1.0), const_bits(2.0), const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3: a
        const_bits(1.0), const_bits(2.0), const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(4), EmirValue(5), EmirValue(6)]), // 7: b (same)
        EmirOp::HammingDistance(EmirValue(3), EmirValue(7)), // 8
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::I64(0));
}

#[test]
fn hamming_distance_one_diff() {
    let program = program(vec![
        const_bits(1.0), const_bits(2.0), const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3: a
        const_bits(1.0), const_bits(9.0), const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(4), EmirValue(5), EmirValue(6)]), // 7: b (one diff)
        EmirOp::HammingDistance(EmirValue(3), EmirValue(7)), // 8
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::I64(1));
}

#[test]
fn hamming_distance_all_diff() {
    let program = program(vec![
        const_bits(1.0), const_bits(2.0), const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3: a
        const_bits(4.0), const_bits(5.0), const_bits(6.0),
        EmirOp::VectorCreate(vec![EmirValue(4), EmirValue(5), EmirValue(6)]), // 7: b (all diff)
        EmirOp::HammingDistance(EmirValue(3), EmirValue(7)), // 8
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::I64(3));
}

#[test]
fn rs_proximity_basic() {
    // RS(7,3) over GF(7): f(x) = 1 + 2x + 3x^2
    // codeword = [1, 6, 3, 6, 1, 2, 2]
    // noisy word: flip position 1 (6→0) and position 3 (6→4)
    // noisy = [1, 0, 3, 4, 1, 2, 2]
    // hamming_distance(codeword, noisy) = 2
    let program = program(vec![
        const_bits(1.0), const_bits(2.0), const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3: coeffs
        const_bits(7.0), // 4: n
        const_bits(7.0), // 5: p
        EmirOp::RSEncode(EmirValue(3), EmirValue(4), EmirValue(5)), // 6: codeword
        const_bits(1.0), const_bits(0.0), const_bits(3.0),
        const_bits(4.0), const_bits(1.0), const_bits(2.0), const_bits(2.0),
        EmirOp::VectorCreate(vec![EmirValue(7), EmirValue(8), EmirValue(9),
                                   EmirValue(10), EmirValue(11), EmirValue(12), EmirValue(13)]), // 14: noisy
        EmirOp::HammingDistance(EmirValue(6), EmirValue(14)), // 15: distance
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    assert_eq!(result, Value::I64(2));
}

#[test]
fn rs_proximity_singleton_bound() {
    // RS(7,3) over GF(7): minimum distance = n - k + 1 = 5
    // Two distinct degree-2 polynomials over GF(7) agree on at most 2 points,
    // so their codewords differ on at least 5 positions.
    // f1(x) = 1 + 2x + 3x^2, f2(x) = 2 + 3x + x^2
    let program = program(vec![
        // f1
        const_bits(1.0), const_bits(2.0), const_bits(3.0),
        EmirOp::VectorCreate(vec![EmirValue(0), EmirValue(1), EmirValue(2)]), // 3
        // f2
        const_bits(2.0), const_bits(3.0), const_bits(1.0),
        EmirOp::VectorCreate(vec![EmirValue(4), EmirValue(5), EmirValue(6)]), // 7
        const_bits(7.0), // 8: n
        const_bits(7.0), // 9: p
        EmirOp::RSEncode(EmirValue(3), EmirValue(8), EmirValue(9)), // 10: cw1
        EmirOp::RSEncode(EmirValue(7), EmirValue(8), EmirValue(9)), // 11: cw2
        EmirOp::HammingDistance(EmirValue(10), EmirValue(11)), // 12: dist
    ]);
    let result = evaluate(&program, &[], &[]).unwrap();
    // Minimum distance should be >= 5 (Singleton bound: n - k + 1 = 5)
    let dist = match result {
        Value::I64(d) => d,
        _ => panic!("expected I64"),
    };
    assert!(dist >= 5, "RS minimum distance {} < 5 (Singleton bound violated)", dist);
}
