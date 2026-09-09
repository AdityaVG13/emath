//! `emath explain --provenance` rendering.
use emath_cli::provenance_explanation;
use emath_test_harness::{Probe, boot};
#[test]
fn probe() {
    boot();
    let mut p = Probe::new("provenance explanation renders text and JSON DAG");
    let path = std::env::temp_dir().join(format!("emath-prov-{}", std::process::id()));
    std::fs::write(&path, "emath function Calibration:
    inputs:
        value: Float64
        missing_source: Float64
    definitions:
        result = value
    provenance:
        value:
            kind: \"Assumed\"
            reason: \"calibration fixture\"
        missing_source:
            kind: \"Unstated\"
").expect("fixture");
    let text = provenance_explanation(&path, false).expect("text");
    p.case("text", |p| { p.contains("assumed", &text, "Calibration.value -> Assumed(reason=calibration fixture)"); p.contains("unstated", &text, "Calibration.missing_source -> Unstated"); });
    let json = provenance_explanation(&path, true).expect("json");
    p.case("json", |p| { for needle in ["\"schema\": \"emath.provenance-explanation.v1\"", "\"binding\": \"Calibration.value\"", "\"kind\": \"Assumed\"", "\"kind\": \"Unstated\""] { p.contains(needle, &json, needle); } });
    p.finish();
}
