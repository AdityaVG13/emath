#![forbid(unsafe_code)]

use std::path::Path;

use emath_exec_ir::interp::{evaluate_with_budget, EvalFault, Value};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::install_language_distribution;
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_test_harness::Probe;

fn seam_eval(capability: &str, inputs: &[Value]) -> Result<Value, EvalFault> {
    let count = inputs.len();
    let mut ops: Vec<_> = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Default::default()))
        .collect();
    ops.push((
        EmirOp::ApplyCapability {
            capability: capability.to_string(),
            class: CellClass::Pure,
            args: (0..count as u32).map(EmirValue).collect(),
        },
        Default::default(),
    ));
    evaluate_with_budget(
        &EmirProgram {
            ops,
            result: EmirValue(count as u32),
            input_count: count as u16,
            state_count: 0,
            domain_obligations: Vec::new(),
        },
        inputs,
        &[],
        EvalBudget::default(),
    )
}

fn refused(result: &Result<Value, EvalFault>, code: &str) -> bool {
    match result {
        Err(fault) => format!("{fault:?}").contains(code),
        Ok(_) => false,
    }
}

fn matrix(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}

fn close_enough(actual: &[f64], expected: &[f64], tolerance: f64) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected.iter())
            .all(|(a, e)| (a - e).abs() <= tolerance)
}

#[test]
fn probe() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = load_language_distribution(&root).expect("language distribution");
    install_language_distribution(&distribution).expect("active kernels bind on this eval thread");
    let mut probe = Probe::new("linear_capsule_cutover.rs: every check in one probe");
    probe.case("authored_vector_norm_preserves_values", |probe| {
        probe.eq(
            "seam_eval norm([3, 4])",
            &(seam_eval(
                "std.capability.linear.vector-norm",
                &[Value::Vector(vec![3.0, 4.0])]
            )),
            &(Ok(Value::F64(5.0))),
        );
        probe.eq(
            "seam_eval norm([])",
            &(seam_eval("std.capability.linear.vector-norm", &[Value::Vector(vec![])])),
            &(Ok(Value::F64(0.0))),
        );
    });
    probe.case("authored_symmetric_spectrum_preserves_values_and_refusals", |probe| {
        let diagonal = matrix(2, 2, &[2.0, 0.0, 0.0, 5.0]);
        probe.eq(
            "seam_eval eigvals(diagonal)",
            &(seam_eval("std.capability.linear.symmetric-eigenvalues", &[diagonal.clone()])),
            &(Ok(Value::Vector(vec![2.0, 5.0]))),
        );
        let refusal = seam_eval(
            "std.capability.linear.symmetric-eigenvalues",
            &[matrix(2, 3, &[1.0; 6])],
        );
        probe.demand("nonsquare refuses E-LINALG-001", refused(&refusal, "E-LINALG-001"), format!("{refusal:?}"));
        let asymmetric = seam_eval(
            "std.capability.linear.symmetric-eigenvalues",
            &[matrix(2, 2, &[1.0, 2.0, 0.0, 1.0])],
        );
        probe.demand("nonsymmetric refuses E-LINALG-002", refused(&asymmetric, "E-LINALG-002"), format!("{asymmetric:?}"));
        probe.eq(
            "seam_eval eigvecs(diagonal)",
            &(seam_eval("std.capability.linear.symmetric-eigenvectors", &[diagonal])),
            &(Ok(matrix(2, 2, &[1.0, 0.0, 0.0, 1.0]))),
        );
    });
    probe.case("coupled_eigenbasis_satisfies_defining_equation", |probe| {
        let coupled = matrix(2, 2, &[2.0, 1.0, 1.0, 3.0]);
        let values = seam_eval(
            "std.capability.linear.symmetric-eigenvalues",
            &[coupled.clone()],
        )
        .expect("coupled spectrum must evaluate");
        let basis = seam_eval(
            "std.capability.linear.symmetric-eigenvectors",
            &[coupled.clone()],
        )
        .expect("coupled basis must evaluate");
        let (Value::Vector(lambdas), Value::Matrix { rows, cols, data }) = (values, basis)
        else {
            panic!("coupled spectrum must return a vector and a matrix")
        };
        probe.demand("coupled basis is 2x2", rows == 2 && cols == 2, format!("{rows}x{cols}"));
        probe.demand("coupled spectrum has order 2", lambdas.len() == 2, format!("{lambdas:?}"));
        for (column_index, lambda) in lambdas.iter().enumerate() {
            let column = vec![data[column_index], data[2 + column_index]];
            let applied = vec![
                2.0 * column[0] + 1.0 * column[1],
                1.0 * column[0] + 3.0 * column[1],
            ];
            let scaled = vec![lambda * column[0], lambda * column[1]];
            probe.demand(
                "coupled column satisfies A*v=lambda*v",
                close_enough(&applied, &scaled, 1e-9),
                format!("column {column_index}: A*v={applied:?} lambda*v={scaled:?}"),
            );
        }
    });
    probe.case("authored_rectangular_spectrum_preserves_values_and_refusals", |probe| {
        probe.eq(
            "seam_eval singular_values(diagonal)",
            &(seam_eval(
                "std.capability.linear.singular-values",
                &[matrix(2, 2, &[2.0, 0.0, 0.0, 5.0])]
            )),
            &(Ok(Value::Vector(vec![5.0, 2.0]))),
        );
        let empty = seam_eval(
            "std.capability.linear.singular-values",
            &[matrix(0, 0, &[])],
        );
        probe.demand("empty refuses E-LINALG-004", refused(&empty, "E-LINALG-004"), format!("{empty:?}"));
        probe.eq(
            "seam_eval svd_factors(diagonal)",
            &(seam_eval(
                "std.capability.linear.svd-factors",
                &[matrix(2, 2, &[2.0, 0.0, 0.0, 5.0])]
            )),
            &(Ok(matrix(5, 2, &[0.0, 1.0, 1.0, 0.0, 5.0, 2.0, 0.0, 1.0, 1.0, 0.0]))),
        );
    });
    probe.case("authored_iterative_solve_preserves_values_and_refusals", |probe| {
        let solved = seam_eval(
            "std.capability.linear.iterative-solve",
            &[
                matrix(2, 2, &[4.0, 0.0, 0.0, 9.0]),
                Value::Vector(vec![8.0, 18.0]),
            ],
        )
        .expect("diagonal system must solve");
        let Value::Vector(solution) = solved else {
            panic!("solver must return a vector")
        };
        probe.demand(
            "diagonal solve is [2, 2]",
            close_enough(&solution, &[2.0, 2.0], 1e-12),
            format!("{solution:?}"),
        );
        let mismatch = seam_eval(
            "std.capability.linear.iterative-solve",
            &[matrix(2, 2, &[1.0, 0.0, 0.0, 1.0]), Value::Vector(vec![1.0])],
        );
        probe.demand("rhs mismatch refuses E-LINALG-004", refused(&mismatch, "E-LINALG-004"), format!("{mismatch:?}"));
        let nonsquare = seam_eval(
            "std.capability.linear.iterative-solve",
            &[matrix(2, 3, &[1.0; 6]), Value::Vector(vec![1.0, 2.0])],
        );
        probe.demand("nonsquare refuses E-LINALG-001", refused(&nonsquare, "E-LINALG-001"), format!("{nonsquare:?}"));
    });
    probe.case("authored_dense_carrier_laws_preserve_values_and_refusals", |probe| {
        probe.eq(
            "seam_eval vector_add",
            &(seam_eval(
                "std.capability.linear.vector-add",
                &[Value::Vector(vec![1.0, 2.0]), Value::Vector(vec![3.0, 4.0])]
            )),
            &(Ok(Value::Vector(vec![4.0, 6.0]))),
        );
        let mismatch = seam_eval(
            "std.capability.linear.vector-add",
            &[Value::Vector(vec![1.0]), Value::Vector(vec![1.0, 2.0])],
        );
        probe.demand("length mismatch refuses E-SHAPE-001", refused(&mismatch, "E-SHAPE-001"), format!("{mismatch:?}"));
        probe.eq(
            "seam_eval matrix_add",
            &(seam_eval(
                "std.capability.linear.matrix-add",
                &[matrix(2, 2, &[1.0, 2.0, 3.0, 4.0]), matrix(2, 2, &[5.0, 6.0, 7.0, 8.0])]
            )),
            &(Ok(matrix(2, 2, &[6.0, 8.0, 10.0, 12.0]))),
        );
        let shape = seam_eval(
            "std.capability.linear.matrix-add",
            &[matrix(2, 2, &[1.0; 4]), matrix(1, 4, &[1.0; 4])],
        );
        probe.demand("same-numel shape mismatch refuses E-SHAPE-001", refused(&shape, "E-SHAPE-001"), format!("{shape:?}"));
        probe.eq(
            "seam_eval matrix_product",
            &(seam_eval(
                "std.capability.linear.matrix-product",
                &[matrix(2, 2, &[1.0, 2.0, 3.0, 4.0]), matrix(2, 2, &[5.0, 6.0, 7.0, 8.0])]
            )),
            &(Ok(matrix(2, 2, &[19.0, 22.0, 43.0, 50.0]))),
        );
        let inner = seam_eval(
            "std.capability.linear.matrix-product",
            &[matrix(2, 3, &[1.0; 6]), matrix(2, 2, &[1.0; 4])],
        );
        probe.demand("inner mismatch refuses E-SHAPE-001", refused(&inner, "E-SHAPE-001"), format!("{inner:?}"));
        probe.eq(
            "seam_eval matrix_vector_product",
            &(seam_eval(
                "std.capability.linear.matrix-vector-product",
                &[matrix(2, 2, &[1.0, 2.0, 3.0, 4.0]), Value::Vector(vec![1.0, 1.0])]
            )),
            &(Ok(Value::Vector(vec![3.0, 7.0]))),
        );
        let width = seam_eval(
            "std.capability.linear.matrix-vector-product",
            &[matrix(2, 2, &[1.0; 4]), Value::Vector(vec![1.0])],
        );
        probe.demand("width mismatch refuses E-SHAPE-001", refused(&width, "E-SHAPE-001"), format!("{width:?}"));
        probe.eq(
            "seam_eval transpose",
            &(seam_eval(
                "std.capability.linear.transpose",
                &[matrix(2, 2, &[1.0, 2.0, 3.0, 4.0])]
            )),
            &(Ok(matrix(2, 2, &[1.0, 3.0, 2.0, 4.0]))),
        );
        let involution = seam_eval(
            "std.capability.linear.transpose",
            &[matrix(2, 2, &[1.0, 3.0, 2.0, 4.0])],
        )
        .expect("transpose must evaluate");
        let roundtrip = seam_eval("std.capability.linear.transpose", &[involution])
            .expect("transpose must evaluate");
        probe.eq("transpose involution", &roundtrip, &matrix(2, 2, &[1.0, 3.0, 2.0, 4.0]));
        probe.eq(
            "seam_eval tensor_add",
            &(seam_eval(
                "std.capability.tensor.add",
                &[
                    Value::Tensor { shape: vec![2], data: vec![1.0, 2.0] },
                    Value::Tensor { shape: vec![2], data: vec![3.0, 4.0] },
                ]
            )),
            &(Ok(Value::Tensor { shape: vec![2], data: vec![4.0, 6.0] })),
        );
        let tshape = seam_eval(
            "std.capability.tensor.add",
            &[
                Value::Tensor { shape: vec![2], data: vec![1.0, 2.0] },
                Value::Tensor { shape: vec![3], data: vec![1.0, 2.0, 3.0] },
            ],
        );
        probe.demand("tensor shape mismatch refuses E-SHAPE-001", refused(&tshape, "E-SHAPE-001"), format!("{tshape:?}"));
    });
    probe.case("authored_polynomial_surface_preserves_values_and_refusals", |probe| {
        probe.eq(
            "seam_eval poly_mul",
            &(seam_eval(
                "std.capability.poly.mul",
                &[Value::Vector(vec![1.0, 1.0]), Value::Vector(vec![1.0, 1.0])]
            )),
            &(Ok(Value::Vector(vec![1.0, 2.0, 1.0]))),
        );
        probe.eq(
            "seam_eval poly_mul empty",
            &(seam_eval(
                "std.capability.poly.mul",
                &[Value::Vector(vec![]), Value::Vector(vec![1.0])]
            )),
            &(Ok(Value::Vector(vec![]))),
        );
        let nonfinite = seam_eval(
            "std.capability.poly.mul",
            &[Value::Vector(vec![f64::NAN]), Value::Vector(vec![1.0])],
        );
        probe.demand("non-finite coefficient refuses E-POLY-001", refused(&nonfinite, "E-POLY-001"), format!("{nonfinite:?}"));
        probe.eq(
            "seam_eval poly_eval",
            &(seam_eval(
                "std.capability.poly.eval",
                &[Value::Vector(vec![1.0, 2.0, 3.0]), Value::F64(2.0)]
            )),
            &(Ok(Value::F64(17.0))),
        );
        probe.eq(
            "seam_eval poly_eval empty",
            &(seam_eval(
                "std.capability.poly.eval",
                &[Value::Vector(vec![]), Value::F64(2.0)]
            )),
            &(Ok(Value::F64(0.0))),
        );
        let point = seam_eval(
            "std.capability.poly.eval",
            &[Value::Vector(vec![1.0]), Value::F64(f64::INFINITY)],
        );
        probe.demand("non-finite point refuses E-POLY-002", refused(&point, "E-POLY-002"), format!("{point:?}"));
    });
    probe.case("authored_scalar_and_fold_surface_preserves_values_and_refusals", |probe| {
        probe.eq(
            "seam_eval lerp",
            &(seam_eval(
                "std.capability.linear.lerp",
                &[Value::F64(0.0), Value::F64(10.0), Value::F64(0.5)]
            )),
            &(Ok(Value::F64(5.0))),
        );
        probe.eq(
            "seam_eval clamp high",
            &(seam_eval(
                "std.capability.linear.clamp",
                &[Value::F64(11.0), Value::F64(0.0), Value::F64(10.0)]
            )),
            &(Ok(Value::F64(10.0))),
        );
        probe.eq(
            "seam_eval clamp low",
            &(seam_eval(
                "std.capability.linear.clamp",
                &[Value::F64(-1.0), Value::F64(0.0), Value::F64(10.0)]
            )),
            &(Ok(Value::F64(0.0))),
        );
        probe.eq(
            "seam_eval sum",
            &(seam_eval(
                "std.capability.reduction.finite",
                &[Value::Vector(vec![1.0, 2.0, 3.0])]
            )),
            &(Ok(Value::F64(6.0))),
        );
        probe.eq(
            "seam_eval sum matrix",
            &(seam_eval(
                "std.capability.reduction.finite",
                &[matrix(2, 2, &[1.0, 2.0, 3.0, 4.0])]
            )),
            &(Ok(Value::F64(10.0))),
        );
        let empty_sum = seam_eval(
            "std.capability.reduction.finite",
            &[Value::Vector(vec![])],
        );
        probe.demand("empty sum refuses E-TYPE-012", refused(&empty_sum, "E-TYPE-012"), format!("{empty_sum:?}"));
        probe.eq(
            "seam_eval product",
            &(seam_eval(
                "std.capability.reduction.product",
                &[Value::Vector(vec![2.0, 3.0, 4.0])]
            )),
            &(Ok(Value::F64(24.0))),
        );
        let solved = seam_eval(
            "std.capability.linear.normalize",
            &[Value::Vector(vec![3.0, 4.0])],
        )
        .expect("normalization must evaluate");
        let Value::Vector(unit) = solved else {
            panic!("normalize must return a vector")
        };
        probe.demand("unit vector is [0.6, 0.8]", close_enough(&unit, &[0.6, 0.8], 1e-12), format!("{unit:?}"));
        let empty_unit = seam_eval(
            "std.capability.linear.normalize",
            &[Value::Vector(vec![])],
        );
        probe.demand("empty normalize refuses E-TYPE-012", refused(&empty_unit, "E-TYPE-012"), format!("{empty_unit:?}"));
    });
    probe.case("checked_dense_index_stays_primitive_through_the_seam", |probe| {
        probe.eq(
            "seam_eval index vector",
            &(seam_eval(
                "std.capability.tensor.index",
                &[Value::Vector(vec![3.0, 4.0]), Value::Set(vec![Value::I64(1)])]
            )),
            &(Ok(Value::F64(4.0))),
        );
        probe.eq(
            "seam_eval index matrix",
            &(seam_eval(
                "std.capability.tensor.index",
                &[
                    matrix(2, 2, &[1.0, 2.0, 3.0, 4.0]),
                    Value::Set(vec![Value::I64(1), Value::I64(1)]),
                ]
            )),
            &(Ok(Value::F64(4.0))),
        );
        let oob = seam_eval(
            "std.capability.tensor.index",
            &[Value::Vector(vec![3.0, 4.0]), Value::Set(vec![Value::I64(2)])],
        );
        probe.demand("out-of-bounds refuses E-SHAPE-006", refused(&oob, "E-SHAPE-006"), format!("{oob:?}"));
        let rank = seam_eval(
            "std.capability.tensor.index",
            &[Value::Vector(vec![3.0, 4.0]), Value::Set(vec![Value::I64(0), Value::I64(0)])],
        );
        probe.demand("rank mismatch refuses E-SHAPE-006", refused(&rank, "E-SHAPE-006"), format!("{rank:?}"));
    });
    probe.finish();
}
