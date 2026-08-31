//! `emath check --verify-data` (04 §5.2, bead emath-r3-observations-9ffu).
//!
//! Contracts:
//! - a file whose InstrumentRun provenance declares a sha256 matching the
//!   data file on disk passes `--verify-data` (exit 0, no E-OBS-HASH);
//! - a file whose declared digest no on-disk file hashes to refuses with
//!   `E-OBS-HASH` and exit 1 (missing file and digest drift are the same
//!   refusal lane: the evidence cannot be confirmed);
//! - plain `emath check` (no flag) does NOT hash: provenance is
//!   declared, not verified, without the flag;
//! - a definitions binding of an observation name refuses `E-OBS-WRITE`
//!   at plain check (read-only measured evidence).

use std::path::PathBuf;
use std::process::Command;

fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn cli(args: &[&str]) -> (String, i32) {
    let output = Command::new(env!("CARGO"))
        .args(["run", "-q", "-p", "emath-cli", "--"])
        .args(args)
        .output()
        .expect("run emath via cargo");
    // Diagnostics print on stderr (output-style rule); assertions match
    // the combined stream so the exact E-* code is assertable either way.
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    (text, output.status.code().unwrap_or(-1))
}

#[test]
fn verify_data_accepts_matching_digest() {
    // The example ships `pk_run_041.csv` beside it with the declared
    // sha256 of those exact bytes.
    let path = repo_file("language/examples/science/observations.emath");
    let (stdout, code) = cli(&["check", "--verify-data", path.to_str().expect("utf8")]);
    assert_eq!(code, 0, "matching digest must pass; got:\n{stdout}");
    assert!(
        !stdout.contains("E-OBS-HASH"),
        "no E-OBS-HASH when the digest matches; got:\n{stdout}"
    );
}

#[test]
fn verify_data_refuses_digest_drift() {
    // The declared digest is a placeholder (64 ones) no on-disk file
    // hashes to; `pk_run_041.csv` is absent beside the fixture, so the
    // evidence cannot be confirmed: E-OBS-HASH, exit 1.
    let path = repo_file("tests/invalid/observations_hash_drift.emath");
    let (stdout, code) = cli(&["check", "--verify-data", path.to_str().expect("utf8")]);
    assert_eq!(code, 1, "unconfirmable evidence must refuse; got:\n{stdout}");
    assert!(
        stdout.contains("E-OBS-HASH"),
        "drift/missing data must refuse E-OBS-HASH; got:\n{stdout}"
    );
}

#[test]
fn plain_check_does_not_hash_data() {
    // Without --verify-data, provenance is declared but not verified:
    // the drift fixture admits (provenance declared, not checked).
    let path = repo_file("tests/invalid/observations_hash_drift.emath");
    let (stdout, code) = cli(&["check", path.to_str().expect("utf8")]);
    assert_eq!(code, 0, "plain check must not hash; got:\n{stdout}");
    assert!(
        !stdout.contains("E-OBS-HASH"),
        "no hashing without the flag; got:\n{stdout}"
    );
}

#[test]
fn writing_to_an_observation_refuses_at_plain_check() {
    let path = repo_file("tests/invalid/observations_write.emath");
    let (stdout, code) = cli(&["check", path.to_str().expect("utf8")]);
    assert_eq!(code, 1, "E-OBS-WRITE must refuse; got:\n{stdout}");
    assert!(
        stdout.contains("E-OBS-WRITE"),
        "definitions binding an observation must refuse E-OBS-WRITE; got:\n{stdout}"
    );
}
