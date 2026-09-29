//! `emath keep` end-to-end: rediscover sequences with the authored
//! synthesizer, keep the programs into a user library, and reuse the
//! library from a second file. The written library must match the
//! golden `tests/fixtures/keep/seq.golden.emath`; re-keeping identical
//! content is a no-op; different content under a kept name refuses
//! unless `--overwrite`, and an overwrite really replaces the
//! definition (the reuse file's tests then fail).

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::emath_bin;
use emath_cli::{EXIT_OK, EXIT_REFUSED};
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

fn keep(dir: &Path, function: &str, name: &str, extra: &[&str]) -> (String, i32) {
    let mut args = vec![
        "keep", "discover.emath", "--function", function, "--name", name, "--to", "lib/seq.emath",
        "--set", "unused=0", "--work", "20000000",
    ];
    args.extend_from_slice(extra);
    emath(dir, &args)
}

#[test]
fn keep_cmd() {
    let mut probe = Probe::new("emath keep: rediscover, keep, reuse");
    let dir = std::env::temp_dir().join(format!("emath_keep_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let fixtures = workspace().join("tests/fixtures/keep");
    for file in ["discover.emath", "uses.emath"] {
        std::fs::copy(fixtures.join(file), dir.join(file)).expect("copy fixture");
    }
    let library = dir.join("lib/seq.emath");

    for (function, name) in [("Triangular", "triangular"), ("Factorial", "factorial"), ("Fibonacci", "fibonacci")] {
        let (text, code) = keep(&dir, function, name, &[]);
        probe.demand(&format!("keep-{name}"), code == EXIT_OK as i32 && text.contains(&format!("kept {name}")), format!("exit {code}\n{text}"));
    }
    let written = std::fs::read_to_string(&library).unwrap_or_default();
    let golden = std::fs::read_to_string(fixtures.join("seq.golden.emath")).expect("golden");
    probe.demand("library-matches-golden", written == golden, written.clone());

    let (text, code) = emath(&dir, &["test", "uses.emath"]);
    probe.demand("reuse-passes", code == EXIT_OK as i32 && text.contains("2 authored tests passed"), format!("exit {code}\n{text}"));

    let (text, code) = keep(&dir, "Factorial", "factorial", &[]);
    probe.demand("same-content-unchanged", code == EXIT_OK as i32 && text.contains("unchanged factorial"), format!("exit {code}\n{text}"));

    let (text, code) = keep(&dir, "Fibonacci", "factorial", &[]);
    let after = std::fs::read_to_string(&library).unwrap_or_default();
    probe.demand(
        "different-content-refuses",
        code == EXIT_REFUSED as i32 && text.contains("E-KEEP-EXISTS") && after == golden,
        format!("exit {code}\n{text}"),
    );

    let (text, code) = keep(&dir, "Fibonacci", "factorial", &["--overwrite"]);
    probe.demand("overwrite-replaces", code == EXIT_OK as i32, format!("exit {code}\n{text}"));
    let (text, code) = emath(&dir, &["test", "uses.emath"]);
    probe.demand("reuse-sees-overwrite", code != EXIT_OK as i32, format!("exit {code}\n{text}"));

    let _ = std::fs::remove_dir_all(&dir);
    probe.finish();
}
