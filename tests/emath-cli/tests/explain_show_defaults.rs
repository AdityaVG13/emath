//! F8 `emath explain --show-defaults`.
//!
//! Contracts:
//! - the flag prints every effective default, each labeled with its
//!   source (language default / declaration attribute / planner default);
//! - a declaration that OVERRIDES a default (`@units_profile`) gains a
//!   per-declaration row; a plain file gains none (the table never
//!   invents overrides);
//! - output is deterministic: same input, byte-identical stdout;
//! - a refused file exits 1 with its diagnostics, never a defaults table;
//! - the `--json` envelope carries `defaults` rows plus
//!   `declaration_overrides`.

use std::path::PathBuf;
use std::process::Command;

mod common;

fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn cli(args: &[&str]) -> (String, i32) {
    // The binary is execed directly (resolved once per process, built on
    // demand for a cold target dir); fixture paths are repo-root-relative.
    // stdout only: the defaults table is a stdout contract.
    let output = Command::new(common::emath_bin())
        .args(args)
        .output()
        .expect("run emath binary");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn show_defaults_lists_every_labeled_default() {
    let path = repo_file("tests/valid/square.emath");
    let (stdout, code) = cli(&["explain", "--show-defaults", path.to_str().expect("utf8")]);
    assert_eq!(code, 0, "plain admitted file must exit 0");
    for expected in [
        "numeric-profile: strict-f64",
        "source: language default",
        "units-profile: permissive",
        "visibility: public",
        "outputs: all definitions",
        "compile: target rust, profile library, numeric strict-f64",
        "untyped-inputs: Float64",
        "source: planner default",
    ] {
        assert!(
            stdout.contains(expected),
            "defaults table must name `{expected}`; got:\n{stdout}"
        );
    }
    assert!(
        !stdout.contains("units-profile: Square="),
        "a file with no @units_profile must gain no declaration override row; got:\n{stdout}"
    );
}

#[test]
fn show_defaults_reports_declaration_overrides() {
    let path = repo_file("tests/fixtures/language/intro/units-profile.emath");
    let (stdout, code) = cli(&["explain", "--show-defaults", path.to_str().expect("utf8")]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("units-profile: Calibrated=publication (source: declaration attribute;"),
        "declared profile must appear as a per-declaration override row; got:\n{stdout}"
    );
}

#[test]
fn show_defaults_is_byte_identical_across_runs() {
    let path = repo_file("tests/valid/square.emath")
        .to_str()
        .expect("utf8")
        .to_string();
    let (first, _) = cli(&["explain", "--show-defaults", &path]);
    let (second, _) = cli(&["explain", "--show-defaults", &path]);
    assert_eq!(
        first, second,
        "same input must produce byte-identical output"
    );
}

#[test]
fn show_defaults_on_refused_file_exits_1_without_table() {
    let path = repo_file("tests/invalid/unit_mismatch.emath");
    let (stdout, code) = cli(&["explain", "--show-defaults", path.to_str().expect("utf8")]);
    assert_eq!(code, 1, "refused file must exit 1");
    assert!(
        !stdout.contains("effective defaults"),
        "no defaults table on a refused file; got:\n{stdout}"
    );
}

#[test]
fn show_defaults_json_carries_rows_and_overrides() {
    let path = repo_file("tests/fixtures/language/intro/units-profile.emath");
    let (stdout, code) = cli(&[
        "explain",
        "--show-defaults",
        path.to_str().expect("utf8"),
        "--json",
    ]);
    assert_eq!(code, 0);
    let parsed = emath_artifact::parse_json_document(stdout.trim())
        .expect("show-defaults envelope must parse as JSON");
    assert_eq!(
        parsed.string_field("command").ok().as_deref(),
        Some("explain-show-defaults")
    );
    let emath_artifact::JsonValue::Arr(defaults) = parsed.field("defaults").expect("defaults rows")
    else {
        panic!("defaults must be an array");
    };
    assert_eq!(defaults.len(), 7, "seven effective defaults");
    for row in defaults {
        assert!(
            row.string_field("source").is_ok(),
            "every default row must name its source: {row:?}"
        );
    }
    let emath_artifact::JsonValue::Arr(overrides) = parsed
        .field("declaration_overrides")
        .expect("declaration_overrides")
    else {
        panic!("declaration_overrides must be an array");
    };
    assert_eq!(overrides.len(), 1);
    assert_eq!(
        overrides[0].string_field("declaration").ok().as_deref(),
        Some("Calibrated")
    );
    assert_eq!(
        overrides[0].string_field("value").ok().as_deref(),
        Some("publication")
    );
}
