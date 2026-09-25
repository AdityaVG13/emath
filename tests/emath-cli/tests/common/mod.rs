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

static EMATH_LAB_BIN: OnceLock<PathBuf> = OnceLock::new();

/// Absolute path to the built `emath-lab` binary (extracted commands).
pub fn emath_lab_bin() -> &'static Path {
    EMATH_LAB_BIN.get_or_init(|| {
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
            candidates.push(PathBuf::from(dir).join(profile).join("emath-lab"));
        }
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        candidates.push(workspace.join("target").join(profile).join("emath-lab"));
        for candidate in &candidates {
            if candidate.is_file() {
                return candidate.clone();
            }
        }
        let status = Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "emath-cli-lab"])
            .current_dir(&workspace)
            .status()
            .expect("run cargo build -p emath-cli-lab");
        assert!(
            status.success(),
            "cargo build -p emath-cli-lab must succeed"
        );
        candidates
            .into_iter()
            .find(|candidate| candidate.is_file())
            .unwrap_or_else(|| panic!("emath-lab binary missing after build"))
    })
}

/// Stable emitted-crate workspace for one authored module, shared by the
/// emission census and the conformance lanes: `<temp>/emath_emit/<key>`,
/// keyed by the module's repo-relative path (`/` -> `__`). The dir is
/// reused across runs, so its `target/` keeps cargo's fingerprints and
/// incremental state: `emath build --out` rewrites only changed bytes
/// (mtimes survive identical output), and a re-run over unchanged emission
/// is a cargo no-op instead of a cold compile of the embedded runtime.
/// Outside the repository by design (an emitted crate inside the workspace
/// tree would be claimed by the root manifest). Nothing is deleted.
#[allow(dead_code)]
pub fn emitted_crate_dir(module: &Path) -> PathBuf {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let repo = repo.canonicalize().unwrap_or(repo);
    let module = module
        .canonicalize()
        .unwrap_or_else(|_| module.to_path_buf());
    let relative = module.strip_prefix(&repo).unwrap_or(&module);
    let key = relative
        .with_extension("")
        .iter()
        .map(|part| {
            part.to_string_lossy()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("__");
    let dir = std::env::temp_dir().join("emath_emit").join(key);
    std::fs::create_dir_all(&dir).expect("create emitted crate dir");
    dir
}

/// Exclusive advisory lock over one emitted-crate dir (released on drop):
/// two concurrent runs over the same module serialize their emit, write,
/// and build instead of racing a half-written `lib.rs`.
#[allow(dead_code)]
pub fn lock_emitted_crate(dir: &Path) -> std::fs::File {
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.with_extension("lock"))
        .expect("open emitted crate lock");
    lock.lock().expect("lock emitted crate dir");
    lock
}

/// Write `contents` only when the bytes differ, so an unchanged file keeps
/// its mtime and cargo does not recompile it.
#[allow(dead_code)]
pub fn write_if_changed(path: &Path, contents: &str) {
    if std::fs::read(path).is_ok_and(|old| old == contents.as_bytes()) {
        return;
    }
    std::fs::write(path, contents).expect("write generated file");
}

/// Offline `cargo build` of an emitted crate into its own persistent
/// target dir. Debuginfo is off: the artifacts are driven, never debugged,
/// and debuginfo is a measured ~15% of a cold emitted-crate build.
#[allow(dead_code)]
pub fn cargo_build_emitted(dir: &Path, target: &[&str]) -> std::process::Output {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    Command::new(cargo)
        .args(["build", "--offline", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .args(target)
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env("CARGO_PROFILE_DEV_DEBUG", "0")
        .output()
        .expect("run cargo build for emitted crate")
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
