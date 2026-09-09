//! simulate_cmd tests migrated from the in-crate `#[cfg(test)]` module.
use emath_cli::simulate_cmd::simulate_error_json;
use emath_test_harness::{Case, Probe, check_all, expect_ok};

#[test]
fn simulate_error_envelope() {
    let mut p = Probe::new("simulate error JSON carries command, severity, and typed code");
    expect_ok(check_all(
        &[
            Case::new("no-model", "add-exact.emath has no `emath model` declaration", "error".to_string()),
            Case::new("pkg", "E-PKG-080: cannot read source file (missing.emath)", "E-PKG-080".to_string()),
        ],
        |text| {
            let body = simulate_error_json(text);
            let parsed = emath_artifact::parse_json_document(&body).expect("json");
            match parsed.field("diagnostics").expect("diagnostics") {
                emath_artifact::JsonValue::Arr(items) => items[0].string_field("code").expect("code"),
                other => panic!("diagnostics must be array, got {other:?}"),
            }
        },
    ));
    for (name, text, code, needle) in [
        ("no-model", "add-exact.emath has no `emath model` declaration", "error", "`emath model`"),
        ("pkg", "E-PKG-080: cannot read source file (missing.emath)", "E-PKG-080", "cannot read source file"),
    ] {
        p.case(name, |p| {
            let body = simulate_error_json(text);
            let parsed = emath_artifact::parse_json_document(&body).expect("json");
            p.eq("command", parsed.string_field("command").expect("command"), "simulate".to_string());
            p.demand(
                "not-admitted",
                matches!(parsed.field("admitted"), Ok(emath_artifact::JsonValue::Bool(false))),
                "admitted must be false",
            );
            let diags = match parsed.field("diagnostics").expect("diagnostics") {
                emath_artifact::JsonValue::Arr(items) => items,
                other => panic!("diagnostics must be array, got {other:?}"),
            };
            p.eq("len", diags.len(), 1);
            p.eq("code", diags[0].string_field("code").expect("code"), code.to_string());
            p.eq("severity", diags[0].string_field("severity").expect("severity"), "error".to_string());
            let message = diags[0].string_field("message").expect("message");
            p.contains("message", &message, needle);
        });
    }
    p.finish();
}
