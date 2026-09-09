//! Reference-evaluator boundary scan and drift-detection tests.
use emath_adapter_dew::dexpr::DewMatrix;
use emath_adapter_dew::{DewExpr, Layout, MutantDrift, ScanCase, ScanProfile, detect_drift, detect_seeded_wrong_result, run_boundary_cases, scan_reference_boundaries};
use emath_test_harness::Probe;

fn scalar_expr() -> DewExpr {
    DewExpr::Add(Box::new(DewExpr::Var("x".into())), Box::new(DewExpr::Float64Bits(1.0f64.to_bits())))
}
fn matrix_expr() -> DewExpr {
    DewExpr::Matrix(DewMatrix { rows: 1, cols: 1, data: vec![DewExpr::Float64Bits(1.0f64.to_bits())], layout: Layout::RowMajor })
}

#[test]
fn probe() {
    let mut p = Probe::new("dew reference oracle scans clean, reports gaps, and catches drift");
    p.demand("scalar-clean", scan_reference_boundaries(&scalar_expr(), "x").is_empty(), "scalar must scan clean");
    p.case("matrix-gaps", |p| {
        let findings = scan_reference_boundaries(&matrix_expr(), "x");
        p.eq("count", findings.len(), ScanCase::all().len());
        p.demand("undefined", findings.iter().all(|f| f.detail.contains("undefined")), format!("every finding must name undefined: {findings:?}"));
    });
    p.case("drift", |p| {
        match detect_drift(&scalar_expr(), "x", MutantDrift::AddAsSub) {
            None => {
                p.fail("drift", "AddAsSub fixture must diverge");
            }
            Some(f) => {
                p.ne("drift-bits", f.reference_bits, f.backend_bits);
            }
        }
    });
    p.demand("no-mutation-clean", run_boundary_cases(&scalar_expr(), "x", &ScanProfile::default(), None).is_empty(), "clean scalar must not diverge");
    p.case("seeded-wrong", |p| {
        match detect_seeded_wrong_result(&scalar_expr(), "x", 0.0) {
            None => {
                p.fail("seeded", "planted wrong derivative must be reported");
            }
            Some(f) => {
                p.ne("seeded-bits", f.reference_bits, f.backend_bits);
                p.contains("seeded-detail", &f.detail, "seeded wrong result");
            }
        }
    });
    p.finish();
}
