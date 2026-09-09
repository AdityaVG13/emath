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

use emath_test_harness::{Probe, Source, boot};

const FUNCTION_BODY: &str = "emath function f:\n    inputs:\n        x: Int\n    outputs:\n        y: Int\n    definitions:\n        y = x + 1\n    tests:\n";

#[test]
fn duplicate_example_names() {
    boot();
    let mut p = Probe::new("duplicate example names refuse E-NAME-022, distinct names admit");
    p.case("duplicate-refuses", |p| {
        Source::from_str(
            "dup-test-name",
            format!(
                "{FUNCTION_BODY}        example <eval>:\n            given x = 1\n            expect y == 2\n        example <eval>:\n            given x = 2\n            expect y == 3\n"
            ),
        )
        .must_refuse(p, &["E-NAME-022"]);
    });
    p.case("distinct-admits", |p| {
        Source::from_str(
            "distinct-test-names",
            format!(
                "{FUNCTION_BODY}        example <eval>:\n            given x = 1\n            expect y == 2\n        example <eval_b>:\n            given x = 2\n            expect y == 3\n"
            ),
        )
        .eval_tests(p);
    });
    p.finish();
}
