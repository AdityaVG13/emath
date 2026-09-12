//! Constructor declaration kinds. Historical `kind` / `model` / `policy`
//! names refuse. The leftover schema-registry cases stay below as comments
//! in the file history; this file now pins the live kinds.

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

fn square() -> &'static str {
    "emath function Square:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n"
}

#[test]
fn declaration_kinds() {
    boot();
    let mut p = Probe::new("constructor kinds admit; model/policy/kind refuse");
    p.case("function-ok", |p| {
        let result = Source::from_str("function-ok", square()).check();
        p.demand(
            "admit",
            !result.diagnostics.has_errors(),
            error_text(&result),
        );
        p.eq(
            "name",
            result
                .package
                .declarations
                .first()
                .map(|declaration| declaration.name.leaf().to_string()),
            Some("Square".to_string()),
        );
    });
    p.case("kind-gone", |p| {
        let source = "emath kind Scoring:\n    extends model\n    schema:\n        require section inputs\n";
        let (_, parse_diags) = parse_str(source);
        p.demand("parse", !parse_diags.has_errors(), "kind still parses as a declaration");
        Source::from_str("kind-gone", source).must_refuse(p, &["E-KIND-GONE"]);
    });
    p.case("model-gone", |p| {
        Source::from_str("model-gone", "emath model Heat:\n    state:\n        x: Float64\n")
            .must_refuse(p, &["E-KIND-GONE"]);
    });
    p.case("policy-gone", |p| {
        Source::from_str("policy-gone", "emath policy P:\n    inputs:\n        x: Float64\n")
            .must_refuse(p, &["E-KIND-GONE"]);
    });
    p.case("goals-gone", |p| {
        Source::from_str(
            "goals-gone",
            "emath function Square:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n    goals:\n        evaluate <y>:\n            produce rust.library\n",
        )
        .must_refuse(p, &["E-SEC-101"]);
    });
    p.finish();
}
