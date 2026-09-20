//! `emath build` emission contract: one NAMED entry per runnable
//! function in the emitted crate, and no flag that promises a gate the
//! command never runs.
//!
//! Failure-first against the duplicate-entry defect: a file with two
//! runnable functions used to emit ONE lib.rs containing two
//! `pub fn entry` definitions - a crate that cannot compile. The
//! shared `entry` symbol is gone; each function emits under its own
//! name, which is also what sibling calls (pilot p8) need.
//! And `--verify` was accepted while feeding only an unreachable path.

mod common;
use emath_cli::{EXIT_OK, EXIT_USAGE};
use emath_test_harness::Probe;

fn build(path: &std::path::Path, args: &[&str]) -> (String, i32) {
    let output = std::process::Command::new(common::emath_bin())
        .arg("build")
        .arg(path)
        .args(args)
        .output()
        .expect("run emath build");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        output.status.code().unwrap_or(-1),
    )
}

fn scratch(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("emath-build-{}-{}", tag, std::process::id()))
}

fn entry_count(lib: &str) -> usize {
    // The embedded runtime (`pub mod emath_rt`) carries its own
    // `pub fn` lines; every emitted entry carries the
    // `// function `<name>`` marker, which the runtime never contains.
    lib.lines().filter(|line| line.starts_with("// function `")).count()
}

const SINGLE: &str = "emath function only_fn:
    inputs:
        a: Int
    outputs:
        result: Int
    definitions:
        result = a + 1
    tests:
        example <one>:
            given a = 2
            expect result == 3
";

const TWO_RUNNABLE: &str = "emath function first_fn:
    inputs:
        a: Int
    outputs:
        result: Int
    definitions:
        result = a + 1
    tests:
        example <one>:
            given a = 2
            expect result == 3
emath function second_fn:
    inputs:
        a: Int
    outputs:
        result: Int
    definitions:
        result = a + 2
    tests:
        example <two>:
            given a = 2
            expect result == 4
";

const ONE_RUNNABLE_ONE_CLOSURE: &str = "emath function plain_fn:
    inputs:
        a: Int
    outputs:
        result: Int
    definitions:
        result = a + 1
    tests:
        example <one>:
            given a = 2
            expect result == 3
emath function closure_fn:
    # A closure parameter emits natively (`&dyn Fn` carrier), so this
    # function emits as its own runnable entry - one named entry per
    # function, closure or not.
    inputs:
        g: Rat -> Rat
        x: Rat
    outputs:
        result: Rat
    definitions:
        result = g(x)
    tests:
        example <identity>:
            given g = function z in Rat: z
            given x = 1 / 2
            expect result == 1 / 2
";

#[test]
fn probe() {
    let mut p = Probe::new("build emits one named entry per runnable function and carries no dead flags");

    p.case("second-runnable-emits-its-own-name", |p| {
        let src = scratch("multi-src");
        let out = scratch("multi-out");
        std::fs::write(&src, TWO_RUNNABLE).expect("write source");
        let (text, code) = build(&src, &["--out", &out.to_string_lossy()]);
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("runnable", &text, "runnable");
        let lib = std::fs::read_to_string(out.join("src/lib.rs")).expect("emitted lib");
        p.contains(
            "first entry named",
            &lib,
            "pub fn first_fn(a: i64) -> Result<i64, String>",
        );
        p.contains(
            "second entry named",
            &lib,
            "pub fn second_fn(a: i64) -> Result<i64, String>",
        );
        p.demand(
            "no shared entry symbol",
            !lib.contains("pub fn entry"),
            "each runnable function emits under its own name, not a shared `entry`",
        );
    });

    p.case("single-function-named-entry", |p| {
        let src = scratch("single-src");
        let out = scratch("single-out");
        std::fs::write(&src, SINGLE).expect("write source");
        let (text, code) = build(&src, &["--out", &out.to_string_lossy()]);
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("runnable", &text, "runnable");
        let lib = std::fs::read_to_string(out.join("src/lib.rs")).expect("emitted lib");
        p.contains("entry named", &lib, "pub fn only_fn(a: i64) -> Result<i64, String>");
        p.demand(
            "one entry",
            entry_count(&lib) == 1,
            format!("a single runnable function emits exactly one entry, got {}", entry_count(&lib)),
        );
    });

    p.case("closure-param-emits-native-entry", |p| {
        let src = scratch("closure-src");
        let out = scratch("closure-out");
        std::fs::write(&src, ONE_RUNNABLE_ONE_CLOSURE).expect("write source");
        let (text, code) = build(&src, &["--out", &out.to_string_lossy()]);
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("closure sibling runnable", &text, "(runnable)");
        let lib = std::fs::read_to_string(out.join("src/lib.rs")).expect("emitted lib");
        p.contains(
            "closure entry typed",
            &lib,
            "pub fn closure_fn(g: std::rc::Rc<dyn Fn(emath_rt::ExactRatio) -> Result<emath_rt::ExactRatio, String>>, x: emath_rt::ExactRatio) -> Result<emath_rt::ExactRatio, String>",
        );
        p.demand(
            "one entry per function",
            entry_count(&lib) == 2,
            format!("each function emits exactly one entry, closure carriers included, got {}", entry_count(&lib)),
        );
    });

    p.case("verify-flag-is-not-a-flag", |p| {
        let src = scratch("verify-src");
        std::fs::write(&src, SINGLE).expect("write source");
        let (text, code) = build(&src, &["--verify"]);
        p.eq("exit", code, EXIT_USAGE as i32);
        p.contains("named refusal", &text, "E-CLI-UNKNOWN-FLAG");
        p.contains(
            "usage without the flag",
            &text,
            "[--out <dir>] [--dry-run] [--json]",
        );
    });

    p.case("bin-flag-is-not-a-flag", |p| {
        // With one runnable entry per crate there is nothing to select:
        // the flag echoed in dry-run and fed nothing in the live path.
        let src = scratch("bin-src");
        std::fs::write(&src, SINGLE).expect("write source");
        let (text, code) = build(&src, &["--bin", "only_fn"]);
        p.eq("exit", code, EXIT_USAGE as i32);
        p.contains("named refusal", &text, "E-CLI-UNKNOWN-FLAG");
        p.contains(
            "usage without the flag",
            &text,
            "[--out <dir>] [--dry-run] [--json]",
        );
    });

    p.finish();
}
