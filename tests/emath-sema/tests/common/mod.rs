//! Shared helpers for the `dae_*` event/transition/disposition test
//! binaries (`dae_events.rs`, `dae_transitions.rs`,
//! `dae_disposition.rs`). One `check_source` / `error_text` pair instead
//! of a copy per file, so a change to the harness edits one place.

// Each test binary compiles this module; a helper one binary never calls
// (e.g. `error_text` in `dae_disposition`) is dead code THERE, not here.
#![allow(dead_code)]

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

/// Install the Language Image capsule distribution for this test thread
/// (rat_cells.rs pattern). Must run before `CompilerSession::new`.
pub fn install_language_seam() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
        .expect("load capsule distribution");
    emath_sema::language::install_language_distribution(&distribution)
        .expect("install capsule-active kernels");
}

/// Check a `.emath` source string through the real compiler session.
pub fn check_source(name: &str, source: &str) -> emath_sema::admit::CheckResult {
    // Capsule surface admission and kernel binding resolve only through
    // the installed language distribution; every test runs on its own
    // thread, so each check installs it for that thread (rat_cells pattern).
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution =
        emath_exec_ir::language_image::load_language_distribution(&root).expect("load capsule distribution");
    emath_sema::language::install_language_distribution(&distribution)
        .expect("install capsule-active kernels");
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    session.check_owned(name, source)
}

/// Join the error diagnostics into one string for `contains` asserts.
pub fn error_text(result: &emath_sema::admit::CheckResult) -> String {
    result
        .diagnostics
        .errors()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(" | ")
}
