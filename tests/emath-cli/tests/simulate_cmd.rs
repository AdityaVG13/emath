//! simulate_cmd tests migrated from the in-crate `#[cfg(test)]` module.

use emath_cli::simulate_cmd::simulate_error_json;

#[test]
fn simulate_error_json_has_code_severity_message() {
    let body = simulate_error_json("hello-square.emath has no `emath model` declaration");
    let parsed = emath_artifact::parse_json_document(&body).expect("json");
    assert_eq!(parsed.string_field("command").expect("command"), "simulate");
    match parsed.field("admitted").expect("admitted") {
        emath_artifact::JsonValue::Bool(false) => {}
        other => panic!("admitted must be false, got {other:?}"),
    }
    let diags = match parsed.field("diagnostics").expect("diagnostics") {
        emath_artifact::JsonValue::Arr(items) => items,
        other => panic!("diagnostics must be array, got {other:?}"),
    };
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].string_field("code").expect("code"), "error");
    assert_eq!(
        diags[0].string_field("severity").expect("severity"),
        "error"
    );
    assert!(
        diags[0]
            .string_field("message")
            .expect("message")
            .contains("`emath model`"),
    );
}

#[test]
fn simulate_error_json_preserves_e_pkg_code() {
    let body = simulate_error_json("E-PKG-080: cannot read source file (missing.emath)");
    let parsed = emath_artifact::parse_json_document(&body).expect("json");
    let diags = match parsed.field("diagnostics").expect("diagnostics") {
        emath_artifact::JsonValue::Arr(items) => items,
        other => panic!("diagnostics must be array, got {other:?}"),
    };
    assert_eq!(diags[0].string_field("code").expect("code"), "E-PKG-080");
    assert_eq!(
        diags[0].string_field("severity").expect("severity"),
        "error"
    );
    assert!(
        diags[0]
            .string_field("message")
            .expect("message")
            .contains("cannot read source file")
    );
}
