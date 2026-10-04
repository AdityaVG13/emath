//! Errors name the line and column they are about. `check`, `test`, and
//! `run --function` all point at the same spot for a type error inside
//! a definition, and `check` at the operator that ends a line too early.
//! Before, `check` printed a byte offset or `(0:0)`, `test` printed no
//! location, and `run --function` ran the file and faulted at runtime.

mod common;

use emath_cli::{EXIT_ADMISSION, EXIT_OK};
use emath_test_harness::Probe;

// `capped` is used on line 7, column 36, before its row defines it.
const UNBOUND: &str = "emath function clamp:
    inputs:
        x: Rat
    outputs:
        result: Rat
    definitions:
        result = if x < 0: 0 else: capped
        capped = x
";

// The operator on line 7 ends the line; the parser stops at column 21.
const CUT: &str = "emath function cut:
    inputs:
        n: Int
    outputs:
        result: Int
    definitions:
        result = n +
";

#[test]
fn diagnostics_name_line_and_column() {
    let mut p = Probe::new("errors name the line and column they are about");
    let dir = std::env::temp_dir().join(format!("emath-diag-location-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let unbound = dir.join("unbound.emath");
    let cut = dir.join("cut.emath");
    std::fs::write(&unbound, UNBOUND).expect("write");
    std::fs::write(&cut, CUT).expect("write");
    let unbound = unbound.to_string_lossy().to_string();
    let cut = cut.to_string_lossy().to_string();

    let (text, code) = common::cli(&["check", &unbound]);
    p.contains("check located", &text, "unbound.emath:7:36: E-TYPE-002");
    p.contains("check shows the line", &text, "result = if x < 0: 0 else: capped");
    p.eq("check exit", code, EXIT_ADMISSION as i32);

    let (text, _) = common::cli(&["check", &unbound, "--json"]);
    p.contains("json line", &text, "\"line\": 7");
    p.contains("json column", &text, "\"column\": 36");

    let (text, code) = common::cli(&["test", &unbound]);
    p.contains("test located", &text, "unbound.emath:7:36: E-TYPE-002");
    p.eq("test exit", code, EXIT_ADMISSION as i32);

    let (text, code) = common::cli(&["run", &unbound, "--function", "clamp", "--set", "x=1"]);
    p.contains("run located", &text, "unbound.emath:7:36: E-TYPE-002");
    p.eq("run exit", code, EXIT_ADMISSION as i32);

    let (text, code) = common::cli(&["check", &cut]);
    p.contains("syntax located", &text, "cut.emath:7:21: E-SYN-153");
    p.eq("syntax exit", code, EXIT_ADMISSION as i32);

    // Every command that reads a program refuses a broken one the same
    // way: located, exit 2. Before, `build` and `compare` went on with a
    // mistyped file, `keep` faulted at runtime, and parse errors exited 1.
    let lib = dir.join("lib.emath").to_string_lossy().to_string();
    let out = dir.join("crate").to_string_lossy().to_string();
    for (file, at) in [(unbound.as_str(), "unbound.emath:7:36: E-TYPE-002"), (cut.as_str(), "cut.emath:7:21: E-SYN-153")] {
        for args in [
            vec!["build", file, "--out", out.as_str()],
            vec!["compare", file, "--function", "clamp", "--set", "x=1"],
            vec!["keep", file, "--function", "clamp", "--set", "x=1", "--to", lib.as_str()],
            vec!["fmt", file],
        ] {
            let (text, code) = common::cli(&args);
            p.contains(&format!("{} located", args.join(" ")), &text, at);
            p.eq(&format!("{} exit", args.join(" ")), code, EXIT_ADMISSION as i32);
        }
    }

    // A fixed file still runs.
    std::fs::write(&unbound, UNBOUND.replace("        result = if x < 0: 0 else: capped\n        capped = x\n", "        capped = x\n        result = if x < 0: 0 else: capped\n")).expect("write");
    let (text, code) = common::cli(&["run", &unbound, "--function", "clamp", "--set", "x=1"]);
    p.eq("fixed runs", code, EXIT_OK as i32);
    p.contains("fixed value", &text, "1");
    p.finish();
}
