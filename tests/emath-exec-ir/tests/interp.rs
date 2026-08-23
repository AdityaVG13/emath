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
