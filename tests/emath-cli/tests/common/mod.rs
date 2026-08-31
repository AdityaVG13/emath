//! Shared end-to-end helper: run the built `emath` CLI binary directly.
//!
//! The end-to-end probes previously spawned `cargo run -q -p emath-cli`
//! per call. Under a loaded machine those nested cargo invocations race
//! the build lock and the child can be killed mid-run (exit code `None`),
//! which surfaced as one random failure per full-suite run. Resolving the
//! binary once and execing it directly removes the nested-cargo failure
//! mode: `cargo test` has already built everything the binary needs
//! before any test runs, and the binary path is stable for the process.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

static EMATH_BIN: OnceLock<PathBuf> = OnceLock::new();

/// Absolute path to the built `emath` CLI binary for the active profile.
/// Builds it once per process if `cargo test` has not already produced
/// it (concurrent builders serialize on cargo's own build lock, and
/// later callers find the binary and skip the build entirely).
pub fn emath_bin() -> &'static Path {
    EMATH_BIN.get_or_init(|| {
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
            candidates.push(PathBuf::from(dir).join(profile).join("emath"));
        }
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        candidates.push(workspace.join("target").join(profile).join("emath"));
        for candidate in &candidates {
            if candidate.is_file() {
                return candidate.clone();
            }
        }
        let status = Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "emath-cli"])
            .current_dir(&workspace)
            .status()
            .expect("run cargo build -p emath-cli");
        assert!(status.success(), "cargo build -p emath-cli must succeed");
        candidates
            .into_iter()
            .find(|candidate| candidate.is_file())
            .unwrap_or_else(|| panic!("emath binary missing after build"))
    })
}

/// Run the CLI binary with `args`, returning (stdout+stderr, exit code).
/// Diagnostics print on stderr (output-style rule); assertions match the
/// combined stream so the exact E-* code is assertable either way.
#[allow(dead_code)] // not every test target in this package uses both helpers
pub fn cli(args: &[&str]) -> (String, i32) {
    let output = Command::new(emath_bin())
        .args(args)
        .output()
        .expect("run emath binary");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    (text, output.status.code().unwrap_or(-1))
}
