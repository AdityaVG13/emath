//! Reference-evaluator boundary scan and drift-detection tests.
//!
//! Moved from #[cfg(test)] in crates/emath-adapter-dew/src/oracle.rs.

use emath_adapter_dew::dexpr::DewMatrix;
use emath_adapter_dew::{
    DewExpr, Layout, MutantDrift, ScanCase, ScanProfile, detect_drift, run_boundary_cases,
    scan_reference_boundaries,
};

fn scalar_expr() -> DewExpr {
    DewExpr::Add(
        Box::new(DewExpr::Var("x".into())),
        Box::new(DewExpr::Float64Bits(1.0f64.to_bits())),
    )
}

fn matrix_expr() -> DewExpr {
    DewExpr::Matrix(DewMatrix {
        rows: 1,
        cols: 1,
        data: vec![DewExpr::Float64Bits(1.0f64.to_bits())],
        layout: Layout::RowMajor,
    })
}

#[test]
fn scalar_boundary_scan_has_no_findings() {
    let findings = scan_reference_boundaries(&scalar_expr(), "x");
    assert!(
        findings.is_empty(),
        "scalar expression must scan clean over every boundary case, got {findings:?}"
    );
}

#[test]
fn non_scalar_node_reports_boundary_gaps_not_false_passes() {
    // A matrix node under the scalar evaluator is undefined at every
    // case; the scan reports each one instead of passing vacuously.
    let findings = scan_reference_boundaries(&matrix_expr(), "x");
    assert_eq!(findings.len(), ScanCase::all().len());
    assert!(
        findings
            .iter()
            .all(|finding| finding.detail.contains("undefined")),
        "every finding must name the undefined evaluator, got {findings:?}"
    );
}

#[test]
fn injected_drift_is_detected_over_the_reference() {
    // The negative fixture: a mutated `+` path (`AddAsSub`) must
    // diverge from the reference at some boundary case; a fixture
    // that never diverges would be a masked mutant.
    let finding = detect_drift(&scalar_expr(), "x", MutantDrift::AddAsSub)
        .expect("drift fixture must diverge at a boundary case");
    assert!(
        finding.reference_bits != finding.backend_bits,
        "drift detection must surface differing bits"
    );
}

#[test]
fn no_mutation_means_no_divergence_findings() {
    let findings = run_boundary_cases(&scalar_expr(), "x", &ScanProfile::default(), None);
    assert!(
        findings.is_empty(),
        "a clean scalar must not produce divergence findings"
    );
}
