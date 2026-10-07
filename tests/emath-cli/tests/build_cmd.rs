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

#[test]
fn check_reports_executable_meaning_changes() {
    let path = scratch("meaning-check").with_extension("emath");
    let meaning = |source: &str| {
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new(common::emath_bin())
            .arg("check")
            .arg(&path)
            .arg("--json")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let json = String::from_utf8(output.stdout).unwrap();
        let field = json.split_once("\"meaning_id\":").expect("MeaningID field").1.trim_start();
        field.strip_prefix('"').expect("admitted source must have a MeaningID")
            .split_once('"').unwrap().0.to_string()
    };
    let base = meaning(SINGLE);
    assert_eq!(meaning(&format!("# presentation only\n{SINGLE}")), base);
    assert_eq!(meaning(&SINGLE.replace("a: Int", "x: Int").replace("a + 1", "x + 1").replace("given a =", "given x =")), base);
    assert_ne!(meaning(&SINGLE.replace("a + 1", "a + 2")), base, "check JSON must not report a signature-shell identity for changed mathematics");
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
fn editor_checks_unsaved_text_with_local_imports() {
    use emath_cli::lsp::{json::JsonValue, protocol::RpcMessage, server::ServerState};
    let dir = scratch("editor space");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("helper.emath"), format!("exports: only_fn\n{SINGLE}")).unwrap();
    let path = dir.join("input.emath");
    std::fs::write(&path, SINGLE.replacen("function only_fn", "function _", 1)).unwrap();
    let source = TWO_RUNNABLE.replacen("result = a + 1", "result = only_fn(a)", 1);
    let uri = format!("file://{}", path.display()).replace(' ', "%20");
    let mut params = emath_artifact::JsonWriter::object();
    let mut document = emath_artifact::JsonWriter::object();
    document.string("uri", &uri);
    document.string("text", &format!("use helper.{{only_fn}}\n{source}"));
    params.object_field("textDocument", document.finish().trim_end());
    let mut state = ServerState::new();
    let mut output = Vec::new();
    state.handle(&RpcMessage {
        id: None,
        method: "textDocument/didOpen".into(),
        params: JsonValue::parse(&params.finish()).unwrap(),
    }, &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("\"diagnostics\":[]"), "the unsaved root must resolve its local import without reading refused disk text: {output}");
    assert!(output.contains(&uri), "notifications must preserve the editor URI");
}

#[test]
fn syntax_helper_and_stdin_format_obey_semantic_admission() {
    use std::io::Write;
    emath_syntax::install_source_parser();
    let refused = SINGLE.replacen("function only_fn", "function _", 1);
    let result = emath_cli::admitted_tree(std::path::Path::new("input.emath"), &refused);
    assert!(
        result.as_ref().is_err_and(|error| error.code.starts_with("E-NAME")),
        "the public admission helper must reject reserved names"
    );
    let mut child = std::process::Command::new(common::emath_bin())
        .args(["fmt", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(refused.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(emath_cli::EXIT_ADMISSION as i32), "{output:?}");
}

#[test]
fn diff_fingerprints_the_checked_source_not_replacement_bytes() {
    use emath_core::parse::{SourceParser, register_source_parser};
    use emath_core::{Diagnostics, Edition, FileId, limits::Limits, tree::SyntaxTree};
    use std::sync::OnceLock;
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    struct EditDuringParse;
    impl SourceParser for EditDuringParse {
        fn parse(&self, text: &str, file: FileId, limits: &Limits, edition: Edition) -> (SyntaxTree, Diagnostics) {
            let parsed = emath_syntax::parse_with_edition(text, file, limits, edition);
            if text == SINGLE {
                std::fs::write(PATH.get().unwrap(), SINGLE.replace("a + 1", "a + 100")).unwrap();
            }
            parsed
        }
    }
    // The parser registry is process-wide; isolate this seam from the
    // other tests without modifying their parser.
    if std::env::var_os("EMATH_DIFF_SNAPSHOT_CHILD").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "diff_fingerprints_the_checked_source_not_replacement_bytes", "--nocapture"])
            .env("EMATH_DIFF_SNAPSHOT_CHILD", "1")
            .output().unwrap();
        assert!(output.status.success(), "{output:?}");
        return;
    }
    let a = scratch("diff-a.emath");
    let b = scratch("diff-b.emath");
    std::fs::write(&a, SINGLE).unwrap();
    std::fs::write(&b, SINGLE).unwrap();
    PATH.set(a.clone()).unwrap();
    register_source_parser(&EditDuringParse);
    assert_eq!(
        emath_cli::run(&["diff".into(), a.display().to_string(), b.display().to_string(), "--json".into()]),
        EXIT_OK,
        "both admitted snapshots are identical despite a later disk edit"
    );
}

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
            "pub fn closure_fn(g: std::rc::Rc<dyn Fn(&emath_rt::ExactRatio) -> Result<emath_rt::ExactRatio, String>>, x: emath_rt::ExactRatio) -> Result<emath_rt::ExactRatio, String>",
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
            "[--out <dir>] [--dry-run] [--worlds w1,w2,...] [--json]",
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
            "[--out <dir>] [--dry-run] [--worlds w1,w2,...] [--json]",
        );
    });

    p.finish();
}
