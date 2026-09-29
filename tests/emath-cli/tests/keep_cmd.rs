//! `emath keep` end-to-end: rediscover sequences with the authored
//! synthesizer, keep the programs into a user library, reuse the
//! library from a second file, and keep a second-tier find that the
//! search built from a kept function (`fibonacci(n + n)`). The written
//! library must match the golden `tests/fixtures/keep/lib/seq.emath`; re-keeping identical
//! content is a no-op; different content under a kept name refuses
//! unless `--overwrite`, and an overwrite really replaces the
//! definition (the reuse file's tests then fail). Last, the automated
//! loop: `search.synthesis.learn` returns every rule it learned, and
//! `keep` without `--name` writes them all (golden
//! `tests/fixtures/keep/lib/learned.emath`), round-2 rules calling
//! round-1 names.

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
    keep_from(dir, "discover.emath", function, name, extra)
}

fn keep_from(dir: &Path, file: &str, function: &str, name: &str, extra: &[&str]) -> (String, i32) {
    let mut args = vec![
        "keep", file, "--function", function, "--name", name, "--to", "lib/seq.emath",
        "--set", "unused=0", "--work", "50000000",
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
    for file in ["discover.emath", "uses.emath", "grows.emath", "learn.emath", "explore.emath"] {
        std::fs::copy(fixtures.join(file), dir.join(file)).expect("copy fixture");
    }
    let library = dir.join("lib/seq.emath");

    for (function, name) in [("Triangular", "triangular"), ("Factorial", "factorial"), ("Fibonacci", "fibonacci")] {
        let (text, code) = keep(&dir, function, name, &[]);
        probe.demand(&format!("keep-{name}"), code == EXIT_OK as i32 && text.contains(&format!("kept {name}")), format!("exit {code}\n{text}"));
    }
    let (text, code) = emath(&dir, &["test", "uses.emath"]);
    probe.demand("reuse-passes", code == EXIT_OK as i32 && text.contains("2 authored tests passed"), format!("exit {code}\n{text}"));

    let (text, code) = keep_from(&dir, "grows.emath", "FibonacciEven", "fibonacci_even", &[]);
    probe.demand("keep-built-from-kept", code == EXIT_OK as i32 && text.contains("kept fibonacci_even"), format!("exit {code}\n{text}"));
    let written = std::fs::read_to_string(&library).unwrap_or_default();
    let golden = std::fs::read_to_string(fixtures.join("lib/seq.emath")).expect("golden");
    probe.demand("library-matches-golden", written == golden, written.clone());

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

    // No --name: every record the curriculum learned keeps under its own
    // name, in learning order, so round-2 rules call round-1 names.
    let learn = [
        "keep", "learn.emath", "--function", "Curriculum", "--to", "lib/learned.emath",
        "--set", "unused=0", "--work", "50000000",
    ];
    let (text, code) = emath(&dir, &learn);
    let names = ["triangular", "fibonacci", "sum_of_cubes", "fibonacci_even"];
    probe.demand(
        "keep-all-learned",
        code == EXIT_OK as i32 && names.iter().all(|n| text.contains(&format!("kept {n} "))),
        format!("exit {code}\n{text}"),
    );
    let written = std::fs::read_to_string(dir.join("lib/learned.emath")).unwrap_or_default();
    let golden = std::fs::read_to_string(fixtures.join("lib/learned.emath")).expect("learned golden");
    probe.demand("learned-matches-golden", written == golden, written.clone());
    let (text, code) = emath(&dir, &learn);
    probe.demand(
        "relearn-unchanged",
        code == EXIT_OK as i32 && names.iter().all(|n| text.contains(&format!("unchanged {n} "))),
        format!("exit {code}\n{text}"),
    );

    // Self-proposed targets keep under their joined names (transform_rule).
    let (text, code) = emath(
        &dir,
        &["keep", "explore.emath", "--function", "Explored", "--to", "lib/explored.emath", "--set", "unused=0", "--work", "50000000"],
    );
    probe.demand("keep-explored", code == EXIT_OK as i32, format!("exit {code}\n{text}"));
    let written = std::fs::read_to_string(dir.join("lib/explored.emath")).unwrap_or_default();
    let golden = std::fs::read_to_string(fixtures.join("lib/explored.emath")).expect("explored golden");
    probe.demand("explored-matches-golden", written == golden, written.clone());

    let _ = std::fs::remove_dir_all(&dir);
    probe.finish();
}
