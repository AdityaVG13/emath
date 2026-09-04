//! `emath migrate` CLI (contract
//!, 05 §5).
//!
//! Contracts:
//! - `emath migrate <file.emath> --check` prints the canonical-format
//!   verdict WITHOUT rewriting (exit 0 = canonical / idempotent,
//!   exit 1 = a rule would fire);
//! - `emath migrate <file.emath> [--fix] [--receipt <path>]` verifies
//!   and emits: the file is rewritten ONLY under `--fix` and only when
//!   the respell verified identity byte-for-byte; the receipt (schema
//!   `emath.migration-receipt v1`, canonical stable JSON) lands at
//!   `--receipt` (or beside the source by default);
//! - a refusing source produces `E-MIG-SOURCE-REFUSES` in the receipt
//!   with the source untouched (exit 1, partial-refused);
//! - replay is byte-identical: same input = same receipt bytes.

use std::path::PathBuf;

mod common;
use common::cli;

fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("emath_migrate_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

const NON_CANONICAL: &str = "\
emath function plain:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y=x*2.0
";

const REFUSING: &str = "\
emath function broken:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y = undefined_name
";

#[test]
fn check_mode_reports_noncanonical_without_writing() {
    let dir = scratch_dir("check");
    let file = dir.join("plain.emath");
    std::fs::write(&file, NON_CANONICAL).expect("write");
    let (text, code) = cli(&["migrate", file.to_str().expect("utf8"), "--check"]);
    assert_eq!(
        code, 1,
        "non-canonical source: a rule would fire; got:\n{text}"
    );
    assert!(
        text.contains("E-MIG-RULE-001"),
        "output names the rule; got:\n{text}"
    );
    // --check never rewrites.
    assert_eq!(
        std::fs::read_to_string(&file).expect("read back"),
        NON_CANONICAL,
        "--check must leave the source untouched"
    );
}

#[test]
fn fix_then_check_is_idempotent() {
    let dir = scratch_dir("fix_ok");
    let file = dir.join("plain.emath");
    std::fs::write(&file, NON_CANONICAL).expect("write");
    let (text, code) = cli(&["migrate", file.to_str().expect("utf8"), "--fix"]);
    assert_eq!(code, 0, "verified respell must succeed; got:\n{text}");
    let canonical = std::fs::read_to_string(&file).expect("read back");
    assert_ne!(canonical, NON_CANONICAL, "--fix rewrites to canonical form");
    let (text2, code2) = cli(&["migrate", file.to_str().expect("utf8"), "--check"]);
    assert_eq!(code2, 0, "canonical source is idempotent; got:\n{text2}");
    assert_eq!(
        std::fs::read_to_string(&file).expect("read back"),
        canonical,
        "second run must not change the source"
    );
}

#[test]
fn receipt_is_canonical_stable_json() {
    let dir = scratch_dir("fix");
    let file = dir.join("plain.emath");
    std::fs::write(&file, NON_CANONICAL).expect("write");
    let receipt_path = dir.join("mig.json");
    let (text, code) = cli(&[
        "migrate",
        file.to_str().expect("utf8"),
        "--fix",
        "--receipt",
        receipt_path.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 0, "verified respell must succeed; got:\n{text}");
    let receipt = std::fs::read_to_string(&receipt_path).expect("receipt written");
    assert!(
        receipt.starts_with("{\"schema\":\"emath.migration-receipt v1\""),
        "receipt is the canonical stable-JSON artifact; got:\n{receipt}"
    );
    assert!(receipt.contains("\"kind\":\"respell\""));
    assert!(receipt.contains("\"identity_delta\":\"none\""));
    assert!(receipt.contains("\"identity_verified\":true"));
    assert!(receipt.contains("\"verdict\":\"complete\""));
    assert!(receipt.contains("E-MIG-RULE-001"));
    // Replay: the same migration on the same input (same path, same
    // bytes) is byte-identical. Restore the pre-fix bytes at the SAME
    // path and re-run --fix; only the receipt content must match.
    std::fs::write(&file, NON_CANONICAL).expect("restore");
    let receipt2_path = dir.join("mig2.json");
    let _ = cli(&[
        "migrate",
        file.to_str().expect("utf8"),
        "--fix",
        "--receipt",
        receipt2_path.to_str().expect("utf8"),
    ]);
    let receipt2 = std::fs::read_to_string(&receipt2_path).expect("receipt 2");
    assert_eq!(
        receipt, receipt2,
        "replay: same input = byte-identical receipt"
    );
}

#[test]
fn without_fix_the_rewrite_is_never_emitted() {
    let dir = scratch_dir("nofix");
    let file = dir.join("plain.emath");
    std::fs::write(&file, NON_CANONICAL).expect("write");
    let (text, code) = cli(&["migrate", file.to_str().expect("utf8")]);
    assert_eq!(code, 1, "a rule would fire without --fix; got:\n{text}");
    assert_eq!(
        std::fs::read_to_string(&file).expect("read back"),
        NON_CANONICAL,
        "without --fix the source is never rewritten"
    );
}

#[test]
fn refusing_source_is_receipted_and_untouched() {
    let dir = scratch_dir("refusing");
    let file = dir.join("broken.emath");
    std::fs::write(&file, REFUSING).expect("write");
    let receipt_path = dir.join("mig.json");
    let (text, code) = cli(&[
        "migrate",
        file.to_str().expect("utf8"),
        "--receipt",
        receipt_path.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 1, "refusing source is partial-refused; got:\n{text}");
    assert!(
        text.contains("E-MIG-SOURCE-REFUSES"),
        "the refusal code is surfaced; got:\n{text}"
    );
    let receipt = std::fs::read_to_string(&receipt_path).expect("receipt written");
    assert!(receipt.contains("E-MIG-SOURCE-REFUSES"));
    assert!(receipt.contains("\"verdict\":\"partial-refused\""));
    assert_eq!(
        std::fs::read_to_string(&file).expect("read back"),
        REFUSING,
        "migrate never rewrites a refusing source"
    );
}

#[test]
fn registry_is_printed_and_rule_is_stable() {
    let (text, code) = cli(&["migrate", "--list-rules"]);
    assert_eq!(code, 0, "rule registry must print; got:\n{text}");
    assert!(
        text.contains("E-MIG-RULE-001") && text.contains("canonical-format"),
        "registry lists the canonical-format rule; got:\n{text}"
    );
    let _ = repo_file(""); // keep the helper referenced for future fixtures
}
