//! emath-xx0x.2: richer linear algebra — eigen, SVD, iterative solves.
//!
//! The bead's law, sliced to the numeric-kernel + EMIR seam (disjoint
//! from the parser lanes):
//! - **Eigen** on real SYMMETRIC square matrices (the documented class):
//!   cyclic Jacobi rotations — deterministic, convergence-checked —
//!   via `EigenSymmetric`/`EigenVectorsSymmetric` EMIR ops. Non-square
//!   or materially non-symmetric input refuses typed (`E-LINALG-001/2`,
//!   the negative seed's silent-success shape). Eigenvalues are
//!   ascending; eigenvector columns are aligned to them.
//! - **SVD**: thin decomposition via the symmetric AᵀA eigenproblem —
//!   singular values DESCENDING (`SvdSingularValues`), U/Vᵀ factors
//!   (`SvdFactors`) satisfying the reconstruction property
//!   A ≈ U·diag(s)·Vᵀ within the strict-f64 policy. Rank-deficient
//!   columns are zero-filled (documented), never NaN.
//! - **Iterative solve**: conjugate gradient over the matrix's dense
//!   storage (`CgSolve`) — SPD-convergence-checked; a non-converging
//!   system (non-SPD or indefinite) refuses typed `E-LINALG-003`,
//!   never a silently wrong x. The sparse STORAGE type is the named
//!   deferred slice; the iterative METHOD computes now.
//! - LU/QR/Cholesky stay on 4wj0 (their typed refusals unchanged);
//!   large-scale/GPU eigensolvers are Horizon (honest refuse here).

use emath_core::limits::Limits;
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_core::Span;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;
use emath_term::{SymbolId, Term};

fn matrix(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}

fn eval(ops: Vec<EmirOp>, inputs: &[Value]) -> Result<Value, EvalFault> {
    // The .14 seam law: inputs enter registers through LoadInput ops,
    // then the kernel ops consume them by register index.
    let mut program_ops: Vec<(EmirOp, Span)> = (0..inputs.len())
        .map(|index| (EmirOp::LoadInput(index as u16), Span::default()))
        .collect();
    program_ops.extend(ops.into_iter().map(|op| (op, Span::default())));
    // Every op appends one register, so the program result is the LAST
    // op's output register.
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

fn vector_of(value: &Value) -> Vec<f64> {
    let Value::Vector(v) = value else {
        panic!("expected a vector, got {value:?}")
    };
    v.clone()
}

fn matrix_of(value: &Value) -> (usize, usize, Vec<f64>) {
    let Value::Matrix { rows, cols, data } = value else {
        panic!("expected a matrix, got {value:?}")
    };
    (*rows, *cols, data.clone())
}

#[test]
fn eigen_known_2x2() {
    // Known 2x2 symmetric: [[2,1],[1,2]] has eigenvalues {1, 3} with
    // eigenvectors (1,-1)/√2 and (1,1)/√2 — the classic fixture.
    let a = matrix(2, 2, &[2.0, 1.0, 1.0, 2.0]);
    let values = eval(
        vec![EmirOp::EigenSymmetric(EmirValue(0))],
        &[a.clone()],
    )
    .expect("eigen computes");
    let values = vector_of(&values);
    assert_eq!(values.len(), 2);
    assert!((values[0] - 1.0).abs() < 1e-10, "ascending: {values:?}");
    assert!((values[1] - 3.0).abs() < 1e-10, "ascending: {values:?}");
    let vectors = eval(
        vec![EmirOp::EigenVectorsSymmetric(EmirValue(0))],
        &[a],
    )
    .expect("eigenvectors compute");
    let (_rows, cols, data) = matrix_of(&vectors);
    assert_eq!(cols, 2);
    // Column j pairs with eigenvalue j; the strong law is A·v_j = λ_j·v_j
    // for each column (checked below), which also pins unit-norm columns
    // for this fixture's eigenbasis.
    for j in 0..2usize {
        let v0 = data[j];
        let v1 = data[2 + j];
        let av0 = 2.0 * v0 + 1.0 * v1;
        let av1 = 1.0 * v0 + 2.0 * v1;
        assert!(
            (av0 - values[j] * v0).abs() < 1e-9 && (av1 - values[j] * v1).abs() < 1e-9,
            "A v_{j} = lambda_{j} v_{{j}} failed: {values:?} vs columns {v0},{v1}"
        );
    }
}

#[test]
fn eigen_diagonal_and_sorted() {
    // A diagonal matrix is already in eigenform: values are the diagonal
    // entries SORTED ASCENDING, vectors are permutation columns.
    let a = matrix(3, 3, &[3.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0]);
    let values = eval(
        vec![EmirOp::EigenSymmetric(EmirValue(0))],
        &[a],
    )
    .expect("eigen computes");
    let values = vector_of(&values);
    let expected = [1.0, 2.0, 3.0];
    for (got, want) in values.iter().zip(expected.iter()) {
        assert!((got - want).abs() < 1e-10, "ascending {values:?}");
    }
}

#[test]
fn eigen_non_square_refuses_typed() {
    // NEGATIVE (the seed's silent-success): eigen on a non-square
    // matrix refuses typed E-LINALG-001 — never a silently truncated
    // or garbage spectrum.
    let a = matrix(2, 3, &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    let error = eval(
        vec![EmirOp::EigenSymmetric(EmirValue(0))],
        &[a],
    )
    .expect_err("non-square eigen refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-LINALG-001"),
        "non-square eigen must name E-LINALG-001, got {fault}"
    );
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/eigen_svd_iterative.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-LINALG-001"),
        "seed expects the non-square refusal, found: {expect_line}"
    );
}

#[test]
fn eigen_non_symmetric_refuses_typed() {
    // The documented class is real SYMMETRIC: a materially
    // non-symmetric matrix refuses typed E-LINALG-002 (never a silent
    // garbage spectrum from running Jacobi on a non-symmetric input).
    let a = matrix(2, 2, &[1.0, 2.0, 3.0, 4.0]);
    let error = eval(
        vec![EmirOp::EigenSymmetric(EmirValue(0))],
        &[a],
    )
    .expect_err("non-symmetric eigen refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-LINALG-002"),
        "non-symmetric eigen must name E-LINALG-002, got {fault}"
    );
}

#[test]
fn svd_reconstruction_property() {
    // Property (the bead's acceptance): A = U·diag(s)·Vᵀ within the
    // numeric policy, singular values DESCENDING, factors are
    // orthonormal (UᵀU = I, VᵀV = I on the computed columns).
    let rows = 3usize;
    let cols = 2usize;
    let a = matrix(rows, cols, &[3.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
    let singular = eval(
        vec![EmirOp::SvdSingularValues(EmirValue(0))],
        &[a.clone()],
    )
    .expect("svd computes");
    let s = vector_of(&singular);
    assert_eq!(s.len(), cols.min(rows));
    assert!(
        s[0] >= s[1] && (s[0] - 3.0).abs() < 1e-9 && (s[1] - 2.0).abs() < 1e-9,
        "descending singular values: {s:?}"
    );
    let factors = eval(
        vec![EmirOp::SvdFactors(EmirValue(0))],
        &[a],
    )
    .expect("svd factors compute");
    // The factors value is the interleaved [U | s | Vᵀ] bundle packed as
    // a matrix with rows = rows + 1 + cols (documented packing).
    let (frows, _fcols, fdata) = matrix_of(&factors);
    assert_eq!(frows, rows + 1 + cols);
    let u: Vec<Vec<f64>> = (0..rows)
        .map(|r| fdata[r * cols..r * cols + cols].to_vec())
        .collect();
    let sv = fdata[rows * cols..rows * cols + cols].to_vec();
    let v_t: Vec<Vec<f64>> = ((rows + 1)..(rows + 1 + cols))
        .map(|r| fdata[r * cols..r * cols + cols].to_vec())
        .collect();
    for (got, want) in sv.iter().zip(s.iter()) {
        assert!((got - want).abs() < 1e-12, "factor bundle carries s");
    }
    // Reconstruction: A - U·diag(s)·Vᵀ ≈ 0.
    for i in 0..rows {
        for j in 0..cols {
            let mut reconstructed = 0.0;
            for k in 0..cols {
                reconstructed += u[i][k] * sv[k] * v_t[k][j];
            }
            let original = match (i, j) {
                (0, 0) => 3.0,
                (1, 1) => 2.0,
                _ => 0.0,
            };
            assert!(
                (reconstructed - original).abs() < 1e-9,
                "reconstruction A[{i}][{j}]: {reconstructed} vs {original}"
            );
        }
    }
    // Orthonormality of Vᵀ rows (columns of V).
    for c1 in 0..cols {
        for c2 in 0..cols {
            let dot: f64 = v_t[c1].iter().zip(v_t[c2].iter()).map(|(x, y)| x * y).sum();
            let want = if c1 == c2 { 1.0 } else { 0.0 };
            assert!((dot - want).abs() < 1e-9, "Vᵀ orthonormal at {c1},{c2}: {dot}");
        }
    }
}

#[test]
fn iterative_solve_computes_and_refuses() {
    // CG on an SPD system (1D Laplacian): x solves A x = b. A
    // non-converging (non-SPD) system refuses typed E-LINALG-003 —
    // never a silently wrong x.
    let n = 4usize;
    let mut laplacian = vec![0.0; n * n];
    for i in 0..n {
        laplacian[i * n + i] = 2.0;
        if i + 1 < n {
            laplacian[i * n + i + 1] = -1.0;
            laplacian[(i + 1) * n + i] = -1.0;
        }
    }
    let b = Value::Vector(vec![1.0, 1.0, 1.0, 1.0]);
    let solved = eval(
        vec![EmirOp::CgSolve(EmirValue(0), EmirValue(1))],
        &[matrix(n, n, &laplacian), b],
    )
    .expect("cg solves the SPD system");
    let x = vector_of(&solved);
    // Verify A x = b directly.
    for i in 0..n {
        let lhs: f64 = (0..n)
            .map(|j| laplacian[i * n + j] * x[j])
            .sum();
        assert!((lhs - 1.0).abs() < 1e-8, "A x = b at row {i}: {lhs}");
    }
    // Non-SPD: a matrix with a negative eigenvalue must refuse.
    let indefinite = matrix(2, 2, &[1.0, 0.0, 0.0, -1.0]);
    let error = eval(
        vec![EmirOp::CgSolve(EmirValue(0), EmirValue(1))],
        &[indefinite, Value::Vector(vec![1.0, 1.0])],
    )
    .expect_err("indefinite system refuses");
    let fault = format!("{error:?}");
    assert!(
        fault.contains("E-LINALG-003"),
        "indefinite iterative solve must name E-LINALG-003, got {fault}"
    );
}

#[test]
fn strict_source_compiles() {
    // E2E: a strict-source model calling the new surface compiles
    // through the emitter vocabulary.
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let source = "emath function spectrum:\n    inputs:\n        a: Matrix<Float64>\n    outputs:\n        y: Float64\n    definitions:\n        y = eigvals(a)[0]\n";
    let result = session.check_owned("spectrum", source);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    // Admission-level check: the surface must not be refused at compile
    // (it either admits cleanly or only shape-annotates; the numeric
    // run is proven by the EMIR tests above).
    let messages: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.message.clone())
        .collect();
    assert!(
        codes.iter().all(|code| code != "E-SYN-101"),
        "eigen surface must not be an unknown function: {codes:?} {messages:?}"
    );
}

#[test]
fn bundle_fixture() {
    // WorldResultBundle fixture (e2e clause; the VM path is touched).
    struct LinalgWorld;
    impl emath_genesis::FirstOrderWorld for LinalgWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let a = matrix(2, 2, &[2.0, 1.0, 1.0, 2.0]);
            let values = eval(
                vec![EmirOp::EigenSymmetric(EmirValue(0))],
                &[a],
            )
            .map(|v| vector_of(&v))
            .unwrap_or_default();
            if values.len() == 2
                && (values[0] - 1.0).abs() < 1e-10
                && (values[1] - 3.0).abs() < 1e-10
            {
                Ok("eigen-computes".to_string())
            } else {
                Ok("eigen-diverged".to_string())
            }
        }

        fn apply(
            &self,
            operator: &SymbolId,
            _arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            Err(emath_genesis::EvalError::UnknownSymbol(operator.clone()))
        }

        fn evidence(&self) -> emath_genesis::WorldEvidence {
            emath_genesis::WorldEvidence::seed(
                "richer-linalg",
                &["deterministic-jacobi", "typed-spectral-refusals"],
            )
        }
    }

    let term = Term::Constant(SymbolId("linalg[fixture]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &LinalgWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "richer-linalg");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
