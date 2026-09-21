//! A record field named like a reserved ordinary-method name (the leftover
//! recipe list: `total`, `sum`, `solve`, ...) is a FIELD ACCESS, not a
//! method call. Postfix `.field` on a non-path receiver (a call result, an
//! index, a literal) is parsed as `field(recv)`, which is indistinguishable
//! from a call, so both the admission lane and the runtime must try the
//! field projection BEFORE the leftover-name refusal.
//!
//! Failure-first: before the fix, `make_tally(h).total` refused at
//! admission with the vague E-TYPE-003 `method_unavailable` message even
//! though the receiver carries the field (the `probability.inference`
//! module hit this live and renamed its field to `denom` to dodge it), and
//! the unbound-call refusal message did not name the collision or the fix.

mod common;

use emath_cli::EXIT_OK;
use emath_test_harness::Probe;

fn scratch(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("emath-field-collision-{}-{}", tag, std::process::id()))
}

fn run_emath(verb: &str, source: &std::path::Path) -> (String, i32) {
    let mut command = std::process::Command::new(common::emath_bin());
    command.arg(verb).arg(source);
    // hermetic: the run must not depend on this process's variables
    command.env_clear();
    command.env("PATH", std::env::var("PATH").unwrap_or_default());
    let output = command.output().expect("run emath");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        output.status.code().unwrap_or(-1),
    )
}

/// A module reading `.total` off a NON-PATH receiver (a call result): the
/// postfix form the parser flattens into `total(make_tally(h))`.
fn tally_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("tally.emath");
    std::fs::write(
        &path,
        "emath object Tally:\n    representation:\n        hits: Int\n        total: Int\n\nemath function make_tally:\n    inputs:\n        h: Int\n    outputs:\n        result: Tally\n    definitions:\n        result = Tally: {hits: h, total: h * 2}\n\nemath function read_total:\n    inputs:\n        h: Int\n    outputs:\n        result: Int\n    definitions:\n        result = make_tally(h).total\n    tests:\n        example <reads_the_total_field>:\n            given h = 3\n            expect result == 6\n",
    )
    .expect("write tally file");
    path
}

/// A module that CALLS an unbound leftover name with a non-record argument:
/// this is a real unbound method call and must still refuse — but naming
/// the collision and the fix.
fn unbound_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("unbound.emath");
    std::fs::write(
        &path,
        "emath function wants_total:\n    inputs:\n        x: Int\n    outputs:\n        result: Int\n    definitions:\n        result = total(x)\n",
    )
    .expect("write unbound file");
    path
}

#[test]
fn probe() {
    let mut p = Probe::new("record fields named like ordinary-method names project");

    p.case("non-path-field-access-named-total-is-readable", |p| {
        let dir = scratch("tally");
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let source = tally_file(&dir);
        let (text, code) = run_emath("test", &source);
        p.eq("test exit", code, EXIT_OK as i32);
        p.contains("field access ran", &text, "1 authored tests passed");
        // the ADMISSION lane must admit it too: `emath check` is where the
        // pre-fix E-TYPE-003 refusal surfaced, so pinning only the runtime
        // lane leaves the admission site unverified (a mutation probe
        // survived against exactly that hole)
        let (check_text, check_code) = run_emath("check", &source);
        p.eq("check exit", check_code, EXIT_OK as i32);
        p.demand(
            "check must not refuse the field name",
            !check_text.contains("E-TYPE-003"),
            format!("admission refused the record field access: {check_text}"),
        );
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.case("unbound-recipe-call-refuses-naming-the-collision", |p| {
        let dir = scratch("unbound");
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let source = unbound_file(&dir);
        let (text, code) = run_emath("check", &source);
        p.demand(
            "must refuse",
            code != EXIT_OK as i32,
            format!("an unbound `total(x)` call must refuse, got exit {code}: {text}"),
        );
        p.contains("names the method lane", &text, "ordinary module method");
        p.contains(
            "names the field fix",
            &text,
            "rename the record field",
        );
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.case("runtime-refusal-message-matches-the-admission-lane", |p| {
        // the RUNTIME site carries the same message text: `emath run`
        // prints the engine fault directly, so a divergent message in
        // `engine_step/call.rs` cannot hide behind a green check lane
        let dir = scratch("runmsg");
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let source = unbound_file(&dir);
        let mut command = std::process::Command::new(common::emath_bin());
        command
            .arg("run")
            .arg(&source)
            .arg("--function")
            .arg("wants_total")
            .arg("--set")
            .arg("x=3");
        command.env_clear();
        command.env("PATH", std::env::var("PATH").unwrap_or_default());
        let output = command.output().expect("run emath run");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let code = output.status.code().unwrap_or(-1);
        p.demand(
            "must refuse",
            code != EXIT_OK as i32,
            format!("an unbound `total(x)` run must refuse, got exit {code}: {text}"),
        );
        p.contains("names the method lane", &text, "ordinary module method");
        p.contains("names the field fix", &text, "rename the record field");
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.finish();
}
