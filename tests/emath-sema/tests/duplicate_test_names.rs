//! Duplicate `example <name>:` blocks inside one function's `tests:`
//! section must refuse typed (`E-NAME-022`) at admission.
//!
//! The generated test crate names each emitted test fn
//! `<function>_<test name>`, so two blocks resolving to the same name
//! collide in generated Rust (rustc E0428) — exactly the collision class
//! E-NAME-022 exists for ("two declarations with the same name would
//! collide in generated Rust, so the second is refused"). Before this
//! check the collision surfaced as a raw rustc error inside
//! `emath test` instead of a typed diagnostic at the .emath source.

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn session() -> CompilerSession {
    install_source_parser();
    CompilerSession::new(Limits::default())
}

fn error_codes(result: &emath_sema::CheckResult) -> Vec<String> {
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect()
}

const FUNCTION_BODY: &str = "emath function f:\n    inputs:\n        x: Int\n    outputs:\n        y: Int\n    definitions:\n        y = x + 1\n    tests:\n";

#[test]
fn duplicate_example_name_refuses() {
    // Two `example <eval>:` blocks in one function resolve to the same
    // generated test fn name; the second must refuse E-NAME-022.
    let source = format!(
        "{FUNCTION_BODY}        example <eval>:\n            given x = 1\n            expect y == 2\n        example <eval>:\n            given x = 2\n            expect y == 3\n"
    );
    let result = session().check_owned("dup-test-name", &source);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    assert!(
        codes.contains(&"E-NAME-022".to_string()),
        "duplicate example name must refuse E-NAME-022, got {codes:?}"
    );
}

#[test]
fn distinct_example_names_admit() {
    // Distinct example names in one function remain legal.
    let source = format!(
        "{FUNCTION_BODY}        example <eval>:\n            given x = 1\n            expect y == 2\n        example <eval_b>:\n            given x = 2\n            expect y == 3\n"
    );
    let result = session().check_owned("distinct-test-names", &source);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    assert!(
        codes.is_empty(),
        "distinct example names must admit, got {codes:?} (messages: {:?})",
        result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
    );
}
