//! `emath explain --provenance` rendering.

use emath_cli::provenance_explanation;
use emath_syntax::install_source_parser;

#[test]
fn provenance_explanation_renders_text_and_json_dag() {
    install_source_parser();
    let path = std::env::temp_dir().join(format!(
        "emath-provenance-explain-{}.emath",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "\
emath function Calibration:
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
",
    )
    .expect("write provenance fixture");

    let text = provenance_explanation(&path, false).expect("text explanation");
    assert!(text.contains("Calibration.value -> Assumed(reason=calibration fixture)"));
    assert!(text.contains("Calibration.missing_source -> Unstated"));

    let json = provenance_explanation(&path, true).expect("JSON explanation");
    assert!(json.contains("\"schema\": \"emath.provenance-explanation.v1\""));
    assert!(json.contains("\"binding\": \"Calibration.value\""));
    assert!(json.contains("\"kind\": \"Assumed\""));
    assert!(json.contains("\"kind\": \"Unstated\""));
}
