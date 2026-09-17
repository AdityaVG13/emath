//! `tests:` row honesty: unknown row forms refuse instead of silently
//! vanishing, and a zero-input constructor no longer needs a dummy
//! input (bead emath-7zplf). Failure-first: before the fix an invented
//! `fault <label>:` row admitted on both lanes and ran as
//! "0 authored tests passed", and an empty `inputs:` section refused
//! E-SYN-112 until a dummy `unused: Int` row was added.

mod common;
use emath_cli::{EXIT_ADMISSION, EXIT_OK};
use emath_test_harness::Probe;

const FAULT_ROW: &str = "emath function Walk:
    inputs:
        bound: Int
    outputs:
        result: Int
    definitions:
        result = bound * 2
    tests:
        fault <heavy>:
            given bound = 3
            expect result == 6
";

const EMPTY_INPUTS: &str = "emath function Constant:
    inputs:
    outputs:
        result: Int
    definitions:
        result = 6
    tests:
        example <six>:
            expect result == 6
";

fn scratch(tag: &str, text: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("emath-row-honesty-{tag}-{}.emath", std::process::id()));
    std::fs::write(&path, text).expect("write source");
    path
}

#[test]
fn probe() {
    let mut p = Probe::new("unknown test rows refuse; empty inputs admit");

    p.case("fault-row-refuses-on-test-lane", |p| {
        let src = scratch("fault-row", FAULT_ROW);
        let (text, code) = common::cli(&["test", &src.to_string_lossy()]);
        p.eq("exit", code, EXIT_ADMISSION as i32);
        p.contains("named refusal", &text, "unknown_test_row");
    });

    p.case("fault-row-refuses-on-check-lane", |p| {
        let src = scratch("fault-row", FAULT_ROW);
        let (text, code) = common::cli(&["check", &src.to_string_lossy()]);
        p.eq("exit", code, EXIT_ADMISSION as i32);
        // The check lane surfaces constructor faults through the
        // `constructor_admit_code` table: unknown row forms map to the
        // default admission family with the full message.
        p.contains("mapped admission code", &text, "E-TYPE-012");
        p.contains("row refused by name", &text, "row in `tests:` is not a test form (section `fault`)");
    });

    p.case("empty-inputs-admit", |p| {
        let src = scratch("empty-inputs", EMPTY_INPUTS);
        let (text, code) = common::cli(&["test", &src.to_string_lossy()]);
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("ran the authored row", &text, "1 authored test");
    });

    p.finish();
}
