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

/// Check a `.emath` source string through the real compiler session.
pub fn check_source(name: &str, source: &str) -> emath_sema::admit::CheckResult {
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
