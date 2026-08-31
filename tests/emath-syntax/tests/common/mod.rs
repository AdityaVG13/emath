//! Shared end-to-end helper: run the built `emath` CLI binary directly
//! instead of spawning `cargo run` per call (see
//! `tests/emath-cli/tests/common/mod.rs` for the rationale: nested cargo
//! invocations race the build lock under load and can be killed
//! mid-run, surfacing as flaky exit-code-`None` failures).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

static EMATH_BIN: OnceLock<PathBuf> = OnceLock::new();

/// Absolute path to the built `emath` CLI binary for the active profile.
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
