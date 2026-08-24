//! Tests for Vector and Matrix types, literals, indexing, and arithmetic in semantic analysis.

use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::run_package;
use emath_sema::admit::CheckResult;
use emath_sema::CompilerSession;
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
    assert!(!result.diagnostics.has_errors(), "vector operations must admit, got: {:?}", result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>());
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
    assert!(!result.diagnostics.has_errors(), "matrix operations must admit, got: {:?}", result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>());
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
    assert!(!result.diagnostics.has_errors(), "linear algebra operations must admit, got: {:?}", result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>());
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
    assert!(result.diagnostics.has_errors(), "ragged matrix must be rejected");
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(codes.contains(&"E-SHAPE-005"), "expected E-SHAPE-005 for ragged matrix, got: {codes:?}");
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
    assert!(result.diagnostics.has_errors(), "dimension mismatch in vector add must be rejected");
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(codes.contains(&"E-SHAPE-005"), "expected E-SHAPE-005, got: {codes:?}");
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
    assert!(result.diagnostics.has_errors(), "matrix-vector inner dimension mismatch must be rejected");
    let codes: Vec<&str> = result.diagnostics.errors().map(|d| d.code).collect();
    assert!(codes.contains(&"E-SHAPE-002"), "expected E-SHAPE-002, got: {codes:?}");
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
    assert!(result.diagnostics.has_errors(), "vector [i, j] must be rejected");
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
    );
}

#[test]
fn finite_sum_one_to_five_admits() {
    let source = include_str!("../../../language/examples/intro/sum-one-to-five.emath");
    let result = check_source("sum-one-to-five", source);
    assert!(
        !result.diagnostics.has_errors(),
        "finite sum must admit, got: {:?}",
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
    outputs:
        s: Float64
        p: Float64
        face: Matrix[2, 2]
    definitions:
        s = sum([1, 2, 3, 4, 5])
        p = product([[1, 2], [3, 4]])
        face = [[1.0, 2.0], [3.0, 4.0]]
    tests:
        example <known>:
            expect s == 15 and p == 24 and face == [[1.0, 2.0], [3.0, 4.0]]
";
    let result = check_source("fold", source);
    assert!(
        !result.diagnostics.has_errors(),
        "known-shape folds must admit, got: {:?}",
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let area = match test.outputs.get("area") {
        Some(Value::F64(v)) => *v,
        _ => panic!("expected f64 output for area, got {:?}", test.outputs.get("area")),
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
    outputs:
        area: Float64
    definitions:
        area = integral x in 0..3: x * x
    tests:
        example <quadratic>:
";
    let result = check_source("integral-xsquared", source);
    assert!(
        !result.diagnostics.has_errors(),
        "integral of x*x must admit, got: {:?}",
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let area = match test.outputs.get("area") {
        Some(Value::F64(v)) => *v,
        _ => panic!("expected f64 output for area, got {:?}", test.outputs.get("area")),
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        (root_val - 2.0).abs() < 0.001,
        "solve(x^2-4) wrt x from x=1 should converge to 2, got {root_val}"
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let opt = match test.outputs.get("optimum") {
        Some(Value::F64(v)) => *v,
        other => panic!("optimum should be F64, got {other:?}"),
    };
    assert!(
        (opt - 3.0).abs() < 0.1,
        "minimize((x-3)^2) wrt x from x=0 should converge to 3, got {opt}"
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let opt = match test.outputs.get("optimum") {
        Some(Value::F64(v)) => *v,
        other => panic!("optimum should be F64, got {other:?}"),
    };
    assert!(
        (opt - 2.0).abs() < 0.1,
        "maximize(-(x-2)^2) wrt x from x=0 should converge to 2, got {opt}"
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
        (opt_x - 1.0).abs() < 0.1,
        "minimize((x-1)^2 + (y-2)^2) wrt x,y from (0,0) should converge x to 1, got {opt_x}"
    );
    assert!(
        (opt_y - 2.0).abs() < 0.1,
        "minimize((x-1)^2 + (y-2)^2) wrt y,x from (0,0) should converge y to 2, got {opt_y}"
    );
}

#[test]
fn exact_integer_product_fold() {
    let source = "\
emath function ExactFactorial:
    outputs:
        fac: Int
    definitions:
        fac = product i in 1..=10: i
    tests:
        example <ten>:
            expect fac == 3628800
";
    let result = check_source("exact_factorial", source);
    assert!(
        !result.diagnostics.has_errors(),
        "exact integer product must admit, got: {:?}",
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    assert_eq!(
        test.outputs.get("fac"),
        Some(&Value::I64(3628800)),
        "10! should be exact i64 3628800, got {:?}",
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
        opt_y = y
    tests:
        example <demo>:
            given x = 0
            given y = 0
            expect abs(opt_x - 0.5) < 0.2
";
    let result = check_source("constrained_min", source);
    assert!(
        !result.diagnostics.has_errors(),
        "constrained optimization must admit, got: {:?}",
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    let test = &report.declarations[0].tests[0];
    let opt_x = match test.outputs.get("opt_x") {
        Some(Value::F64(v)) => *v,
        Some(Value::I64(v)) => *v as f64,
        other => panic!("opt_x should be numeric, got {other:?}"),
    };
    assert!(
        (opt_x - 0.5).abs() < 0.2,
        "constrained minimum should be near x=0.5 (constraint x+y>=1), got {opt_x}"
    );
    assert!(test.verdict.expect_passed());
}

#[test]
fn heat_rod_laplacian_admits_and_inline_tests_pass() {
    let source = include_str!("../../../language/examples/numerical/heat-rod.emath");
    let result = check_source("heat-rod", source);
    assert!(
        !result.diagnostics.has_errors(),
        "heat-rod must admit, got: {:?}",
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
    );
    let report = run_package(&result.package);
    // constant_holds: the laplacian of a constant field is zero everywhere
    // (clamped edges replicate the boundary), so one Euler step leaves u fixed.
    let t0 = &report.declarations[0].tests[0];
    assert_eq!(
        t0.outputs.get("next"),
        Some(&Value::Vector(vec![5.0; 5]))
    );
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
    let source = include_str!("../../../language/examples/numerical/heat-plate.emath");
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
    let source = include_str!("../../../language/examples/numerical/gradient-field.emath");
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
    // interior / 0.5 clamped edges; a zero matrix has zero gradients.
    let t0 = &report.declarations[0].tests[0];
    assert_eq!(
        t0.outputs.get("du"),
        Some(&Value::Vector(vec![0.5, 1.0, 1.0, 1.0, 0.5]))
    );
    assert!(t0.verdict.expect_passed());
    // ramp_matrix_has_axis_gradients: du/dc of a column-ramp is 1 interior
    // / 0.5 clamped; du/dr of a column-ramp is zero (no row variation).
    let t1 = &report.declarations[0].tests[1];
    assert_eq!(
        t1.outputs.get("gx"),
        Some(&Value::Matrix {
            rows: 3,
            cols: 3,
            data: vec![0.5, 1.0, 0.5, 0.5, 1.0, 0.5, 0.5, 1.0, 0.5]
        })
    );
    assert!(t1.verdict.expect_passed());
}

// ---- B02: filtered binder tests ---------------------------------------

#[test]
fn b02_filtered_sum_computes() {
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
fn b02_always_false_filter_gives_identity() {
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
fn b02_filtered_forall_computes() {
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
        result.diagnostics.errors().map(|d| d.to_string()).collect::<Vec<_>>()
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
