//! `emath verify` end-to-end: a library holding one claim of each
//! producer kind (construction witness, exhaustion receipt, certified
//! bound) verifies with one accepted row per claim; a tampered
//! certificate in any kind refuses with the claim named; a claim whose
//! stored content id does not match its content refuses; a claim whose
//! checker the library neither defines nor uses refuses; and `--json`
//! emits the same rows as the table.

mod common;

use std::path::PathBuf;
use std::process::Command;

use common::emath_bin;
use emath_cli::{EXIT_OK, EXIT_REFUSED};
use emath_test_harness::Probe;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn emath(dir: &PathBuf, args: &[&str]) -> (String, i32) {
    let out = Command::new(emath_bin())
        .args(args)
        .current_dir(dir)
        .env("EMATH_ROOT", workspace())
        .output()
        .expect("run emath");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (text, out.status.code().unwrap_or(-1))
}

fn verify(dir: &PathBuf, file: &str, extra: &[&str]) -> (String, i32) {
    let mut args = vec!["verify", file];
    args.extend_from_slice(extra);
    emath(dir, &args)
}

#[test]
fn verify_cmd() {
    let mut probe = Probe::new("emath verify: recheck kept claims, refuse tampered ones");
    let dir = std::env::temp_dir().join(format!("emath_verify_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let fixtures = workspace().join("tests/fixtures/verify");
    for file in [
        "claims.emath",
        "tamper_witness.emath",
        "tamper_exhaustion.emath",
        "tamper_bound.emath",
        "tamper_id.emath",
        "unknown_checker.emath",
    ] {
        std::fs::copy(fixtures.join(file), dir.join(file)).expect("copy fixture");
    }

    // A library with one claim of each producer kind verifies: one
    // accepted row per claim, named, with kind and standing tags.
    let (text, code) = verify(&dir, "claims.emath", &[]);
    probe.demand(
        "all-three-kinds-verify",
        code == EXIT_OK as i32
            && text.contains("witness_claim kind=0 standing=5 checker=witness_checker conditions=5 verdict=accepted")
            && text.contains("exhaustion_claim kind=1 standing=3 checker=exhaustion_checker conditions=2 verdict=accepted")
            && text.contains("certified_bound_claim kind=2 standing=4 checker=bound_checker")
            && text.contains("verdict=accepted"),
        format!("exit {code}\n{text}"),
    );

    // --json emits the same rows: the same names, the same tags, the
    // same verdicts.
    let (text, code) = verify(&dir, "claims.emath", &["--json"]);
    probe.demand(
        "json-emits-the-same-rows",
        code == EXIT_OK as i32
            && text.contains("\"name\":\"witness_claim\"")
            && text.contains("\"name\":\"exhaustion_claim\"")
            && text.contains("\"name\":\"certified_bound_claim\"")
            && text.contains("\"kind\":0")
            && text.contains("\"standing\":5")
            && text.contains("\"verdict\":\"accepted\""),
        format!("exit {code}\n{text}"),
    );

    // A tampered certificate in each kind refuses with the claim
    // named. The records stay internally consistent (make_claim
    // recomputed the content id), so the refusal is the checker's
    // verdict, not bookkeeping.
    for (fixture, claim) in [
        ("tamper_witness.emath", "tampered_witness_claim"),
        ("tamper_exhaustion.emath", "tampered_exhaustion_claim"),
        ("tamper_bound.emath", "tampered_bound_claim"),
    ] {
        let (text, code) = verify(&dir, fixture, &[]);
        probe.demand(
            &format!("tampered-certificate-refuses-{fixture}"),
            code == EXIT_REFUSED as i32 && text.contains("E-VERIFY-REJECTED") && text.contains(claim),
            format!("exit {code}\n{text}"),
        );
    }

    // A claim whose bytes differ from its content id refuses with the
    // claim named, before any checker runs.
    let (text, code) = verify(&dir, "tamper_id.emath", &[]);
    probe.demand(
        "content-id-mismatch-refuses",
        code == EXIT_REFUSED as i32 && text.contains("E-VERIFY-CONTENT-ID") && text.contains("tampered_id_claim"),
        format!("exit {code}\n{text}"),
    );

    // A claim whose checker the library neither defines nor use is a
    // named refusal, not a crash.
    let (text, code) = verify(&dir, "unknown_checker.emath", &[]);
    probe.demand(
        "unknown-checker-refuses",
        code == EXIT_REFUSED as i32 && text.contains("E-VERIFY-CHECKER") && text.contains("stranger_claim"),
        format!("exit {code}\n{text}"),
    );

    probe.finish();
}
