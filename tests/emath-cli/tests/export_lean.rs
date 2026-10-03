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
//! rejected naming that example. `mixed.emath` joins Int and Rat where
//! Lean needs one type (an `if`, a list, a cons, a fold's start), and
//! `remainder.emath` calls `int_rem` beside its own `int_rem` and
//! `exact.modular`'s guarded one, which emath never runs for that call;
//! the whole folder must still check. The `--check` half needs `lake`
//! on PATH.
//!
//! Laws: with `law/certify.emath` beside the chain, its four
//! `certify_equal` calls and one `certify_all` call become theorems.
//! The chain against its closed form at one point, the running sum
//! against n(n - 1)/2, the powers of 1/2 summed from t = 1, and the
//! chain against its closed form at every
//! parameter point the closed form computes at are proven; a quotient
//! by a sequence of n is reported unproven (exit 3), not rejected. A
//! closed form at the wrong point, or with its parameters swapped, is
//! rejected with where the sides differ (exit 1). Exported again
//! without the laws into the same folder, the chain checks without
//! Mathlib. This half also needs prebuilt Mathlib.

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

/// The shared Mathlib packages `emath export lean` uses, when already
/// prebuilt: the test must not start a multi-gigabyte download.
fn prebuilt_mathlib() -> Option<PathBuf> {
    let dir = std::env::var_os("EMATH_LEAN_MATHLIB")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache/emath/lean-mathlib")))?;
    dir.join("mathlib/.lake/build/lib/lean/Mathlib/LinearAlgebra/Charpoly/Basic.olean")
        .is_file()
        .then_some(dir)
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
    for file in ["lakefile.lean", "lean-toolchain", "Emath/Prelude.lean", "Export/refused.lean", "Export/chain/both.lean"] {
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
        // `mixed.emath` joins Int and Rat in an `if`, a list, a cons and
        // a fold's start, as emath does; Lean must accept the same file.
        let (text, code) = emath(&dir, &["export", "lean", "proj", "--check"]);
        probe.demand(
            "folder-accepted-beside-refusals",
            code == EXIT_PARTIAL as i32 && text.contains("lean: accepted"),
            format!("exit {code}\n{text}"),
        );

        let out = dir.join("checked");
        let out_arg = out.to_string_lossy().to_string();
        let (text, code) = emath(&dir, &["export", "lean", "proj/chain", "--out", &out_arg, "--check"]);
        probe.demand("chain-accepted", code == EXIT_OK as i32 && text.contains("lean: accepted"), format!("exit {code}\n{text}"));

        let certify = dir.join("proj/chain/certify.emath");
        std::fs::copy(fixtures.join("law/certify.emath"), &certify).expect("copy certify");
        // Exported for review, then built by hand where Mathlib is not
        // prebuilt: Lake must refuse before it clones or compiles Mathlib.
        let bare = dir.join("bare");
        let missing = dir.join("no-mathlib");
        let exported = Command::new(emath_bin())
            .args(["export", "lean", "proj/chain", "--out", &bare.to_string_lossy()])
            .current_dir(&dir)
            .env("EMATH_ROOT", workspace())
            .env("EMATH_LEAN_MATHLIB", &missing)
            .status()
            .expect("run emath");
        // Git may not reach the network here, so a missing guard fails
        // this check rather than download and compile Mathlib.
        let built = Command::new("lake")
            .arg("build")
            .current_dir(&bare)
            .env("GIT_ALLOW_PROTOCOL", "file")
            .output()
            .expect("run lake");
        let text = format!("{exported}\n{}{}", String::from_utf8_lossy(&built.stdout), String::from_utf8_lossy(&built.stderr));
        probe.demand(
            "bare-lake-refuses-unbuilt-mathlib",
            exported.success() && !built.status.success() && text.contains("Mathlib is not prebuilt") && !missing.exists(),
            text,
        );

        if let Some(mathlib) = prebuilt_mathlib() {
            let laws = dir.join("laws");
            let laws_arg = laws.to_string_lossy().to_string();
            let check = || {
                let out = Command::new(emath_bin())
                    .args(["export", "lean", "proj/chain", "--out", &laws_arg, "--check"])
                    .current_dir(&dir)
                    .env("EMATH_ROOT", workspace())
                    .env("EMATH_LEAN_MATHLIB", &mathlib)
                    .output()
                    .expect("run emath");
                let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                (text, out.status.code().unwrap_or(-1))
            };
            let (text, code) = check();
            probe.demand(
                "law-proven-beside-unproven",
                code == EXIT_PARTIAL as i32
                    && text.contains("lean: not proven")
                    && text.contains("unproven: certify.emath law 3 of `certified`: Lean found no order bound")
                    && !text.contains("law 1 of")
                    && !text.contains("law 2 of")
                    && !text.contains("law 4 of")
                    && !text.contains("not translated")
                    && !text.contains("rejected:"),
                format!("exit {code}\n{text}"),
            );
            let laws_lean = std::fs::read_to_string(laws.join("Export/certify.lean")).unwrap_or_default();
            probe.demand(
                "all-law-hypothesis-is-the-divisor",
                laws_lean.contains("(h₁ : 1 - (a - b) ≠ 0)\n    : ∀ n₀ : ℕ,"),
                laws_lean,
            );
            let source = std::fs::read_to_string(&certify).expect("read certify");
            let wrong = source
                .replace("mean_closed(1 / 2, 1 / 4, n)", "mean_closed(1 / 2, 1 / 5, n)")
                .replace("mean_closed(a, b, n)", "mean_closed(b, a, n)");
            std::fs::write(&certify, wrong).expect("write wrong laws");
            let (text, code) = check();
            probe.demand(
                "wrong-laws-rejected",
                code == EXIT_REFUSED as i32
                    && text.contains("rejected: certify.emath law 1 of `certified`: the two sides differ at n = ")
                    && text.contains(
                        "rejected: certify.emath law 1 of `certified_all`: the two sides differ at a parameter point the hypotheses allow",
                    ),
                format!("exit {code}\n{text}"),
            );
            // Without its laws the folder needs no Mathlib, even where
            // the last export left `Law.lean` and the laws' Lean file.
            std::fs::remove_file(&certify).expect("remove certify");
            let (text, code) = emath(&dir, &["export", "lean", "proj/chain", "--out", &laws_arg, "--check"]);
            probe.demand(
                "stale-laws-not-built",
                code == EXIT_OK as i32 && text.contains("lean: accepted"),
                format!("exit {code}\n{text}"),
            );
        } else {
            eprintln!("export_lean: no prebuilt Mathlib (EMATH_LEAN_MATHLIB or ~/.cache/emath/lean-mathlib); the law half did not run");
        }

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
