//! `emath build` emission contract: exactly one runnable entry per
//! emitted crate, and no flag that promises a gate the command never
//! runs.
//!
//! Failure-first against the duplicate-entry defect: a file with two
//! runnable functions used to emit ONE lib.rs containing two
//! `pub fn entry` definitions — a crate that cannot compile. And
//! `--verify` was accepted while feeding only an unreachable path.

mod common;
use emath_cli::{EXIT_ADMISSION, EXIT_OK, EXIT_USAGE};
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
    lib.matches("pub fn entry").count()
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
    # A closure call does not lower to flat scalar arithmetic, so this
    # function is emitted as a not-runnable comment, NOT a second
    # entry — the build must stay green with exactly one entry.
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
    let mut p = Probe::new("build emits exactly one runnable entry per crate and carries no dead flags");

    p.case("second-runnable-refuses-by-name", |p| {
        let src = scratch("multi-src");
        let out = scratch("multi-out");
        std::fs::write(&src, TWO_RUNNABLE).expect("write source");
        let (text, code) = build(&src, &["--out", &out.to_string_lossy()]);
        p.eq("exit", code, EXIT_ADMISSION as i32);
        p.contains("named code", &text, "E-CODEGEN-013");
        p.demand(
            "no partial crate",
            !out.join("src/lib.rs").is_file(),
            "a refused build must not write the crate",
        );
    });

    p.case("single-function-one-entry", |p| {
        let src = scratch("single-src");
        let out = scratch("single-out");
        std::fs::write(&src, SINGLE).expect("write source");
        let (text, code) = build(&src, &["--out", &out.to_string_lossy()]);
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("runnable", &text, "runnable");
        let lib = std::fs::read_to_string(out.join("src/lib.rs")).expect("emitted lib");
        p.demand(
            "one entry",
            entry_count(&lib) == 1,
            format!("a single runnable function emits exactly one entry, got {}", entry_count(&lib)),
        );
    });

    p.case("closure-sibling-still-one-entry", |p| {
        let src = scratch("closure-src");
        let out = scratch("closure-out");
        std::fs::write(&src, ONE_RUNNABLE_ONE_CLOSURE).expect("write source");
        let (text, code) = build(&src, &["--out", &out.to_string_lossy()]);
        p.eq("exit", code, EXIT_OK as i32);
        p.contains(
            "sibling marked not runnable",
            &text,
            "not marked runnable",
        );
        let lib = std::fs::read_to_string(out.join("src/lib.rs")).expect("emitted lib");
        p.demand(
            "one entry with closure sibling",
            entry_count(&lib) == 1,
            format!("a not-runnable sibling emits a comment, never a second entry, got {}", entry_count(&lib)),
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
