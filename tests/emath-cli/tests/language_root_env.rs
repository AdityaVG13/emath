//! `EMATH_ROOT` language-root resolution contract: external consumers run
//! `emath` from their own project directories by pointing `EMATH_ROOT` at a
//! repository root containing `language/spec`.
//!
//! Failure-first: before the env seam existed, the env-set case failed —
//! the resolver only walked the source path and cwd ancestors, so a
//! foreign working directory refused with `E-LANG-IMAGE` even with the
//! variable set. The bad-root and unset cases pin the refusal semantics.

mod common;

use emath_cli::EXIT_OK;
use emath_test_harness::Probe;

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn scratch(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("emath-lang-root-{}-{}", tag, std::process::id()))
}

/// A consumer file that imports a LIBRARY module (`optimization.allocate`),
/// so a green run proves the Language Image itself resolved — not just the
/// parse lane.
fn consumer_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("consumer.emath");
    std::fs::write(
        &path,
        "use optimization.allocate\n\nemath function best_value:\n    inputs:\n        budget: Int\n    outputs:\n        result: Rat\n    definitions:\n        plan = allocate_best([[AllocOption: {cost: 2, value: 5/1}, AllocOption: {cost: 1, value: 3/1}]], budget)\n        result = if plan.ok: plan.value else: 0/1\n    tests:\n        example <consumer_picks_best>:\n            given budget = 2\n            expect result == 5/1\n",
    )
    .expect("write consumer file");
    path
}

fn run_with_env(
    cwd: &std::path::Path,
    root: Option<&std::path::Path>,
    source: &std::path::Path,
) -> (String, i32) {
    let mut command = std::process::Command::new(common::emath_bin());
    command.arg("test").arg(source).current_dir(cwd);
    // scrub the ambient environment: the resolution must not accidentally
    // succeed through this process's own variables
    command.env_clear();
    command.env("PATH", std::env::var("PATH").unwrap_or_default());
    if let Some(root) = root {
        command.env("EMATH_ROOT", root);
    }
    let output = command.output().expect("run emath test");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn probe() {
    let mut p = Probe::new("EMATH_ROOT resolves the language root for external consumers");

    p.case("env-root-runs-a-module-import-from-a-foreign-cwd", |p| {
        let dir = scratch("foreign");
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let source = consumer_file(&dir);
        let (text, code) = run_with_env(&dir, Some(&repo_root()), &source);
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("module actually ran", &text, "1 authored tests passed");
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.case("bad-env-root-is-a-named-refusal", |p| {
        let dir = scratch("badroot");
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let source = consumer_file(&dir);
        let (text, code) = run_with_env(&dir, Some(&dir), &source);
        p.demand(
            "must refuse",
            code != EXIT_OK as i32,
            format!("EMATH_ROOT without language/spec must refuse, got exit {code}: {text}"),
        );
        p.contains("E-LANG-IMAGE family", &text, "E-LANG-IMAGE");
        p.contains(
            "names the set root",
            &text,
            "EMATH_ROOT is set to",
        );
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.case("unset-env-keeps-the-ancestor-walk-refusal", |p| {
        let dir = scratch("unset");
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let source = consumer_file(&dir);
        let (text, code) = run_with_env(&dir, None, &source);
        p.demand(
            "must refuse",
            code != EXIT_OK as i32,
            format!("unset EMATH_ROOT from a foreign cwd must still refuse, got exit {code}: {text}"),
        );
        p.contains("E-LANG-IMAGE family", &text, "E-LANG-IMAGE");
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.finish();
}
