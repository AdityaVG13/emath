//! Duplicate `example <name>:` blocks inside one function's `tests:`
//! section admit and both rows run: example names are row labels in the
//! constructor test lane (positional, not link keys), so a repeated
//! label is not a collision. The pre-cutover `E-NAME-022` check existed
//! because the recipe codegen emitted one Rust fn per
//! `<function>_<test name>` and duplicates collided there (rustc E0428);
//! that codegen is gone with the recipe lane. `E-NAME-022` still refuses
//! duplicate declaration names.

use emath_test_harness::{Probe, Source, boot};

const FUNCTION_BODY: &str = "emath function f:\n    inputs:\n        x: Int\n    outputs:\n        y: Int\n    definitions:\n        y = x + 1\n    tests:\n";

#[test]
fn duplicate_example_names() {
    boot();
    let mut p = Probe::new(
        "duplicate example names admit and both rows run; distinct names admit",
    );
    p.case("duplicate-admits-and-runs", |p| {
        Source::from_str(
            "dup-test-name",
            format!(
                "{FUNCTION_BODY}        example <eval>:\n            given x = 1\n            expect y == 2\n        example <eval>:\n            given x = 2\n            expect y == 3\n"
            ),
        )
        .eval_tests(p);
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
