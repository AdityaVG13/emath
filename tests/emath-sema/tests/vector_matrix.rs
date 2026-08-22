//! Tests for Vector and Matrix types, literals, indexing, and arithmetic in semantic analysis.

use emath_core::limits::Limits;
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
