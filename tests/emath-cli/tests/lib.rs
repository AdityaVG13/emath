//! CLI ergonomics tests, moved from `crates/emath-cli/src/lib.rs`.

use emath_cli::{run, EXIT_OK, EXIT_USAGE};

fn args(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_string).collect()
}

#[test]
fn bare_and_help_exit_ok() {
    assert_eq!(run(&[]), EXIT_OK);
    assert_eq!(run(&args("help")), EXIT_OK);
    assert_eq!(run(&args("--help")), EXIT_OK);
    assert_eq!(run(&args("-h")), EXIT_OK);
}

#[test]
fn version_aliases_exit_ok() {
    assert_eq!(run(&args("version")), EXIT_OK);
    assert_eq!(run(&args("--version")), EXIT_OK);
    assert_eq!(run(&args("-V")), EXIT_OK);
}

#[test]
fn command_help_is_first_try() {
    assert_eq!(run(&args("check --help")), EXIT_OK);
    assert_eq!(run(&args("help check")), EXIT_OK);
    assert_eq!(run(&args("agent --help")), EXIT_OK);
}

#[test]
fn unknown_command_is_usage() {
    assert_eq!(run(&args("chek")), EXIT_USAGE);
    assert_eq!(run(&args("buld")), EXIT_USAGE);
    assert_eq!(run(&args("zzzzzzzz")), EXIT_USAGE);
}

#[test]
fn capabilities_and_robot_docs_exit_ok() {
    assert_eq!(run(&args("capabilities")), EXIT_OK);
    assert_eq!(run(&args("capabilities --json")), EXIT_OK);
    assert_eq!(run(&args("robot-docs")), EXIT_OK);
    assert_eq!(run(&args("robot-docs guide")), EXIT_OK);
    assert_eq!(run(&args("robot-docs waffle")), EXIT_USAGE);
}

#[test]
fn read_side_json_and_triage_help_exit_ok() {
    assert_eq!(run(&args("architecture --json")), EXIT_OK);
    assert_eq!(run(&args("doctor --json")), EXIT_OK);
    assert_eq!(run(&args("provider list --json")), EXIT_OK);
    assert_eq!(run(&args("agent triage --help")), EXIT_OK);
}

#[test]
fn unknown_flag_is_usage() {
    assert_eq!(run(&args("check --jason file.emath")), EXIT_USAGE);
    assert_eq!(run(&args("plan --jason file.emath")), EXIT_USAGE);
    assert_eq!(run(&args("run file.emath --verify")), EXIT_USAGE);
    assert_eq!(run(&args("run file.emath --json")), EXIT_USAGE);
    assert_eq!(run(&args("test file.emath --json")), EXIT_USAGE);
    assert_eq!(run(&args("new name --verify")), EXIT_USAGE);
    assert_eq!(run(&args("vendor --out d --verify")), EXIT_USAGE);
    assert_eq!(run(&args("capabilities --waffle")), EXIT_USAGE);
    assert_eq!(run(&args("simulate --help")), EXIT_OK);
    assert_eq!(run(&args("simulate")), EXIT_USAGE);
}

#[test]
fn catalog_commands_honor_help_and_unknown_flags() {
    assert_eq!(run(&args("version --help")), EXIT_OK);
    assert_eq!(run(&args("capabilities --help")), EXIT_OK);
    assert_eq!(run(&args("robot-docs --help")), EXIT_OK);
}
