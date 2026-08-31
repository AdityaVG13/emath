//! Tests for Vector and Matrix types, literals, indexing, and arithmetic in semantic analysis.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_sema::CompilerSession;
use emath_sema::admit::CheckResult;
use emath_syntax::install_source_parser;

fn check_source(name: &str, source: &str) -> CheckResult {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

#[test]
fn vector_literals_and_indexing_admit() {
    let source = "\
emath function VectorOps:
    inputs:
        x: Float64
    outputs:
        v: Vector[3]
        first: Float64
    definitions:
        v = [x, 2.0 * x, 3.0]
        first = v[0]
";
    let result = check_source("vec-test", source);
    assert!(
        !result.diagnostics.has_errors(),
        "vector operations must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(result.package.declarations.len(), 1);
    let decl = &result.package.declarations[0];
    assert_eq!(decl.outputs.len(), 2);
}

#[test]
fn matrix_literals_and_indexing_admit() {
    let source = "\
emath function MatrixOps:
    inputs:
        a: Float64
        b: Float64
    outputs:
        m: Matrix[2, 2]
        elem: Float64
    definitions:
        m = [[a, b], [0.0, 1.0]]
        elem = m[0, 1]
";
    let result = check_source("mat-test", source);
    assert!(
        !result.diagnostics.has_errors(),
        "matrix operations must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn linear_algebra_arithmetic_admits() {
    let source = "\
emath function LinearAlgebra:
    inputs:
        v1: Vector[3]
        v2: Vector[3]
        m1: Matrix[3, 3]
        s: Float64
    outputs:
        v_sum: Vector[3]
        v_diff: Vector[3]
        v_scaled: Vector[3]
        d: Float64
        n: Float64
        mv: Vector[3]
        m_trans: Matrix[3, 3]
        m_sq: Matrix[3, 3]
    definitions:
        v_sum = v1 + v2
        v_diff = v1 - v2
        v_scaled = s * v1
        d = dot(v1, v2)
        n = norm(v1)
        mv = m1 * v1
        m_trans = transpose(m1)
        m_sq = m1 * m1
";
    let result = check_source("la-test", source);
    assert!(
        !result.diagnostics.has_errors(),
        "linear algebra operations must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn ragged_matrix_literal_is_rejected() {
    let source = "\
emath function BadMatrix:
    inputs:
        x: Float64
    outputs:
        m: Matrix[2, 2]
    definitions:
        m = [[1.0, 2.0], [3.0]]
";
    let result = check_source("ragged-mat", source);
    assert!(
        result.diagnostics.has_errors(),
        "ragged matrix must be rejected"
    );
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-SHAPE-005"),
        "expected E-SHAPE-005 for ragged matrix, got: {codes:?}"
    );
}

#[test]
fn vector_dimension_mismatch_is_rejected() {
    let source = "\
emath function DimMismatch:
    inputs:
        v1: Vector[2]
        v2: Vector[3]
    outputs:
        v3: Vector[2]
    definitions:
        v3 = v1 + v2
";
    let result = check_source("dim-mismatch", source);
    assert!(
        result.diagnostics.has_errors(),
        "dimension mismatch in vector add must be rejected"
    );
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-SHAPE-005"),
        "expected E-SHAPE-005, got: {codes:?}"
    );
}

#[test]
fn matrix_mul_dimension_mismatch_is_rejected() {
    let source = "\
emath function MatDimMismatch:
    inputs:
        m1: Matrix[2, 3]
        v: Vector[2]
    outputs:
        res: Vector[2]
    definitions:
        res = m1 * v
";
    let result = check_source("mat-dim-mismatch", source);
    assert!(
        result.diagnostics.has_errors(),
        "matrix-vector inner dimension mismatch must be rejected"
    );
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-SHAPE-002"),
        "expected E-SHAPE-002, got: {codes:?}"
    );
}

#[test]
fn vector_index_rank_mismatch_is_rejected() {
    let source = "\
emath function BadIndex:
    inputs:
        v: Vector[3]
    outputs:
        x: Float64
    definitions:
        x = v[0, 1]
";
    let result = check_source("bad-index", source);
    assert!(
        result.diagnostics.has_errors(),
        "vector [i, j] must be rejected"
    );
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-SHAPE-006"),
        "expected E-SHAPE-006, got: {codes:?}"
    );
}

#[test]
fn rank3_tensor_literal_and_slice_admit() {
    let source = "\
emath function TensorSlice:
    inputs:
        n: Float64
    outputs:
        t: Tensor[2, 2, 2]
        face: Matrix[2, 2]
    definitions:
        t = [[[1.0, 2.0], [3.0, 4.0]], [[5.0, 6.0], [7.0, 8.0]]]
        face = t[0, :, :]
";
    let result = check_source("tensor-slice", source);
    assert!(
        !result.diagnostics.has_errors(),
        "rank-3 tensor + slice must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn rank3_spatial_operators_and_divergence_admit() {
    let source = include_str!("../../../tests/fixtures/language/numerical/spatial-3d.emath");
    let result = check_source("spatial-3d", source);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result
            .diagnostics
            .errors()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}

#[test]
fn rank3_spatial_operator_refuses_matrix_input() {
    let source = "\
emath function BadSpatial3d:
    inputs:
        u: Matrix[3, 3]
    outputs:
        lap: Matrix[3, 3]
    definitions:
        lap = laplacian_3d(u, 1.0)
";
    let result = check_source("bad-spatial-3d", source);
    assert!(result.diagnostics.errors().any(|diagnostic| {
        diagnostic.code == "E-TYPE-012" && diagnostic.message.contains("rank-3 Tensor")
    }));
}

#[test]
fn tensor_face_example_evaluates() {
    let source = include_str!("../../../tests/fixtures/language/intro/tensor-face.emath");
    let result = check_source("tensor-face", source);
    assert!(
        !result.diagnostics.has_errors(),
        "tensor-face.emath must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(
        test.outputs.get("face"),
        Some(&Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![1.0, 2.0, 3.0, 4.0],
        })
    );
    assert!(
        test.verdict.expect_passed(),
        "t[0, :, :] must be the first face, got {}",
        test.verdict
    );
}

#[test]
fn einsum_example_evaluates() {
    let source = include_str!("../../../tests/fixtures/language/intro/einsum.emath");
    let result = check_source("einsum-example", source);
    assert!(
        !result.diagnostics.has_errors(),
        "einsum.emath must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(
        test.outputs.get("ein"),
        Some(&Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![19.0, 22.0, 43.0, 50.0],
        })
    );
    assert_eq!(test.outputs.get("ab"), test.outputs.get("ein"));
    assert_eq!(test.outputs.get("ein"), test.outputs.get("implicit"));
    assert_eq!(test.outputs.get("ddot"), test.outputs.get("ein_dot"));
    assert!(
        test.verdict.expect_passed(),
        "einsum.emath must pin [[19,22],[43,50]] and dot 32, got {}",
        test.verdict
    );
}

/// einsum vs matmul / dot / transpose involution, including implicit
/// `"ik,kj"` (alphabetical free indices, not HashSet order).
#[test]
fn einsum_contraction_identities_evaluate() {
    let source = "\
emath function EinsumIds:
    inputs:
        n: Float64
    outputs:
        ab: Matrix[2, 2]
        ein: Matrix[2, 2]
        implicit: Matrix[2, 2]
        ddot: Float64
        ein_dot: Float64
        tt: Matrix[2, 3]
        m: Matrix[2, 3]
    definitions:
        a = [[1.0, 2.0], [3.0, 4.0]]
        b = [[5.0, 6.0], [7.0, 8.0]]
        ab = a * b
        ein = einsum(\"ik,kj->ij\", a, b)
        implicit = einsum(\"ik,kj\", a, b)
        u = [n, 2.0, 3.0]
        v = [4.0, 5.0, 6.0]
        ddot = dot(u, v)
        ein_dot = einsum(\"i,i->\", u, v)
        m = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
        tt = transpose(transpose(m))
    tests:
        example <ids>:
            given n = 1.0
            expect ab == ein and ein == implicit and ddot == ein_dot and tt == m
";
    let result = check_source("einsum-ids", source);
    assert!(
        !result.diagnostics.has_errors(),
        "einsum identities must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(
        test.outputs.get("ab"),
        Some(&Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![19.0, 22.0, 43.0, 50.0],
        })
    );
    assert_eq!(test.outputs.get("ab"), test.outputs.get("ein"));
    assert_eq!(test.outputs.get("ein"), test.outputs.get("implicit"));
    assert_eq!(test.outputs.get("ddot"), Some(&Value::F64(32.0)));
    assert_eq!(test.outputs.get("ein_dot"), Some(&Value::F64(32.0)));
    assert_eq!(test.outputs.get("tt"), test.outputs.get("m"));
    assert!(
        test.verdict.expect_passed(),
        "einsum identities must hold, got {}",
        test.verdict
    );
}

#[test]
fn vector3_plus_vector1_is_refused() {
    let source = "\
emath function Broadcast:
    inputs:
        v3: Vector[3]
        v1: Vector[1]
    outputs:
        out: Vector[3]
    definitions:
        out = v3 + v1
";
    let result = check_source("vec-broadcast", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-SHAPE-005"),
        "Vector[3]+Vector[1] must be E-SHAPE-005, got {codes:?}"
    );
}

#[test]
fn nat_index_admits() {
    let source = "\
emath function NatIndex:
    inputs:
        v: Vector[3]
        i: Nat
    outputs:
        x: Float64
    definitions:
        x = v[i]
";
    let result = check_source("nat-index", source);
    assert!(
        !result.diagnostics.has_errors(),
        "Nat index must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn finite_sum_one_to_five_admits() {
    let source = include_str!("../../../tests/fixtures/language/intro/sum-one-to-five.emath");
    let result = check_source("sum-one-to-five", source);
    assert!(
        !result.diagnostics.has_errors(),
        "finite sum must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("total"), Some(&Value::F64(15.0)));
    assert_eq!(test.outputs.get("folded"), Some(&Value::F64(15.0)));
    assert!(test.verdict.expect_passed());
}

#[test]
fn vector_sum_and_matrix_expect_compute() {
    let source = "\
emath function Fold:
    inputs:
        n: Float64
    outputs:
        s: Float64
        p: Float64
        face: Matrix[2, 2]
    definitions:
        s = sum([1, 2, 3, 4, 5])
        p = product([[1, 2], [3, 4]]) * n
        face = [[1.0, 2.0], [3.0, 4.0]]
    tests:
        example <known>:
            given n = 1.0
            expect s == 15 and p == 24 and face == [[1.0, 2.0], [3.0, 4.0]]
";
    let result = check_source("fold", source);
    assert!(
        !result.diagnostics.has_errors(),
        "known-shape folds must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("s"), Some(&Value::F64(15.0)));
    assert_eq!(test.outputs.get("p"), Some(&Value::F64(24.0)));
    assert_eq!(
        test.outputs.get("face"),
        Some(&Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![1.0, 2.0, 3.0, 4.0],
        })
    );
    assert!(
        test.verdict.expect_passed(),
        "matrix expect must compare values, got {}",
        test.verdict
    );
}

#[test]
fn constant_negative_index_is_refused() {
    let source = "\
emath function NegIndex:
    inputs:
        v: Vector[3]
    outputs:
        x: Float64
    definitions:
        x = v[-1]
";
    let result = check_source("neg-index", source);
    assert!(result.diagnostics.has_errors());
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-SHAPE-006"),
        "v[-1] must be E-SHAPE-006, got {codes:?}"
    );
}

#[test]
fn mean_and_abs_on_vector_compute() {
    let source = "\nemath function VecStats:\n    inputs:\n        v: Vector[3]\n    outputs:\n        avg: Float64\n        a: Vector[3]\n    definitions:\n        avg = mean(v)\n        a = abs(v)\n    tests:\n        example <stats>:\n            given v = [1.0, -2.0, 4.0]\n            expect avg == 1.0 and a == [1.0, 2.0, 4.0]\n";
    let result = check_source("vec-stats", source);
    assert!(
        !result.diagnostics.has_errors(),
        "mean/abs on a vector must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("avg"), Some(&Value::F64(1.0)));
    assert_eq!(
        test.outputs.get("a"),
        Some(&Value::Vector(vec![1.0, 2.0, 4.0]))
    );
    assert!(
        test.verdict.expect_passed(),
        "mean/abs expect must pass, got {}",
        test.verdict
    );
}

#[test]
fn variable_bound_sum_identity_computes() {
    let source = "\
emath function TriangularSum:
    inputs:
        n: Float64
    outputs:
        total: Float64
    definitions:
        total = sum i in 0..n: i
    tests:
        example <triangular>:
            given n = 5
            expect total == 10
";
    let result = check_source("triangular-sum", source);
    assert!(
        !result.diagnostics.has_errors(),
        "variable-bound sum of i must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("total"), Some(&Value::F64(10.0)));
    assert!(
        test.verdict.expect_passed(),
        "triangular sum expect must pass, got {}",
        test.verdict
    );
}

#[test]
fn variable_bound_sum_vector_index_computes() {
    let source = "\
emath function VectorRangeSum:
    inputs:
        v: Vector[3]
    outputs:
        s: Float64
    definitions:
        n = length(v)
        s = sum i in 0..n: v[i]
    tests:
        example <range>:
            given v = [1.0, 2.0, 3.0]
            expect s == 6
";
    let result = check_source("range-sum", source);
    assert!(
        !result.diagnostics.has_errors(),
        "variable-bound sum with index must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("s"), Some(&Value::F64(6.0)));
    assert!(
        test.verdict.expect_passed(),
        "variable-bound sum with index expect must pass, got {}",
        test.verdict
    );
}

#[test]
fn forall_positive_vector_computes() {
    let source = "
emath function AllPositive:
    inputs:
        v: Vector[3]
    outputs:
        all_pos: Bool
    definitions:
        n = length(v)
        all_pos = forall i in 0..n: v[i] > 0
    tests:
        example <positive>:
            given v = [1.0, 2.0, 3.0]
            expect all_pos == true
";
    let result = check_source("forall-positive", source);
    assert!(
        !result.diagnostics.has_errors(),
        "forall must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("all_pos"), Some(&Value::Bool(true)));
    assert!(
        test.verdict.expect_passed(),
        "forall positive expect must pass, got {}",
        test.verdict
    );
}

#[test]
fn forall_fails_on_negative_element() {
    let source = "
emath function AllPositiveCheck:
    inputs:
        v: Vector[3]
    outputs:
        all_pos: Bool
    definitions:
        n = length(v)
        all_pos = forall i in 0..n: v[i] > 0
    tests:
        example <mixed>:
            given v = [1.0, -2.0, 3.0]
            expect all_pos == false
";
    let result = check_source("forall-negative", source);
    assert!(
        !result.diagnostics.has_errors(),
        "forall with failing element must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("all_pos"), Some(&Value::Bool(false)));
    assert!(
        test.verdict.expect_passed(),
        "forall false expect must pass, got {}",
        test.verdict
    );
}

#[test]
fn exists_zero_in_vector_computes() {
    let source = "
emath function HasZero:
    inputs:
        v: Vector[3]
    outputs:
        has_zero: Bool
    definitions:
        n = length(v)
        has_zero = exists i in 0..n: v[i] == 0
    tests:
        example <zero>:
            given v = [1.0, 0.0, 3.0]
            expect has_zero == true
";
    let result = check_source("exists-zero", source);
    assert!(
        !result.diagnostics.has_errors(),
        "exists must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("has_zero"), Some(&Value::Bool(true)));
    assert!(
        test.verdict.expect_passed(),
        "exists zero expect must pass, got {}",
        test.verdict
    );
}

#[test]
fn integral_of_x_computes() {
    let source = "
emath function IntegrateX:
    inputs:
        a: Float64
        b: Float64
    outputs:
        area: Float64
    definitions:
        area = integral x in a..b: x
    tests:
        example <linear>:
            given a = 0
            given b = 2
";
    let result = check_source("integral-x", source);
    assert!(
        !result.diagnostics.has_errors(),
        "integral must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let area = match test.outputs.get("area") {
        Some(Value::F64(v)) => *v,
        _ => panic!(
            "expected f64 output for area, got {:?}",
            test.outputs.get("area")
        ),
    };
    // Simpson's rule is exact for polynomials of degree <= 3.
    assert!(
        (area - 2.0).abs() < 1e-10,
        "integral of x from 0 to 2 should be ~2.0, got {area}"
    );
}

#[test]
fn integral_of_x_squared_computes() {
    let source = "
emath function IntegrateXSquared:
    inputs:
        n: Float64
    outputs:
        area: Float64
    definitions:
        area = integral x in 0..3: x * x * n
    tests:
        example <quadratic>:
            given n = 1.0
";
    let result = check_source("integral-xsquared", source);
    assert!(
        !result.diagnostics.has_errors(),
        "integral of x*x must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let area = match test.outputs.get("area") {
        Some(Value::F64(v)) => *v,
        _ => panic!(
            "expected f64 output for area, got {:?}",
            test.outputs.get("area")
        ),
    };
    // Integral of x^2 from 0 to 3 = 9.  Simpson's rule is exact for degree <= 3.
    assert!(
        (area - 9.0).abs() < 1e-10,
        "integral of x*x from 0 to 3 should be ~9.0, got {area}"
    );
}

#[test]
fn derivative_of_x_squared_computes() {
    let source = "
emath function AutoDiffSquare:
    inputs:
        x: Float64
    outputs:
        y: Float64
        dy: Float64
    definitions:
        y = x * x
        dy = derivative(y) wrt x
    tests:
        example <parabola>:
            given x = 3
            expect dy == 6
";
    let result = check_source("autodiff-square", source);
    assert!(
        !result.diagnostics.has_errors(),
        "derivative must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("dy"), Some(&Value::F64(6.0)));
    assert!(
        test.verdict.expect_passed(),
        "derivative of x*x at x=3 should be 6, got {}",
        test.verdict
    );
}

#[test]
fn derivative_of_sin_computes() {
    let source = "
emath function AutoDiffSin:
    inputs:
        x: Float64
    outputs:
        dy: Float64
    definitions:
        dy = derivative(sin(x)) wrt x
    tests:
        example <sin>:
            given x = 0
            expect dy == 1
";
    let result = check_source("autodiff-sin", source);
    assert!(
        !result.diagnostics.has_errors(),
        "derivative of sin must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("dy"), Some(&Value::F64(1.0)));
    assert!(
        test.verdict.expect_passed(),
        "derivative of sin(x) at x=0 should be 1, got {}",
        test.verdict
    );
}

#[test]
fn solve_finds_root_of_quadratic() {
    let source = "
emath function SolveRoot:
    inputs:
        x: Float64
    outputs:
        root: Float64
    definitions:
        residual = x * x - 4
        root = solve(residual) wrt x
    tests:
        example <from_one>:
            given x = 1
            expect abs(root - 2) < 0.001
";
    let result = check_source("solve-root", source);
    assert!(
        !result.diagnostics.has_errors(),
        "solve must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let root = test.outputs.get("root");
    assert!(root.is_some(), "root output missing");
    let root_val = match root {
        Some(Value::F64(v)) => *v,
        other => panic!("root should be F64, got {other:?}"),
    };
    assert!(
        (root_val - 2.0).abs() < 1e-9,
        "solve(x^2-4) wrt x from x=1 should converge to 2, got {root_val}"
    );
    assert!(
        (root_val * root_val - 4.0).abs() < 1e-12,
        "claimed root {root_val} has residual {}",
        root_val * root_val - 4.0
    );
}

#[test]
fn minimize_finds_minimum() {
    let source = "
emath function MinimizeSquare:
    inputs:
        x: Float64
    outputs:
        optimum: Float64
    definitions:
        loss = (x - 3) * (x - 3)
        optimum = minimize(loss) wrt x
    tests:
        example <from_zero>:
            given x = 0
            expect abs(optimum - 3) < 0.1
";
    let result = check_source("minimize-square", source);
    assert!(
        !result.diagnostics.has_errors(),
        "minimize must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let opt = match test.outputs.get("optimum") {
        Some(Value::F64(v)) => *v,
        other => panic!("optimum should be F64, got {other:?}"),
    };
    assert!(
        (opt - 3.0).abs() < 1e-6,
        "minimize((x-3)^2) wrt x from x=0 should converge to 3, got {opt}"
    );
    assert!(
        (2.0 * (opt - 3.0)).abs() < 1e-6,
        "claimed min {opt} has gradient {}, not stationary",
        2.0 * (opt - 3.0)
    );
}

#[test]
fn maximize_finds_maximum() {
    let source = "
emath function MaximizeNegSquare:
    inputs:
        x: Float64
    outputs:
        optimum: Float64
    definitions:
        score = -(x - 2) * (x - 2)
        optimum = maximize(score) wrt x
    tests:
        example <from_zero>:
            given x = 0
            expect abs(optimum - 2) < 0.1
";
    let result = check_source("maximize-neg-square", source);
    assert!(
        !result.diagnostics.has_errors(),
        "maximize must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let opt = match test.outputs.get("optimum") {
        Some(Value::F64(v)) => *v,
        other => panic!("optimum should be F64, got {other:?}"),
    };
    assert!(
        (opt - 2.0).abs() < 1e-6,
        "maximize(-(x-2)^2) wrt x from x=0 should converge to 2, got {opt}"
    );
    assert!(
        (-2.0 * (opt - 2.0)).abs() < 1e-6,
        "claimed max {opt} has gradient {}, not stationary",
        -2.0 * (opt - 2.0)
    );
}

#[test]
fn minimize_multi_variable_converges() {
    let source = "
emath function MultiVarOpt:
    inputs:
        x: Float64
        y: Float64
    outputs:
        opt_x: Float64
        opt_y: Float64
    definitions:
        loss = (x - 1) * (x - 1) + (y - 2) * (y - 2)
        opt_x = minimize(loss) wrt x, y
        opt_y = minimize(loss) wrt y, x
    tests:
        example <bowl>:
            given x = 0
            given y = 0
            expect abs(opt_x - 1) < 0.1
            expect abs(opt_y - 2) < 0.1
";
    let result = check_source("multivar-opt", source);
    assert!(
        !result.diagnostics.has_errors(),
        "multi-variable minimize must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let opt_x = match test.outputs.get("opt_x") {
        Some(Value::F64(v)) => *v,
        other => panic!("opt_x should be F64, got {other:?}"),
    };
    let opt_y = match test.outputs.get("opt_y") {
        Some(Value::F64(v)) => *v,
        other => panic!("opt_y should be F64, got {other:?}"),
    };
    assert!(
        (opt_x - 1.0).abs() < 1e-6,
        "minimize((x-1)^2 + (y-2)^2) wrt x,y from (0,0) should converge x to 1, got {opt_x}"
    );
    assert!(
        (opt_y - 2.0).abs() < 1e-6,
        "minimize((x-1)^2 + (y-2)^2) wrt y,x from (0,0) should converge y to 2, got {opt_y}"
    );
}

#[test]
fn exact_integer_product_fold() {
    // 20! = 2432902008176640000 does not fit in f64's 53-bit mantissa.
    // Unrolled Int product must stay on the exact i64 path, not round
    // through Float64 and convert back.
    let source = "\
emath function ExactFactorial:
    inputs:
        n: Int
    outputs:
        fac: Int
    definitions:
        fac = product i in 1..=n: i
    tests:
        example <twenty>:
            given n = 20
            expect fac == 2432902008176640000
";
    let result = check_source("exact_factorial", source);
    assert!(
        !result.diagnostics.has_errors(),
        "exact integer product must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(
        test.outputs.get("fac"),
        Some(&Value::I64(2_432_902_008_176_640_000)),
        "20! should be exact i64 2432902008176640000, got {:?}",
        test.outputs.get("fac")
    );
    assert!(test.verdict.expect_passed());
}

#[test]
fn constraints_section_feeds_optimization() {
    let source = "\
emath function ConstrainedMin:
    inputs:
        x: Float64
        y: Float64
    outputs:
        opt_x: Float64
        opt_y: Float64
    constraints:
        x + y >= 1
    definitions:
        objective = x * x + y * y
        opt_x = minimize(objective) wrt x, y
        opt_y = minimize(objective) wrt y, x
    tests:
        example <demo>:
            given x = 0
            given y = 0
            expect abs(opt_x - 0.5) < 0.01
            expect abs(opt_y - 0.5) < 0.01
            expect opt_x + opt_y >= 0.999
";
    let result = check_source("constrained_min", source);
    assert!(
        !result.diagnostics.has_errors(),
        "constrained optimization must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let opt_x = match test.outputs.get("opt_x") {
        Some(Value::F64(v)) => *v,
        Some(Value::I64(v)) => *v as f64,
        other => panic!("opt_x should be numeric, got {other:?}"),
    };
    let opt_y = match test.outputs.get("opt_y") {
        Some(Value::F64(v)) => *v,
        Some(Value::I64(v)) => *v as f64,
        other => panic!("opt_y should be numeric, got {other:?}"),
    };
    assert!(
        (opt_x - 0.5).abs() < 0.01,
        "constrained minimum should be near x=0.5 (penalty eq. of x+y>=1), got {opt_x}"
    );
    assert!(
        (opt_y - 0.5).abs() < 0.01,
        "constrained minimum should be near y=0.5, got {opt_y}"
    );
    assert!(
        opt_x + opt_y >= 0.999,
        "penalty must nearly enforce x+y>=1, got {}",
        opt_x + opt_y
    );
    assert!(test.verdict.expect_passed());
}

#[test]
fn intro_solve_example_residual_is_zero_in_both_basins() {
    let source = include_str!("../../../language/examples/intro/solve.emath");
    let result = check_source("solve-example", source);
    assert!(
        !result.diagnostics.has_errors(),
        "solve.emath must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    assert_eq!(report.declarations[0].tests.len(), 2);
    let pos = match report.declarations[0].tests[0].outputs.get("root") {
        Some(Value::F64(v)) => *v,
        other => panic!("from_one root should be F64, got {other:?}"),
    };
    let neg = match report.declarations[0].tests[1].outputs.get("root") {
        Some(Value::F64(v)) => *v,
        other => panic!("from_neg_one root should be F64, got {other:?}"),
    };
    assert!(
        (pos - 2.0).abs() < 1e-9 && (pos * pos - 4.0).abs() < 1e-12,
        "from x=1 must be +2 with residual ~0, got {pos}"
    );
    assert!(
        (neg + 2.0).abs() < 1e-9 && (neg * neg - 4.0).abs() < 1e-12,
        "from x=-1 must be -2 with residual ~0, got {neg}"
    );
    assert!(report.declarations[0].tests[0].verdict.expect_passed());
    assert!(report.declarations[0].tests[1].verdict.expect_passed());
}

#[test]
fn intro_optimize_example_is_stationary() {
    let source = include_str!("../../../language/examples/intro/optimize.emath");
    let result = check_source("optimize-example", source);
    assert!(
        !result.diagnostics.has_errors(),
        "optimize.emath must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let min_x = match test.outputs.get("min_x") {
        Some(Value::F64(v)) => *v,
        other => panic!("min_x should be F64, got {other:?}"),
    };
    let max_x = match test.outputs.get("max_x") {
        Some(Value::F64(v)) => *v,
        other => panic!("max_x should be F64, got {other:?}"),
    };
    assert!(
        (2.0 * (min_x - 3.0)).abs() < 1e-6,
        "min_x={min_x} is not stationary for (x-3)^2"
    );
    assert!(
        (-2.0 * (max_x - 2.0)).abs() < 1e-6,
        "max_x={max_x} is not stationary for -(x-2)^2"
    );
    assert!(test.verdict.expect_passed());
}

#[test]
fn intro_constrained_opt_example_nearly_enforces_constraint() {
    let source = include_str!("../../../tests/fixtures/language/intro/constrained-optimization.emath");
    let result = check_source("constrained-opt-example", source);
    assert!(
        !result.diagnostics.has_errors(),
        "constrained-opt.emath must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let opt_x = match test.outputs.get("opt_x") {
        Some(Value::F64(v)) => *v,
        other => panic!("opt_x should be F64, got {other:?}"),
    };
    let opt_y = match test.outputs.get("opt_y") {
        Some(Value::F64(v)) => *v,
        other => panic!("opt_y should be F64, got {other:?}"),
    };
    assert!(
        opt_x + opt_y >= 0.999,
        "constrained-opt example must nearly satisfy x+y>=1, got {} + {} = {}",
        opt_x,
        opt_y,
        opt_x + opt_y
    );
    assert!(test.verdict.expect_passed());
}

#[test]
fn heat_rod_laplacian_admits_and_inline_tests_pass() {
    let source = r#"
emath function HeatStep:
    inputs:
        u: Vector[5]
        alpha: Float64
        dt: Float64

    outputs:
        next: Vector[5]
        next_dirichlet: Vector[5]
        next_neumann: Vector[5]

    definitions:
        next = u + dt * alpha * laplacian(u, 1.0)
        next_dirichlet = u + dt * alpha * laplacian_dirichlet(u, 1.0, 0.0, 0.0)
        next_neumann = u + dt * alpha * laplacian_neumann(u, 1.0)

    tests:
        example <constant_holds>:
            given u = [5.0, 5.0, 5.0, 5.0, 5.0]
            given alpha = 1.0
            given dt = 1.0
            expect next == [5.0, 5.0, 5.0, 5.0, 5.0]

        example <zero_dt_identity>:
            given u = [0.0, 1.0, 4.0, 9.0, 16.0]
            given alpha = 1.0
            given dt = 0.0
            expect next == [0.0, 1.0, 4.0, 9.0, 16.0]

        example <dirichlet_cools_boundary>:
            given u = [5.0, 5.0, 5.0, 5.0, 5.0]
            given alpha = 1.0
            given dt = 1.0
            expect next_dirichlet == [0.0, 5.0, 5.0, 5.0, 0.0]

        example <neumann_insulated_linear>:
            given u = [0.0, 1.0, 2.0, 3.0, 4.0]
            given alpha = 1.0
            given dt = 1.0
            expect next_neumann == [2.0, 1.0, 2.0, 3.0, 2.0]
"#;
    let result = check_source("heat-rod", source);
    assert!(
        !result.diagnostics.has_errors(),
        "heat-rod must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    // constant_holds: the laplacian of a constant field is zero everywhere
    // (clamped edges replicate the boundary), so one Euler step leaves u fixed.
    let t0 = &report.declarations[0].tests[0];
    assert_eq!(t0.outputs.get("next"), Some(&Value::Vector(vec![5.0; 5])));
    assert!(t0.verdict.expect_passed());
    // zero_dt_identity: dt = 0 zeroes the diffusion update, so next == u.
    let t1 = &report.declarations[0].tests[1];
    assert_eq!(
        t1.outputs.get("next"),
        Some(&Value::Vector(vec![0.0, 1.0, 4.0, 9.0, 16.0]))
    );
    assert!(t1.verdict.expect_passed());
    // Dirichlet boundaries held at 0 cool the edge cells of a constant
    // field; the interior holds its temperature.
    let t2 = &report.declarations[0].tests[2];
    assert_eq!(
        t2.outputs.get("next_dirichlet"),
        Some(&Value::Vector(vec![0.0, 5.0, 5.0, 5.0, 0.0]))
    );
    assert!(t2.verdict.expect_passed());
    // Neumann (insulated) on a linear field: the mirrored ghost pulls the
    // edge cells toward the interior (no heat flux out).
    let t3 = &report.declarations[0].tests[3];
    assert_eq!(
        t3.outputs.get("next_neumann"),
        Some(&Value::Vector(vec![2.0, 1.0, 2.0, 3.0, 2.0]))
    );
    assert!(t3.verdict.expect_passed());
}

#[test]
fn heat_plate_2d_laplacian_admits_and_inline_tests_pass() {
    let source = r#"
emath function HeatPlate:
    inputs:
        u: Matrix[3, 3]
        alpha: Float64
        dt: Float64

    outputs:
        next: Matrix[3, 3]

    definitions:
        next = u + dt * alpha * laplacian_2d(u, 1.0)

    tests:
        example <constant_holds>:
            given u = [[5.0, 5.0, 5.0], [5.0, 5.0, 5.0], [5.0, 5.0, 5.0]]
            given alpha = 1.0
            given dt = 1.0
            expect next == [[5.0, 5.0, 5.0], [5.0, 5.0, 5.0], [5.0, 5.0, 5.0]]

        example <hot_spot_diffuses>:
            given u = [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]]
            given alpha = 1.0
            given dt = 1.0
            expect next == [[0.0, 1.0, 0.0], [1.0, -3.0, 1.0], [0.0, 1.0, 0.0]]
"#;
    let result = check_source("heat-plate", source);
    assert!(
        !result.diagnostics.has_errors(),
        "heat-plate must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    // constant_holds: a constant field has a zero laplacian, so the plate
    // holds its temperature under one Euler step.
    let t0 = &report.declarations[0].tests[0];
    assert_eq!(
        t0.outputs.get("next"),
        Some(&Value::Matrix {
            rows: 3,
            cols: 3,
            data: vec![5.0; 9]
        })
    );
    assert!(t0.verdict.expect_passed());
    // hot_spot_diffuses: a single hot cell spreads to its four neighbors
    // and drops by 4 (the 5-point laplacian center term).
    let t1 = &report.declarations[0].tests[1];
    assert_eq!(
        t1.outputs.get("next"),
        Some(&Value::Matrix {
            rows: 3,
            cols: 3,
            data: vec![0.0, 1.0, 0.0, 1.0, -3.0, 1.0, 0.0, 1.0, 0.0]
        })
    );
    assert!(t1.verdict.expect_passed());
}

#[test]
fn gradient_field_admits_and_inline_tests_pass() {
    let source = include_str!("../../../tests/fixtures/language/numerical/gradient-field.emath");
    let result = check_source("gradient-field", source);
    assert!(
        !result.diagnostics.has_errors(),
        "gradient-field must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    // linear_vector_has_constant_gradient: du/dx of a slope-1 ramp is 1
    // everywhere (one-sided edges, not the clamp-central 0.5); a zero
    // matrix has zero gradients.
    let t0 = &report.declarations[0].tests[0];
    assert_eq!(
        t0.outputs.get("du"),
        Some(&Value::Vector(vec![1.0, 1.0, 1.0, 1.0, 1.0]))
    );
    assert!(t0.verdict.expect_passed());
    // ramp_matrix_has_axis_gradients: du/dc of a column-ramp is 1
    // everywhere; du/dr of a column-ramp is zero (no row variation).
    let t1 = &report.declarations[0].tests[1];
    assert_eq!(
        t1.outputs.get("gx"),
        Some(&Value::Matrix {
            rows: 3,
            cols: 3,
            data: vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]
        })
    );
    assert!(t1.verdict.expect_passed());
}

// ---- B02: filtered binder tests ---------------------------------------

#[test]
fn filtered_sum_computes() {
    // `sum i in 0..n if i > 2: i` — sums only elements > 2.
    // For n=5: 3 + 4 = 7
    let source = "\
emath function FilteredSum:
    inputs:
        n: Float64
    outputs:
        total: Float64
    definitions:
        total = sum i in 0..n if i > 2: i
    tests:
        example <filtered>:
            given n = 5
            expect total == 7
";
    let result = check_source("filtered-sum", source);
    assert!(
        !result.diagnostics.has_errors(),
        "filtered sum must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(
        test.outputs.get("total"),
        Some(&Value::F64(7.0)),
        "filtered sum of i>2 for n=5 should be 7 (3+4)"
    );
    assert!(
        test.verdict.expect_passed(),
        "filtered sum expect must pass, got {}",
        test.verdict
    );
}

#[test]
fn always_false_filter_gives_identity() {
    // `sum i in 0..n if i < 0: i` — always-false filter = empty sum = 0
    let source = "\
emath function EmptyFilteredSum:
    inputs:
        n: Float64
    outputs:
        total: Float64
    definitions:
        total = sum i in 0..n if i < 0: i
    tests:
        example <empty>:
            given n = 5
            expect total == 0
";
    let result = check_source("empty-filtered-sum", source);
    assert!(
        !result.diagnostics.has_errors(),
        "always-false filtered sum must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(
        test.outputs.get("total"),
        Some(&Value::F64(0.0)),
        "always-false filter should give identity (0)"
    );
    assert!(
        test.verdict.expect_passed(),
        "empty filtered sum expect must pass, got {}",
        test.verdict
    );
}

#[test]
fn filtered_forall_computes() {
    // `forall i in 0..n if i < n: i >= 0` — all elements less than n
    // are non-negative. For n=5: all of 0,1,2,3,4 are >= 0 → true.
    let source = "\
emath function FilteredForAll:
    inputs:
        n: Float64
    outputs:
        ok: Bool
    definitions:
        ok = forall i in 0..n if i < n: i >= 0
    tests:
        example <filteredforall>:
            given n = 5
            expect ok == true
";
    let result = check_source("filtered-forall", source);
    assert!(
        !result.diagnostics.has_errors(),
        "filtered forall must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(
        test.outputs.get("ok"),
        Some(&Value::Bool(true)),
        "filtered forall should be true"
    );
    assert!(
        test.verdict.expect_passed(),
        "filtered forall expect must pass, got {}",
        test.verdict
    );
}

#[test]
fn integer_exponent_power_computes() {
    // Spec "Implemented today": `+ - * / ^`. Integer exponents must not
    // refuse just because the literal is Nat rather than Float64.
    let source = "\
emath function Power:
    inputs:
        x: Float64
    outputs:
        y: Float64
        dy: Float64
    definitions:
        y = x^2
        dy = derivative(x^2) wrt x
    tests:
        example <nine>:
            given x = 3
            expect y == 9
            expect dy == 6
";
    let result = check_source("integer-pow", source);
    assert!(
        !result.diagnostics.has_errors(),
        "x^2 must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("y"), Some(&Value::F64(9.0)));
    assert_eq!(test.outputs.get("dy"), Some(&Value::F64(6.0)));
    assert!(
        test.verdict.expect_passed(),
        "x^2 at x=3 should be 9 and d/dx=6, got {}",
        test.verdict
    );
}

#[test]
fn partial_without_holding_is_refused() {
    // Spec: a partial without `holding` is a MeaningHole — do not guess
    // which variables are held fixed.
    let source = "\
emath function PartialBare:
    inputs:
        x: Float64
        y: Float64
    outputs:
        d: Float64
    definitions:
        d = partial(x * y) wrt x
";
    let result = check_source("partial-no-holding", source);
    assert!(
        result.diagnostics.has_errors(),
        "partial without holding must refuse, not silently autodiff"
    );
    let messages: Vec<String> = result.diagnostics.errors().map(|d| d.to_string()).collect();
    assert!(
        messages.iter().any(|m| m.contains("holding")),
        "refusal must mention holding, got {messages:?}"
    );
}

#[test]
fn partial_with_holding_computes() {
    let source = "\
emath function PartialHeld:
    inputs:
        x: Float64
        y: Float64
    outputs:
        d: Float64
    definitions:
        d = partial(x * y) wrt x holding y
    tests:
        example <held>:
            given x = 3
            given y = 5
            expect d == 5
";
    let result = check_source("partial-holding", source);
    assert!(
        !result.diagnostics.has_errors(),
        "partial with holding must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(test.outputs.get("d"), Some(&Value::F64(5.0)));
    assert!(
        test.verdict.expect_passed(),
        "partial(x*y) wrt x holding y at (3,5) should be 5, got {}",
        test.verdict
    );
}

/// CAPABILITY: factorial is exact i64 on [0, 20]; n=21 named-refuses.
#[test]
fn factorial_domain_computes_and_refuses() {
    let source = "\
emath function Fac:
    inputs:
        n: Int
    outputs:
        z: Int
        f5: Int
        f20: Int
    definitions:
        z = factorial(0)
        f5 = factorial(n)
        f20 = factorial(20)
    tests:
        example <ok>:
            given n = 5
            expect z == 1
            expect f5 == 120
            expect f20 == 2432902008176640000
";
    let fac = check_source("fac-ok", source);
    assert!(
        !fac.diagnostics.has_errors(),
        "factorial happy path must admit, got: {:?}",
        fac.diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let t = &run_package(&fac.package).declarations[0].tests[0];
    assert_eq!(t.outputs.get("z"), Some(&Value::I64(1)));
    assert_eq!(t.outputs.get("f5"), Some(&Value::I64(120)));
    assert_eq!(
        t.outputs.get("f20"),
        Some(&Value::I64(2_432_902_008_176_640_000))
    );
    assert!(
        t.verdict.expect_passed(),
        "factorial expects must pass, got {}",
        t.verdict
    );

    let overflow = "\
emath function Fac21:
    inputs:
        n: Int
    outputs:
        f: Int
    definitions:
        f = factorial(n)
    tests:
        example <overflow>:
            given n = 21
            expect f == 0
";
    let fac21 = check_source("fac-21", overflow);
    assert!(
        !fac21.diagnostics.has_errors(),
        "factorial(21) must admit then named-refuse at eval, got: {:?}",
        fac21
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let t21 = &run_package(&fac21.package).declarations[0].tests[0];
    assert!(
        t21.verdict.is_refused(),
        "factorial(21) must named-refuse, not wrap, got {} outputs={:?}",
        t21.verdict,
        t21.outputs
    );

    // 0/0 is IEEE NaN; `as i64` would map that to 0 and return 0! = 1.
    let nan = "\
emath function FacNan:
    inputs:
        n: Int
    outputs:
        f: Int
    definitions:
        f = factorial(n / 0)
    tests:
        example <nan>:
            given n = 0
            expect f == 1
";
    let fac_nan = check_source("fac-nan", nan);
    assert!(
        !fac_nan.diagnostics.has_errors(),
        "factorial(0/0) must admit then named-refuse at eval, got: {:?}",
        fac_nan
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let tnan = &run_package(&fac_nan.package).declarations[0].tests[0];
    assert!(
        tnan.verdict.is_refused(),
        "factorial(0/0) must not silently return 1, got {} outputs={:?}",
        tnan.verdict,
        tnan.outputs
    );
}

/// CAPABILITY modular/coding builtins vs documented closed forms.
#[test]
fn modular_and_coding_builtins_compute() {
    let source = include_str!("../../../tests/fixtures/language/intro/modular-arithmetic.emath");
    let result = check_source("modular-coding", source);
    assert!(
        !result.diagnostics.has_errors(),
        "modular-arithmetic.emath must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    assert_eq!(report.declarations.len(), 4);
    let basics = &report.declarations[0].tests[0];
    assert_eq!(basics.outputs.get("inv3"), Some(&Value::I64(5)));
    assert_eq!(basics.outputs.get("check"), Some(&Value::Bool(true)));
    assert_eq!(basics.outputs.get("fac6"), Some(&Value::I64(720)));
    assert_eq!(basics.outputs.get("wilson_ok"), Some(&Value::Bool(true)));
    assert!(
        basics.verdict.expect_passed(),
        "modular_basics must pass, got {}",
        basics.verdict
    );

    let distance = &report.declarations[1].tests[0];
    assert_eq!(distance.outputs.get("distance"), Some(&Value::F64(5.0)));
    assert!(
        distance.verdict.expect_passed(),
        "rs_distance must pass, got {}",
        distance.verdict
    );

    let encode = &report.declarations[2].tests[0];
    assert_eq!(encode.outputs.get("val_at_2"), Some(&Value::I64(3)));
    assert_eq!(encode.outputs.get("d0"), Some(&Value::I64(0)));
    assert!(
        encode.verdict.expect_passed(),
        "rs_encode_demo must pass, got {} outputs={:?}",
        encode.verdict,
        encode.outputs
    );

    let table = &report.declarations[3].tests[0];
    assert_eq!(
        table.outputs.get("table"),
        Some(&Value::Vector(vec![1.0, 4.0, 5.0, 2.0, 3.0, 6.0]))
    );
    assert!(
        table.verdict.expect_passed(),
        "gf7_inverse_table must pass, got {}",
        table.verdict
    );
}

/// Remaining scalar builtins: hypot, lerp, clamp, recip, cbrt, sign.
#[test]
fn remaining_scalar_builtins_closed_form() {
    let source = "\
emath function Scalars:
    inputs:
        n: Float64
    outputs:
        h: Float64
        l: Float64
        c: Float64
        r: Float64
        cb: Float64
        s0: Float64
        sneg: Float64
        spos: Float64
    definitions:
        h = hypot(n, 4)
        l = lerp(0, 10, 0.5)
        c = clamp(12, 0, 10)
        r = recip(4)
        cb = cbrt(8)
        s0 = sign(0)
        sneg = sign(-2)
        spos = sign(3)
    tests:
        example <closed>:
            given n = 3.0
            expect h == 5
            expect l == 5
            expect c == 10
            expect r == 0.25
            expect cb == 2
            expect s0 == 0
            expect sneg == -1
            expect spos == 1
";
    let sc = check_source("scalar-rest", source);
    assert!(
        !sc.diagnostics.has_errors(),
        "remaining scalar builtins must admit, got: {:?}",
        sc.diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let st = &run_package(&sc.package).declarations[0].tests[0];
    assert_eq!(
        st.outputs.get("h"),
        Some(&Value::F64(5.0)),
        "hypot(3,4) must be 5, got {:?}",
        st.outputs.get("h")
    );
    assert_eq!(
        st.outputs.get("l"),
        Some(&Value::F64(5.0)),
        "lerp(0,10,0.5) must be 5, got {:?}",
        st.outputs.get("l")
    );
    assert_eq!(
        st.outputs.get("c"),
        Some(&Value::F64(10.0)),
        "clamp(12,0,10) must be 10, got {:?}",
        st.outputs.get("c")
    );
    assert_eq!(
        st.outputs.get("r"),
        Some(&Value::F64(0.25)),
        "recip(4) must be 0.25, got {:?}",
        st.outputs.get("r")
    );
    assert_eq!(
        st.outputs.get("cb"),
        Some(&Value::F64(2.0)),
        "cbrt(8) must be 2, got {:?}",
        st.outputs.get("cb")
    );
    assert_eq!(
        st.outputs.get("s0"),
        Some(&Value::F64(0.0)),
        "sign(0) must be 0, got {:?}",
        st.outputs.get("s0")
    );
    assert_eq!(
        st.outputs.get("sneg"),
        Some(&Value::F64(-1.0)),
        "sign(-2) must be -1, got {:?}",
        st.outputs.get("sneg")
    );
    assert_eq!(
        st.outputs.get("spos"),
        Some(&Value::F64(1.0)),
        "sign(3) must be 1, got {:?}",
        st.outputs.get("spos")
    );
    assert!(
        st.verdict.expect_passed(),
        "scalar closed forms must pass, got {}",
        st.verdict
    );
}

/// `core::math::add` (and sub/mul/div/neg) must compute the same as
/// `+ - * /` and unary `-`. HIR/notation already treated them as
/// arity-known builtins; lowering used to leave them unbound.
#[test]
fn arithmetic_function_duals_match_operators() {
    let happy = run_one(
        "arith-duals",
        "\
emath function Duals:
    inputs:
        base: Float64
    outputs:
        a: Float64
        s: Float64
        m: Float64
        d: Float64
        n: Float64
        via_op: Float64
        via_caret: Float64
        via_pow: Float64
    definitions:
        a = core::math::add(base, 3)
        s = sub(10, 4)
        m = math::mul(3, 5)
        d = div(9, 3)
        n = neg(6)
        via_op = 2 + 3
        via_caret = 2 ^ 3
        via_pow = core::math::pow(2, 3)
    tests:
        example <duals>:
            given base = 2.0
            expect a == 5
            expect s == 6
            expect m == 15
            expect d == 3
            expect n == -6
            expect a == via_op
            expect via_caret == 8
            expect via_pow == via_caret
",
    );
    assert_eq!(happy.outputs.get("a"), Some(&Value::F64(5.0)), "add(2,3)");
    assert_eq!(happy.outputs.get("s"), Some(&Value::F64(6.0)), "sub(10,4)");
    assert_eq!(happy.outputs.get("m"), Some(&Value::F64(15.0)), "mul(3,5)");
    assert_eq!(happy.outputs.get("d"), Some(&Value::F64(3.0)), "div(9,3)");
    assert_eq!(happy.outputs.get("n"), Some(&Value::F64(-6.0)), "neg(6)");
    assert_eq!(happy.outputs.get("via_op"), Some(&Value::F64(5.0)), "2+3");
    assert_eq!(
        happy.outputs.get("via_caret"),
        Some(&Value::F64(8.0)),
        "2^3"
    );
    assert_eq!(
        happy.outputs.get("via_pow"),
        Some(&Value::F64(8.0)),
        "pow(2,3) must match 2^3"
    );
    assert!(
        happy.verdict.expect_passed(),
        "arithmetic duals must pass, got {}",
        happy.verdict
    );

    let noted = run_one(
        "arith-duals-notation",
        "\
emath function Noted:
    inputs:
        x: Float64
        y: Float64
    outputs:
        r: Float64
        s: Float64
    definitions:
        r = x ⊕ y
        s = x + y
    tests:
        example <plus_glyph>:
            given x = 2
            given y = 3
            expect r == 5
            expect r == s
notation infixl 40 \"⊕\" => core::math::add
",
    );
    assert_eq!(noted.outputs.get("r"), Some(&Value::F64(5.0)));
    assert_eq!(noted.outputs.get("s"), Some(&Value::F64(5.0)));
    assert!(
        noted.verdict.expect_passed(),
        "notation targeting add must compute, got {}",
        noted.verdict
    );
}

#[test]
fn len_alias_is_unknown_after_removal() {
    let source = "\
emath function LenGone:
    inputs:
        v: Vector[3]
    outputs:
        n: Float64
    definitions:
        n = len(v)
";
    let result = check_source("len-gone", source);
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-TYPE-003"),
        "len(v) must be unknown (length is the canonical name), got {codes:?}"
    );
}

/// Empty vector literals are a named shape refuse, not a silent NaN
/// from `mean([])` / `0/0` or a 0-norm of a 0-length vector.
#[test]
fn empty_vector_literal_mean_and_norm_are_named_refuse() {
    for (name, source) in [
        (
            "empty-lit",
            "\
emath function EmptyLit:
    outputs:
        v: Vector[1]
    definitions:
        v = []
",
        ),
        (
            "mean-empty",
            "\
emath function MeanEmpty:
    outputs:
        m: Float64
    definitions:
        m = mean([])
",
        ),
        (
            "norm-empty",
            "\
emath function NormEmpty:
    outputs:
        n: Float64
    definitions:
        n = norm([])
",
        ),
    ] {
        let result = check_source(name, source);
        assert!(
            result.diagnostics.has_errors(),
            "{name} must named-refuse, not admit"
        );
        let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
        assert!(
            codes.contains(&"E-SHAPE-004"),
            "{name} must be E-SHAPE-004 empty vector, got {codes:?}"
        );
    }
}

fn run_one(name: &str, source: &str) -> emath_exec_ir::runner::TestRun {
    let checked = check_source(name, source);
    assert!(
        !checked.diagnostics.has_errors(),
        "{name} must admit, got: {:?}",
        checked
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    run_package(&checked.package).declarations[0].tests[0].clone()
}

/// Closed-form builtins vs known values, including domain edges.
/// `sqrt(-1)` / `ln(-1)` / `log(0)` follow the named IEEE strict-f64 policy
/// (NaN / -Inf, not a silent finite lie). `mod_inv` is a typed refuse.
#[test]
fn closed_form_builtin_numeric_honesty() {
    let happy = run_one(
        "cf-happy",
        "\
emath function Closed:
    inputs:
        n: Float64
    outputs:
        s0: Float64
        e0: Float64
        c0: Float64
        sq4: Float64
        p00: Float64
        caret00: Float64
        a00: Float64
        l2: Float64
        l10: Float64
        ln1: Float64
        log1: Float64
        log10e: Float64
        a10: Float64
        cbneg: Float64
    definitions:
        s0 = sin(n)
        e0 = exp(0)
        c0 = cos(0)
        sq4 = sqrt(4)
        p00 = pow(0, 0)
        caret00 = 0 ^ 0
        a00 = atan2(0, 0)
        l2 = log2(8)
        l10 = log10(1000)
        ln1 = ln(1)
        log1 = log(1)
        log10e = log(10)
        a10 = atan2(1, 0)
        cbneg = cbrt(-8)
    tests:
        example <closed>:
            given n = 0.0
            expect s0 == 0
            expect e0 == 1
            expect c0 == 1
            expect sq4 == 2
            expect p00 == 1
            expect caret00 == 1
            expect a00 == 0
            expect l2 == 3
            expect l10 == 3
            expect ln1 == 0
            expect log1 == 0
            expect cbneg == -2
",
    );
    assert_eq!(happy.outputs.get("s0"), Some(&Value::F64(0.0)), "sin(0)");
    assert_eq!(happy.outputs.get("e0"), Some(&Value::F64(1.0)), "exp(0)");
    assert_eq!(happy.outputs.get("c0"), Some(&Value::F64(1.0)), "cos(0)");
    assert_eq!(happy.outputs.get("sq4"), Some(&Value::F64(2.0)), "sqrt(4)");
    assert_eq!(
        happy.outputs.get("p00"),
        Some(&Value::F64(1.0)),
        "pow(0,0) is IEEE 1, got {:?}",
        happy.outputs.get("p00")
    );
    assert_eq!(
        happy.outputs.get("caret00"),
        Some(&Value::F64(1.0)),
        "0^0 is IEEE 1, got {:?}",
        happy.outputs.get("caret00")
    );
    assert_eq!(
        happy.outputs.get("a00"),
        Some(&Value::F64(0.0)),
        "atan2(0,0) is IEEE +0, got {:?}",
        happy.outputs.get("a00")
    );
    assert_eq!(happy.outputs.get("l2"), Some(&Value::F64(3.0)), "log2(8)");
    assert_eq!(
        happy.outputs.get("l10"),
        Some(&Value::F64(3.0)),
        "log10(1000)"
    );
    assert_eq!(happy.outputs.get("ln1"), Some(&Value::F64(0.0)), "ln(1)");
    assert_eq!(happy.outputs.get("log1"), Some(&Value::F64(0.0)), "log(1)");
    match happy.outputs.get("log10e") {
        Some(Value::F64(v)) => {
            assert!(
                (v - 10.0_f64.ln()).abs() < 1e-12,
                "log is ln (not log10): log(10)={v} ln(10)={}",
                10.0_f64.ln()
            );
            assert!(
                (v - 1.0).abs() > 0.5,
                "log(10) must not be log10(10)=1, got {v}"
            );
        }
        other => panic!("log(10) must be Float64, got {other:?}"),
    }
    match happy.outputs.get("a10") {
        Some(Value::F64(v)) => {
            assert!(
                (v - std::f64::consts::FRAC_PI_2).abs() < 1e-12,
                "atan2(1,0) must be π/2, got {v}"
            );
        }
        other => panic!("atan2(1,0) must be Float64, got {other:?}"),
    }
    assert_eq!(
        happy.outputs.get("cbneg"),
        Some(&Value::F64(-2.0)),
        "cbrt(-8)"
    );
    assert!(
        happy.verdict.expect_passed(),
        "closed forms must pass, got {}",
        happy.verdict
    );

    let sqrt_neg = run_one(
        "cf-sqrt-neg",
        "\
emath function SqrtNeg:
    inputs:
        n: Float64
    outputs:
        y: Float64
        finite: Bool
    definitions:
        y = sqrt(-1 * n)
        finite = is_finite(y)
    tests:
        example <nan>:
            given n = 1.0
            expect finite == false
",
    );
    match sqrt_neg.outputs.get("y") {
        Some(Value::F64(v)) if v.is_nan() => {}
        other => panic!(
            "sqrt(-1) on Float64 must be IEEE NaN or a named refuse, got {other:?} verdict={}",
            sqrt_neg.verdict
        ),
    }
    assert_eq!(sqrt_neg.outputs.get("finite"), Some(&Value::Bool(false)));
    assert!(
        sqrt_neg.verdict.expect_passed(),
        "sqrt(-1) must not be a silent finite success, got {} outputs={:?}",
        sqrt_neg.verdict,
        sqrt_neg.outputs
    );

    let ln_neg = run_one(
        "cf-ln-neg",
        "\
emath function LnNeg:
    inputs:
        n: Float64
    outputs:
        y: Float64
        finite: Bool
    definitions:
        y = ln(-1 * n)
        finite = is_finite(y)
    tests:
        example <nan>:
            given n = 1.0
            expect finite == false
",
    );
    match ln_neg.outputs.get("y") {
        Some(Value::F64(v)) if v.is_nan() => {}
        other => panic!(
            "ln(-1) must be IEEE NaN or a named refuse, got {other:?} verdict={}",
            ln_neg.verdict
        ),
    }
    assert!(
        ln_neg.verdict.expect_passed(),
        "ln(-1) must not be a silent finite success, got {}",
        ln_neg.verdict
    );

    let log0 = run_one(
        "cf-log0",
        "\
emath function Log0:
    inputs:
        n: Float64
    outputs:
        y: Float64
        finite: Bool
    definitions:
        y = log(n)
        finite = is_finite(y)
    tests:
        example <ninf>:
            given n = 0.0
            expect finite == false
",
    );
    match log0.outputs.get("y") {
        Some(Value::F64(v)) if *v == f64::NEG_INFINITY => {}
        other => panic!(
            "log(0) must be IEEE -Inf or a named refuse, got {other:?} verdict={}",
            log0.verdict
        ),
    }
    assert!(
        log0.verdict.expect_passed(),
        "log(0) must not be a silent finite success, got {}",
        log0.verdict
    );

    let inv0 = run_one(
        "cf-modinv0",
        "\
emath function Inv0:
    inputs:
        n: Int
    outputs:
        y: Int
    definitions:
        y = mod_inv(n, 7)
    tests:
        example <noinv>:
            given n = 0
            expect y == 0
",
    );
    assert!(
        inv0.verdict.is_refused(),
        "mod_inv(0, 7) must named-refuse (gcd=7), got {} outputs={:?}",
        inv0.verdict,
        inv0.outputs
    );

    let inv_m0 = run_one(
        "cf-modinv-m0",
        "\
emath function InvM0:
    inputs:
        m: Int
    outputs:
        y: Int
    definitions:
        y = mod_inv(3, m)
    tests:
        example <badm>:
            given m = 0
            expect y == 0
",
    );
    assert!(
        inv_m0.verdict.is_refused(),
        "mod_inv(3, 0) must named-refuse (modulus not positive), got {} outputs={:?}",
        inv_m0.verdict,
        inv_m0.outputs
    );

    let mod0 = run_one(
        "cf-mod0",
        "\
emath function Mod0:
    inputs:
        d: Float64
    outputs:
        y: Float64
        finite: Bool
    definitions:
        y = mod(1, d)
        finite = is_finite(y)
    tests:
        example <nan>:
            given d = 0.0
            expect finite == false
",
    );
    match mod0.outputs.get("y") {
        Some(Value::F64(v)) if v.is_nan() => {}
        other => panic!(
            "mod(1, 0) must be IEEE NaN, got {other:?} verdict={}",
            mod0.verdict
        ),
    }

    let tan_pole = run_one(
        "cf-tan-half-pi",
        "\
emath function TanHalfPi:
    inputs:
        n: Float64
    outputs:
        y: Float64
        finite: Bool
    definitions:
        y = tan(n)
        finite = is_finite(y)
    tests:
        example <ieee>:
            given n = 1.5707963267948966
            expect finite == true
",
    );
    match tan_pole.outputs.get("y") {
        Some(Value::F64(v)) if v.is_finite() && v.abs() > 1e15 => {}
        other => panic!(
            "tan(π/2 as f64) must be IEEE huge-finite, got {other:?} verdict={}",
            tan_pole.verdict
        ),
    }
    assert!(
        tan_pole.verdict.expect_passed(),
        "tan(π/2) IEEE huge-finite must not refuse, got {}",
        tan_pole.verdict
    );

    let gf0 = run_one(
        "cf-gf0",
        "\
emath function Gf0:
    inputs:
        p: Int
    outputs:
        y: Int
    definitions:
        y = poly_eval_mod([1, 2], 3, p)
    tests:
        example <badp>:
            given p = 0
            expect y == 0
",
    );
    assert!(
        gf0.verdict.is_refused(),
        "poly_eval_mod p=0 must named-refuse, got {} outputs={:?}",
        gf0.verdict,
        gf0.outputs
    );

    let gf1 = run_one(
        "cf-gf1",
        "\
emath function Gf1:
    inputs:
        p: Int
    outputs:
        y: Int
        inv: Int
    definitions:
        y = poly_eval_mod([5], 3, p)
        inv = mod_inv(1, p)
    tests:
        example <zeroring>:
            given p = 1
            expect y == 0
            expect inv == 0
",
    );
    assert_eq!(gf1.outputs.get("y"), Some(&Value::I64(0)));
    assert_eq!(gf1.outputs.get("inv"), Some(&Value::I64(0)));
    assert!(
        gf1.verdict.expect_passed(),
        "GF(1) is the zero ring (gcd policy), got {} outputs={:?}",
        gf1.verdict,
        gf1.outputs
    );
}

/// Spec-oracle: `notation-ops.emath` package path is XID segments
/// (`tst.notation_ops`), not a hyphenated filename echo.
#[test]
fn notation_ops_intro_example_computes() {
    let source = include_str!("../../../tests/fixtures/language/intro/notation-ops.emath");
    let result = check_source("notation-ops", source);
    assert!(
        !result.diagnostics.has_errors(),
        "notation-ops.emath must admit, got: {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let tests = &report.declarations[0].tests;
    assert_eq!(tests.len(), 2);
    assert_eq!(
        tests[0].outputs.get("t"),
        Some(&Value::F64(0.125)),
        "√(4⊕3) inv must be 0.125, got {:?}",
        tests[0].outputs.get("t")
    );
    assert!(
        tests[0].verdict.expect_passed(),
        "pow_sqrt_recip must pass, got {}",
        tests[0].verdict
    );
    assert!(
        tests[1].verdict.expect_passed(),
        "alias_equals_glyph must pass, got {}",
        tests[1].verdict
    );
}

/// Surface involution: `f(f⁻¹(x)) == x` where defined. `i64::MIN` negate
/// is a named overflow, not a wrap that would make `-(-MIN) == MIN`.
#[test]
fn invertible_ops_surface_involutions() {
    let round = run_one(
        "inv-round",
        "\
emath function InvRound:
    inputs:
        k: Int
    outputs:
        n: Int
        r: Float64
        tt: Matrix[2, 3]
        m: Matrix[2, 3]
        a: Int
    definitions:
        n = -(-k)
        r = recip(recip(8))
        m = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
        tt = transpose(transpose(m))
        a = mod_inv(mod_inv(3, 7), 7)
    tests:
        example <round>:
            given k = 42
            expect n == 42
            expect r == 8
            expect tt == m
            expect a == 3
",
    );
    assert_eq!(round.outputs.get("n"), Some(&Value::I64(42)));
    assert_eq!(round.outputs.get("r"), Some(&Value::F64(8.0)));
    assert_eq!(round.outputs.get("tt"), round.outputs.get("m"));
    assert_eq!(round.outputs.get("a"), Some(&Value::I64(3)));
    assert!(
        round.verdict.expect_passed(),
        "surface involutions must hold, got {} outputs={:?}",
        round.verdict,
        round.outputs
    );

    let min_neg = run_one(
        "inv-imin",
        "\
emath function NegI64Min:
    inputs:
        k: Int
    outputs:
        y: Int
    definitions:
        x = -k - 1
        y = -x
    tests:
        example <overflow>:
            given k = 9223372036854775807
            expect y == 0
",
    );
    assert!(
        min_neg.verdict.is_refused(),
        "-I64::MIN must named-fault, not wrap, got {} outputs={:?}",
        min_neg.verdict,
        min_neg.outputs
    );
}

/// Binder scope: dummy indices do not leak, inner names shadow outer
/// names, empty ranges are identities, and `if` guards see the binder.
#[test]
fn binder_scope_empty_shadow_guard() {
    // Empty constant ranges: 0 for sum, 1 for product, true/false for
    // forall/exists. Vacuous forall must not evaluate `1/i`.
    let empty = run_one(
        "binder-empty-const",
        "\
emath function EmptyConst:
    inputs:
        n: Float64
    outputs:
        s: Float64
        p: Float64
        a: Bool
        e: Bool
        vacuous: Bool
    definitions:
        s = sum i in 0..0: i * n
        p = product i in 0..0: i
        a = forall i in 0..0: false
        e = exists i in 0..0: true
        vacuous = forall i in 0..0: 1 / i == 0
    tests:
        example <empty>:
            given n = 1.0
            expect s == 0
            expect p == 1
            expect a == true
            expect e == false
            expect vacuous == true
",
    );
    assert_eq!(empty.outputs.get("s"), Some(&Value::F64(0.0)), "empty sum");
    assert_eq!(
        empty.outputs.get("p"),
        Some(&Value::F64(1.0)),
        "empty product must be 1, not 0 (would mean i=0 was evaluated)"
    );
    assert_eq!(
        empty.outputs.get("a"),
        Some(&Value::Bool(true)),
        "empty forall"
    );
    assert_eq!(
        empty.outputs.get("e"),
        Some(&Value::Bool(false)),
        "empty exists"
    );
    assert_eq!(
        empty.outputs.get("vacuous"),
        Some(&Value::Bool(true)),
        "vacuous forall must not evaluate 1/i"
    );
    assert!(
        empty.verdict.expect_passed(),
        "empty constant binders must pass, got {} outputs={:?}",
        empty.verdict,
        empty.outputs
    );

    // Same identities through a runtime-empty `0..n`.
    let empty_n = run_one(
        "binder-empty-n",
        "\
emath function EmptyN:
    inputs:
        n: Float64
    outputs:
        s: Float64
        p: Float64
        a: Bool
        e: Bool
    definitions:
        s = sum i in 0..n: i
        p = product i in 0..n: i
        a = forall i in 0..n: false
        e = exists i in 0..n: true
    tests:
        example <n0>:
            given n = 0
            expect s == 0
            expect p == 1
            expect a == true
            expect e == false
",
    );
    assert_eq!(empty_n.outputs.get("s"), Some(&Value::F64(0.0)));
    assert_eq!(
        empty_n.outputs.get("p"),
        Some(&Value::F64(1.0)),
        "product over 0..0 must be 1, got {:?}",
        empty_n.outputs.get("p")
    );
    assert_eq!(empty_n.outputs.get("a"), Some(&Value::Bool(true)));
    assert_eq!(empty_n.outputs.get("e"), Some(&Value::Bool(false)));
    assert!(
        empty_n.verdict.expect_passed(),
        "empty 0..n binders must pass, got {} outputs={:?}",
        empty_n.verdict,
        empty_n.outputs
    );

    // Always-false product guard is identity 1, not 0.
    let filtered = run_one(
        "binder-empty-product-guard",
        "\
emath function EmptyProductGuard:
    inputs:
        n: Float64
    outputs:
        p: Float64
        s: Float64
    definitions:
        p = product i in 1..5 if i > 10: i
        s = sum i in 1..5 if i > 10: i * n
    tests:
        example <id>:
            given n = 1.0
            expect p == 1
            expect s == 0
",
    );
    assert_eq!(
        filtered.outputs.get("p"),
        Some(&Value::F64(1.0)),
        "filtered-empty product identity"
    );
    assert_eq!(
        filtered.outputs.get("s"),
        Some(&Value::F64(0.0)),
        "filtered-empty sum identity"
    );
    assert!(filtered.verdict.expect_passed());

    // Nested different names: inner captures outer i.
    let capture = run_one(
        "binder-capture",
        "\
emath function Capture:
    inputs:
        n: Float64
        m: Float64
    outputs:
        t: Float64
    definitions:
        t = sum i in 0..n: sum j in 0..m: i
    tests:
        example <cap>:
            given n = 3
            given m = 2
            expect t == 6
",
    );
    assert_eq!(
        capture.outputs.get("t"),
        Some(&Value::F64(6.0)),
        "sum_i sum_j i with n=3,m=2 is 0+0+1+1+2+2=6, got {:?}",
        capture.outputs.get("t")
    );
    assert!(capture.verdict.expect_passed());

    // Constant-range nested same name already unrolls via index_locals.
    let const_shadow = run_one(
        "binder-const-shadow",
        "\
emath function ConstShadow:
    inputs:
        n: Float64
    outputs:
        t: Float64
    definitions:
        t = sum i in 1..4: sum i in 10..12: i * n
    tests:
        example <cs>:
            given n = 1.0
            expect t == 63
",
    );
    assert_eq!(
        const_shadow.outputs.get("t"),
        Some(&Value::F64(63.0)),
        "inner 10+11=21, three outer iterations → 63, got {:?}",
        const_shadow.outputs.get("t")
    );
    assert!(const_shadow.verdict.expect_passed());

    // Dummy index must shadow a prior definition of the same name.
    let def_shadow = run_one(
        "binder-def-shadow",
        "\
emath function DefShadow:
    inputs:
        n: Float64
    outputs:
        t: Float64
        k: Float64
    definitions:
        k = 7
        t = sum k in 0..n: k
    tests:
        example <ds>:
            given n = 4
            expect t == 6
            expect k == 7
",
    );
    assert_eq!(
        def_shadow.outputs.get("t"),
        Some(&Value::F64(6.0)),
        "sum k in 0..4: k must be 6, not 7*4 (leaked def), got {:?}",
        def_shadow.outputs.get("t")
    );
    assert_eq!(def_shadow.outputs.get("k"), Some(&Value::F64(7.0)));
    assert!(
        def_shadow.verdict.expect_passed(),
        "def shadow must pass, got {} outputs={:?}",
        def_shadow.verdict,
        def_shadow.outputs
    );

    // Dummy index must shadow an input of the same name, including in `if`.
    let input_shadow = run_one(
        "binder-input-shadow",
        "\
emath function InputShadow:
    inputs:
        i: Float64
        n: Float64
    outputs:
        t: Float64
        g: Float64
        after: Float64
    definitions:
        t = sum i in 0..n: i
        g = sum i in 0..n if i > 2: i
        after = (sum i in 1..4: i) + i
    tests:
        example <is>:
            given i = 99
            given n = 5
            expect t == 10
            expect g == 7
            expect after == 105
",
    );
    assert_eq!(
        input_shadow.outputs.get("t"),
        Some(&Value::F64(10.0)),
        "sum i in 0..5: i must be 10, not 99*5, got {:?}",
        input_shadow.outputs.get("t")
    );
    assert_eq!(
        input_shadow.outputs.get("g"),
        Some(&Value::F64(7.0)),
        "filtered sum i>2 in 0..5 is 3+4=7, not input i, got {:?}",
        input_shadow.outputs.get("g")
    );
    assert_eq!(
        input_shadow.outputs.get("after"),
        Some(&Value::F64(105.0)),
        "(sum i in 1..4: i)+i must restore input i=99 → 6+99=105, got {:?}",
        input_shadow.outputs.get("after")
    );
    assert!(
        input_shadow.verdict.expect_passed(),
        "input shadow must pass, got {} outputs={:?}",
        input_shadow.verdict,
        input_shadow.outputs
    );

    // Nested variable-bound same name: inner i shadows outer i.
    let nest = run_one(
        "binder-nested-shadow",
        "\
emath function NestShadow:
    inputs:
        n: Float64
        m: Float64
    outputs:
        t: Float64
    definitions:
        t = sum i in 0..n: sum i in 0..m: i
    tests:
        example <ns>:
            given n = 3
            given m = 2
            expect t == 3
",
    );
    assert_eq!(
        nest.outputs.get("t"),
        Some(&Value::F64(3.0)),
        "inner sum 0+1=1, three outer → 3 (not outer-i replay), got {:?}",
        nest.outputs.get("t")
    );
    assert!(
        nest.verdict.expect_passed(),
        "nested shadow must pass, got {} outputs={:?}",
        nest.verdict,
        nest.outputs
    );

    // Constant-range outer + variable-bound inner, same dummy name.
    let mixed = run_one(
        "binder-mixed-shadow",
        "\
emath function MixedShadow:
    inputs:
        n: Float64
    outputs:
        t: Float64
    definitions:
        t = sum i in 1..4: sum i in 0..n: i
    tests:
        example <ms>:
            given n = 2
            expect t == 3
",
    );
    assert_eq!(
        mixed.outputs.get("t"),
        Some(&Value::F64(3.0)),
        "inner 0+1=1, three outer → 3 (not outer literals), got {:?}",
        mixed.outputs.get("t")
    );
    assert!(
        mixed.verdict.expect_passed(),
        "mixed shadow must pass, got {} outputs={:?}",
        mixed.verdict,
        mixed.outputs
    );

    // Binder variable must not leak into a later definition. Use `k`
    // rather than `i`: bare `i` is the imaginary unit (B14) when not
    // shadowed, which is not a binder leak.
    let leak = check_source(
        "binder-leak",
        "\
emath function Leak:
    outputs:
        s: Float64
        leaked: Float64
    definitions:
        s = sum k in 1..4: k
        leaked = k
",
    );
    let codes: Vec<&str> = leak.diagnostics.errors().map(|d| d.code).collect();
    assert!(
        codes.contains(&"E-TYPE-002"),
        "binder k must not leak after the binder, got {codes:?}"
    );
}

#[test]
fn complex_sqrt_ln_compute() {
    let result = check_source(
        "cplx-elem",
        "\
emath function CplxElem:
    inputs:
        n: Float64
    outputs:
        s: Complex
        l: Complex
        mag: Float64
    definitions:
        s = sqrt(-1 + 0i)
        l = ln(-1 + 0i)
        mag = abs(i) * n
    tests:
        example <principal>:
            given n = 1.0
            expect abs(s - i) < 1e-12
            expect abs(l - 3.141592653589793i) < 1e-12
            expect mag == 1
",
    );
    assert!(
        !result.diagnostics.has_errors(),
        "Complex sqrt/ln must admit, got {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    match test.outputs.get("s") {
        Some(Value::Complex { re, im }) => {
            assert!(
                re.abs() < 1e-12 && (im - 1.0).abs() < 1e-12,
                "sqrt(-1)={re}+{im}i"
            );
        }
        other => panic!("expected Complex sqrt, got {other:?}"),
    }
    match test.outputs.get("l") {
        Some(Value::Complex { re, im }) => {
            assert!(
                re.abs() < 1e-12 && (im - std::f64::consts::PI).abs() < 1e-12,
                "ln(-1)={re}+{im}i"
            );
        }
        other => panic!("expected Complex ln, got {other:?}"),
    }
    assert_eq!(test.outputs.get("mag"), Some(&Value::F64(1.0)));
    assert!(test.verdict.expect_passed(), "got {}", test.verdict);
}

#[test]
fn vectordot_derivative_computes() {
    let result = check_source(
        "dot-ad",
        "\
emath function DotDeriv:
    inputs:
        x: Float64
    outputs:
        d: Float64
    definitions:
        d = derivative(dot([x, 1.0], [1.0, x])) wrt x
    tests:
        example <two>:
            given x = 3.0
            expect d == 2.0
",
    );
    assert!(
        !result.diagnostics.has_errors(),
        "dot derivative must admit, got {:?}",
        result
            .diagnostics
            .errors()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    match test.outputs.get("d") {
        Some(Value::F64(v)) => assert!((v - 2.0).abs() < 1e-12, "d={v}"),
        other => panic!("expected F64, got {other:?}"),
    }
    assert!(test.verdict.expect_passed(), "got {}", test.verdict);
}
