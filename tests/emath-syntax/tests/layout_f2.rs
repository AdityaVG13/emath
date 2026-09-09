//! F2 layout diagnostics + bracket idiom.
//!
//! The layout rule: NEWLINE fires at statement boundaries and is suppressed
//! inside `()[]{}`. The bracket idiom continues an expression across lines.
//! A bare hanging infix (`y = x +` then newline) is a typed refusal
//! (E-SYN-153) that teaches the idiom. C4: the hanging-sum idiom after a
//! binder `:` is NOT supported; rewrite with brackets.

use emath_cli::{CliExit, run_check};
use std::path::{Path, PathBuf};

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

fn run_check_public(path: &Path) -> CliExit {
    let args = vec!["check".to_string(), path.display().to_string()];
    emath_cli::run(&args)
}

use emath_test_harness::{Probe, boot};

#[test]
fn layout_f2() {
    boot();
    let mut probe = Probe::new("F2 layout diagnostics + bracket idiom. The layout rule: NEWLINE fires at statement boundaries and is suppressed inside `()[]{}`. The bracket idiom");
    probe.case("bracket_idiom_continues_expression_across_lines", |p| {
    let f0 = p.failures().len();

    let path = scratch(
        "bracket.emath",
        "emath function T:\n    inputs:\n        x: Float64\n    definitions:\n        y = (x +\n        1.0)\n",
    );
    let (diagnostics, _, _) = run_check(&path);
    p.demand("1",!diagnostics
            .items()
            .iter()
            .any(|item| item.severity == emath_core::Severity::Error), format!(
        "bracket idiom must parse: {:?}",
        diagnostics.items()
    ));
    if p.failures().len() != f0 { return; }
    cleanup(&path);

    });
    probe.case("hanging_infix_is_e_syn_153_with_bracket_help", |p| {
    let f0 = p.failures().len();

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
    p.demand("1",item.message.contains("bracket"), format!(
        "diagnostic must teach the bracket idiom: {}",
        item.message
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",item.message.contains("NEWLINE"), format!(
        "diagnostic must explain NEWLINE: {}",
        item.message
    ));
    if p.failures().len() != f0 { return; }
    cleanup(&path);

    });
    probe.case("single_line_binders_still_work", |p| {
    let f0 = p.failures().len();

    let path = scratch(
        "single.emath",
        "emath function T:\n    inputs:\n        x: Float64\n    definitions:\n        y = x + 1.0\n",
    );
    let (diagnostics, _, _) = run_check(&path);
    p.demand("1",!diagnostics
            .items()
            .iter()
            .any(|item| item.severity == emath_core::Severity::Error), format!(
        "single-line binder regression: {:?}",
        diagnostics.items()
    ));
    if p.failures().len() != f0 { return; }
    cleanup(&path);

    });
    probe.case("multi_line_argument_list_suppresses_newline", |p| {
    let f0 = p.failures().len();

    // Inside brackets NEWLINE is suppressed: the call parses as one flow.
    let path = scratch(
        "args.emath",
        "emath function T:\n    inputs:\n        x: Float64\n        y: Float64\n    definitions:\n        z = add2(\n            x,\n            y,\n        )\n",
    );
    let (diagnostics, _, _) = run_check(&path);
    // `add2` may be unknown at admission; the layout property under test is
    // that no layout/expectation error fires at the line breaks.
    p.demand("1",!diagnostics
            .items()
            .iter()
            .any(|item| item.message.contains("end of line")), format!(
        "NEWLINE must be suppressed inside brackets: {:?}",
        diagnostics.items()
    ));
    if p.failures().len() != f0 { return; }
    cleanup(&path);

    });
    probe.case("refusal_exit_is_refused_for_hanging_infix", |p| {

    let path = scratch(
        "hanging2.emath",
        "emath function T:\n    inputs:\n        x: Float64\n    definitions:\n        y = x *\n        2.0\n",
    );
    let exit = run_check_public(&path);
    p.eq("1", exit, CliExit::Refused);
    cleanup(&path);

    });
    probe.finish();
}
