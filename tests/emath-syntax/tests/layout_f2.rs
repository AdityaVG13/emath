//! F2 layout diagnostics + bracket idiom.
//!
//! The layout rule: NEWLINE fires at statement boundaries and is suppressed
//! inside `()[]{}`. The bracket idiom continues an expression across lines.
//! A bare hanging infix (`y = x +` then newline) is a typed refusal
//! (E-SYN-153) that teaches the idiom. C4: the hanging-sum idiom after a
//! binder `:` is NOT supported; rewrite with brackets.

use emath_cli::{CliExit, run_check};
use std::path::{Path, PathBuf};

fn install_parser() {
    // One per process; idempotent.
    emath_syntax::install_source_parser();
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("emath-layout-f2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(name);
    std::fs::write(&path, body).expect("write probe");
    path
}

fn cleanup(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir(parent);
    }
}

#[test]
fn bracket_idiom_continues_expression_across_lines() {
    install_parser();
    let path = scratch(
        "bracket.emath",
        "emath function T:\n    inputs:\n        x: Float64\n    definitions:\n        y = (x +\n        1.0)\n",
    );
    let (diagnostics, _, _) = run_check(&path);
    assert!(
        !diagnostics
            .items()
            .iter()
            .any(|item| item.severity == emath_core::Severity::Error),
        "bracket idiom must parse: {:?}",
        diagnostics.items()
    );
    cleanup(&path);
}

#[test]
fn hanging_infix_is_e_syn_153_with_bracket_help() {
    install_parser();
    let path = scratch(
        "hanging.emath",
        "emath function T:\n    inputs:\n        x: Float64\n    definitions:\n        y = x +\n        1.0\n",
    );
    let (diagnostics, _, _) = run_check(&path);
    let item = diagnostics
        .items()
        .iter()
        .find(|item| item.code == "E-SYN-153")
        .expect("E-SYN-153 for hanging infix");
    assert!(
        item.message.contains("bracket"),
        "diagnostic must teach the bracket idiom: {}",
        item.message
    );
    assert!(
        item.message.contains("NEWLINE"),
        "diagnostic must explain NEWLINE: {}",
        item.message
    );
    cleanup(&path);
}

#[test]
fn single_line_binders_still_work() {
    install_parser();
    let path = scratch(
        "single.emath",
        "emath function T:\n    inputs:\n        x: Float64\n    definitions:\n        y = x + 1.0\n",
    );
    let (diagnostics, _, _) = run_check(&path);
    assert!(
        !diagnostics
            .items()
            .iter()
            .any(|item| item.severity == emath_core::Severity::Error),
        "single-line binder regression: {:?}",
        diagnostics.items()
    );
    cleanup(&path);
}

#[test]
fn multi_line_argument_list_suppresses_newline() {
    install_parser();
    // Inside brackets NEWLINE is suppressed: the call parses as one flow.
    let path = scratch(
        "args.emath",
        "emath function T:\n    inputs:\n        x: Float64\n        y: Float64\n    definitions:\n        z = add2(\n            x,\n            y,\n        )\n",
    );
    let (diagnostics, _, _) = run_check(&path);
    // `add2` may be unknown at admission; the layout property under test is
    // that no layout/expectation error fires at the line breaks.
    assert!(
        !diagnostics
            .items()
            .iter()
            .any(|item| item.message.contains("end of line")),
        "NEWLINE must be suppressed inside brackets: {:?}",
        diagnostics.items()
    );
    cleanup(&path);
}

#[test]
fn refusal_exit_is_refused_for_hanging_infix() {
    install_parser();
    let path = scratch(
        "hanging2.emath",
        "emath function T:\n    inputs:\n        x: Float64\n    definitions:\n        y = x *\n        2.0\n",
    );
    let exit = run_check_public(&path);
    assert_eq!(exit, CliExit::Refused);
    cleanup(&path);
}

fn run_check_public(path: &Path) -> CliExit {
    let args = vec!["check".to_string(), path.display().to_string()];
    emath_cli::run(&args)
}
