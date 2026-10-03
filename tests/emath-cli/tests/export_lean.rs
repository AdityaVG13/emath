//! `emath export lean` end-to-end on a small project folder: a chain
//! model, its closed form, a file importing both, and a file with one
//! declaration Lean cannot take (quoted code) plus one that depends on
//! it.
//!
//! Without `--check` the export only writes the Lean project for
//! review: every translatable declaration is written, the two refused
//! ones are named with their reason (exit 3), Lean never runs, and the
//! model's Lean text matches the reviewed golden
//! `tests/fixtures/export_lean/golden/model.lean`. With `--check`
//! Lean's kernel checks every authored example: the chain folder is
//! accepted, and the same folder with one wrong expected value is
//! rejected naming that example. The `--check` half needs `lake` on
//! PATH.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::emath_bin;
use emath_cli::{EXIT_OK, EXIT_PARTIAL, EXIT_REFUSED};
use emath_test_harness::Probe;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn emath(dir: &Path, args: &[&str]) -> (String, i32) {
    let out = Command::new(emath_bin())
        .args(args)
        .current_dir(dir)
        .env("EMATH_ROOT", workspace())
        .output()
        .expect("run emath");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (text, out.status.code().unwrap_or(-1))
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read fixture dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy fixture");
        }
    }
}

fn lake_available() -> bool {
    Command::new("lake").arg("--version").output().is_ok_and(|out| out.status.success())
}

#[test]
fn export_lean() {
    let mut probe = Probe::new("emath export lean: convert for review, then check with Lean");
    let dir = std::env::temp_dir().join(format!("emath_export_lean_{}", std::process::id()));
    let fixtures = workspace().join("tests/fixtures/export_lean");
    copy_tree(&fixtures.join("proj"), &dir.join("proj"));

    let (text, code) = emath(&dir, &["export", "lean", "proj"]);
    probe.demand("folder-partial", code == EXIT_PARTIAL as i32, format!("exit {code}\n{text}"));
    probe.demand(
        "refusals-named",
        text.contains("not translated: refused.emath `code`: `quote`")
            && text.contains("not translated: refused.emath `uses_code`: calls `code`"),
        text.clone(),
    );
    let lean = dir.join("proj/lean");
    for file in ["lakefile.toml", "lean-toolchain", "Emath/Prelude.lean", "Export/refused.lean", "Export/chain/both.lean"] {
        probe.demand(&format!("wrote-{file}"), lean.join(file).is_file(), text.clone());
    }
    let refused = std::fs::read_to_string(lean.join("Export/refused.lean")).unwrap_or_default();
    probe.demand(
        "translated-beside-refused",
        refused.contains("def square") && !refused.contains("def code") && !refused.contains("def uses_code"),
        refused,
    );
    probe.demand("lean-not-run", !lean.join(".lake").exists(), text.clone());
    let model = std::fs::read_to_string(lean.join("Export/chain/model.lean")).unwrap_or_default();
    let golden = std::fs::read_to_string(fixtures.join("golden/model.lean")).unwrap_or_default();
    probe.demand("model-matches-golden", model == golden, model);

    if lake_available() {
        let out = dir.join("checked");
        let out_arg = out.to_string_lossy().to_string();
        let (text, code) = emath(&dir, &["export", "lean", "proj/chain", "--out", &out_arg, "--check"]);
        probe.demand("chain-accepted", code == EXIT_OK as i32 && text.contains("lean: accepted"), format!("exit {code}\n{text}"));

        let found = dir.join("proj/chain/found.emath");
        let source = std::fs::read_to_string(&found).expect("read found");
        std::fs::write(&found, source.replace("expect result == 7 / 8", "expect result == 7 / 9")).expect("write wrong");
        let (text, code) = emath(&dir, &["export", "lean", "proj/chain", "--out", &out_arg, "--check"]);
        probe.demand(
            "wrong-value-rejected",
            code == EXIT_REFUSED as i32 && text.contains("lean: rejected") && text.contains("two_steps"),
            format!("exit {code}\n{text}"),
        );
    } else {
        eprintln!("export_lean: `lake` is not on PATH; the --check half did not run");
    }
    probe.finish();
}
