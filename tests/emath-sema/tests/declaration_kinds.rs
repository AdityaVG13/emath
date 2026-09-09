//! Spec-oracle: declaration kinds, sections, and admission
//! (`language/CAPABILITY.md` vs syntax + `emath-sema`).

use emath_syntax::parse_str;
use emath_test_harness::{Probe, Source, boot};

fn error_text(result: &emath_sema::admit::CheckResult) -> String {
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.to_string())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn admitted_names(result: &emath_sema::admit::CheckResult) -> Vec<String> {
    result
        .package
        .declarations
        .iter()
        .map(|declaration| declaration.name.leaf().to_string())
        .collect()
}

fn admitted_labels(result: &emath_sema::admit::CheckResult) -> Vec<String> {
    result
        .package
        .declarations
        .iter()
        .map(|declaration| declaration.kind_label.clone())
        .collect()
}

fn square_body() -> &'static str {
    "    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n"
}

#[test]
fn declaration_kinds() {
    boot();
    let mut p = Probe::new("declaration kinds admit their sections, refuse foreign ones by name");
    p.case("kind-ok", |p| {
        let source = "\
emath kind Scoring:
    extends model

    schema:
        require section inputs
        allow section state

    lower:
        model.inputs = section.inputs
";
        let (_, parse_diags) = parse_str(source);
        p.demand("kind-ok:parse", !parse_diags.has_errors(), "kind declaration must parse");
        let result = Source::from_str("kind-ok", source).check();
        let text = error_text(&result);
        p.demand("kind-ok:admit", text.is_empty(), format!("valid kind schema admits: {text}"));
        p.eq("kind-ok:names", admitted_names(&result), vec!["Scoring".to_string()]);
        p.eq("kind-ok:labels", admitted_labels(&result), vec!["kind".to_string()]);
    });
    p.case("kind-bad-section", |p| {
        let source = "emath kind Scoring:\n    inputs:\n        x: Float64\n";
        let (_, parse_diags) = parse_str(source);
        p.demand("kind-bad-section:parse", !parse_diags.has_errors(), "extra section still parses");
        let result = Source::from_str("kind-bad-section", source).check();
        let text = error_text(&result);
        p.contains("kind-bad-section:code", &text, "E-SYN-101");
        p.contains("kind-bad-section:names-section", &text, "inputs");
        p.demand(
            "kind-bad-section:no-admit",
            admitted_names(&result).is_empty(),
            "kind must not be admitted as a function",
        );
    });
    p.case("custom", |p| {
        let source = format!("emath custom Square:\n{body}", body = square_body());
        let (_, parse_diags) = parse_str(&source);
        p.demand("custom:parse", !parse_diags.has_errors(), "emath custom must parse");
        let result = Source::from_str("custom", &source).check();
        let text = error_text(&result);
        let as_function =
            admitted_labels(&result) == ["function".to_string()] && admitted_names(&result) == ["Square".to_string()];
        let refused = text.contains("E-KIND-") && admitted_names(&result).is_empty();
        p.demand(
            "custom:function-or-refusal",
            as_function || refused,
            format!(
                "treat as function or refuse with a named error, names={:?} labels={:?} text={text}",
                admitted_names(&result),
                admitted_labels(&result),
            ),
        );
        if refused {
            p.contains("custom:names-kind", &text, "`custom`");
        }
    });
    p.case("widget", |p| {
        let source = format!("emath widget W:\n{body}", body = square_body());
        let (_, parse_diags) = parse_str(&source);
        p.demand("widget:parse", !parse_diags.has_errors(), "other kinds must parse");
        let result = Source::from_str("widget", &source).check();
        let text = error_text(&result);
        p.contains("widget:code", &text, "E-KIND-");
        p.demand(
            "widget:no-admit",
            admitted_names(&result).is_empty(),
            "other kinds must not be admitted",
        );
    });
    p.case("hybrid-sections", |p| {
        let source = "\
emath function Hybrid:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
    transitions:
        dummy = 1
    events:
        dummy = 1
";
        let (_, parse_diags) = parse_str(source);
        p.demand("hybrid-sections:parse", !parse_diags.has_errors(), "transitions/events parse");
        let result = Source::from_str("hybrid-sections", source).check();
        let text = error_text(&result);
        p.contains("hybrid-sections:events", &text, "E-SYN-101");
        p.contains("hybrid-sections:transitions", &text, "E-TRANS-003");
    });
    p.case("invariant", |p| {
        let source = "\
emath function Bounded:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
    invariant:
        x >= 0
";
        let (_, parse_diags) = parse_str(source);
        let result = Source::from_str("invariant", source).check();
        let text = error_text(&result);
        let clean = !parse_diags.has_errors() && text.is_empty();
        p.demand("invariant:admit", clean, format!("invariant: admits: {text}"));
        p.eq("invariant:names", admitted_names(&result), vec!["Bounded".to_string()]);
    });
    p.case("invariants-plural", |p| {
        let source = "\
emath function Bounded:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
    invariants:
        x >= 0
";
        let (_, parse_diags) = parse_str(source);
        p.demand("invariants-plural:parse", !parse_diags.has_errors(), "plural parses as a section head");
        let result = Source::from_str("invariants-plural", source).check();
        let text = error_text(&result);
        p.contains("invariants-plural:code", &text, "E-SEC-101");
        p.contains("invariants-plural:names-section", &text, "invariants");
    });
    p.case("kind-schema-shape", |p| {
        let source = "emath kind Scoring:\n    schema:\n        x = 1\n";
        let (_, parse_diags) = parse_str(source);
        p.demand("kind-schema-shape:parse", !parse_diags.has_errors(), "kind schema parses");
        let result = Source::from_str("kind-schema-shape", source).check();
        let text = error_text(&result);
        p.contains("kind-schema-shape:code", &text, "E-SYN-101");
        p.contains("kind-schema-shape:names-section", &text, "schema");
        p.demand(
            "kind-schema-shape:no-admit",
            admitted_names(&result).is_empty(),
            "invalid kind schema must not run",
        );
    });
    p.finish();
}
