//! Catalog behavioral tests, moved from `crates/emath-cli/src/catalog.rs`.
//!
//! `jason_suggests_json` was dropped: it asserts the private
//! `catalog::suggest_flag`, which the public API does not expose.

use emath_cli::catalog::{
    COMMANDS, capabilities_json, command_help_text, command_summary, command_usage, flags_for,
    reject_unknown_flags, robot_docs_guide, suggest_command,
};

#[test]
fn typo_chek_suggests_check() {
    assert_eq!(suggest_command("chek"), Some("check"));
}

#[test]
fn typo_buld_suggests_build() {
    assert_eq!(suggest_command("buld"), Some("build"));
}

#[test]
fn typo_verson_suggests_version() {
    assert_eq!(suggest_command("verson"), Some("version"));
}

#[test]
fn nonsense_has_no_suggestion() {
    assert_eq!(suggest_command("zzzzzzzz"), None);
}

#[test]
fn every_catalogued_command_has_usage_and_summary() {
    for command in COMMANDS {
        assert!(command_usage(command).is_some(), "{command} usage");
        assert!(command_summary(command).is_some(), "{command} summary");
        assert!(command_help_text(command).is_some(), "{command} help");
    }
}

#[test]
fn capabilities_json_names_schema_and_exit_codes() {
    let body = capabilities_json();
    assert!(
        body.contains("\"schema\": \"emath.capabilities\""),
        "{body}"
    );
    assert!(body.contains("\"name\": \"check\""), "{body}");
    assert!(body.contains("\"name\": \"simulate\""), "{body}");
    assert!(body.contains("\"name\": \"capabilities\""), "{body}");
    assert!(body.contains("\"0\": \"ok\""), "{body}");
}

#[test]
fn robot_docs_names_capabilities() {
    let body = robot_docs_guide();
    assert!(body.contains("emath capabilities --json"), "{body}");
    assert!(body.contains("Exit codes"), "{body}");
}

#[test]
fn flags_for_matches_implemented_usage() {
    assert!(flags_for("build").contains(&"--verify"));
    assert!(flags_for("build").contains(&"--json"));
    assert!(!flags_for("run").contains(&"--json"));
    assert!(!flags_for("run").contains(&"--verify"));
    assert!(flags_for("run").contains(&"--out"));
    assert!(!flags_for("test").contains(&"--json"));
    assert!(!flags_for("new").contains(&"--verify"));
    assert!(!flags_for("vendor").contains(&"--json"));
    assert_eq!(
        reject_unknown_flags("run", &["f.emath".into(), "--verify".into()]),
        Some(2)
    );
    assert_eq!(
        reject_unknown_flags("vendor", &["--out".into(), "d".into(), "--json".into()]),
        Some(2)
    );
}
