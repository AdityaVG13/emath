//! `emath verify` end-to-end: a library holding one claim of each
//! producer kind (construction witness, exhaustion receipt, certified
//! bound) verifies with one row per claim, where a row reads settled
//! only when the reference VM and the emitted Rust AGREE on the
//! checker's verdict; a claim whose checker the emitter cannot lower
//! is reported vm-only with the emit refusal named, not settled, not a
//! crash; a tampered certificate in any kind refuses with the claim
//! named; a claim whose stored content id does not match its content
//! refuses; a claim whose checker the library neither defines nor uses
//! refuses; an injected native disagreement (a compile-time-gated test
//! fault, unreachable from production builds) leaves the claim
//! unsettled with both verdicts shown; and `--json` emits the same
//! rows as the table.

mod common;

use std::path::PathBuf;
use std::process::Command;

use common::emath_bin;
use emath_cli::{EXIT_OK, EXIT_REFUSED};
use emath_test_harness::Probe;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn emath_env(dir: &PathBuf, args: &[&str], env: &[(&str, &str)]) -> (String, i32) {
    let out = Command::new(emath_bin())
        .args(args)
        .current_dir(dir)
        .env("EMATH_ROOT", workspace())
        .envs(env.iter().map(|(k, v)| (*k, *v)))
        .output()
        .expect("run emath");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (text, out.status.code().unwrap_or(-1))
}

fn emath(dir: &PathBuf, args: &[&str]) -> (String, i32) {
    emath_env(dir, args, &[])
}

fn verify(dir: &PathBuf, file: &str, extra: &[&str]) -> (String, i32) {
    let mut args = vec!["verify", file];
    args.extend_from_slice(extra);
    emath(dir, &args)
}

#[test]
fn verify_cmd() {
    let mut probe = Probe::new("emath verify: two-path recheck of kept claims, refuse tampered ones");
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

    // A library with one claim of each producer kind verifies: one row
    // per claim. The witness claim's checker lowers, and the VM and
    // the emitted Rust agree on its verdict, so its row reads settled.
    // The other two claims are outside the emitter's current subset
    // (the refusals are named), so their rows read vm-only, not
    // settled, not a crash.
    let (text, code) = verify(&dir, "claims.emath", &[]);
    probe.demand(
        "witness-claim-settles-on-agreement",
        code == EXIT_OK as i32
            && text.contains("witness_claim kind=0 standing=5 checker=witness_checker conditions=5 verdict=settled native=true"),
        format!("exit {code}\n{text}"),
    );
    probe.demand(
        "unlowerable-checker-is-vm-only-not-a-crash",
        code == EXIT_OK as i32
            && text.contains("exhaustion_claim kind=1 standing=3 checker=exhaustion_checker conditions=2 verdict=vm-only native=unavailable")
            && text.contains("certified_bound_claim kind=2 standing=4 checker=bound_checker")
            && text.contains("verdict=vm-only native=unavailable")
            && text.contains("the emitter cannot lower"),
        format!("exit {code}\n{text}"),
    );

    // --json emits the same rows: the same names, the same tags, the
    // same verdicts, the same native facts.
    let (text, code) = verify(&dir, "claims.emath", &["--json"]);
    probe.demand(
        "json-emits-the-same-rows",
        code == EXIT_OK as i32
            && text.contains("\"name\":\"witness_claim\"")
            && text.contains("\"verdict\":\"settled\",\"native\":true")
            && text.contains("\"name\":\"exhaustion_claim\"")
            && text.contains("\"verdict\":\"vm-only\",\"native\":\"unavailable\"")
            && text.contains("\"name\":\"certified_bound_claim\"")
            && text.contains("\"kind\":0")
            && text.contains("\"standing\":5"),
        format!("exit {code}\n{text}"),
    );

    // A tampered certificate in each kind refuses with the claim
    // named. The records stay internally consistent (make_claim
    // recomputed the content id), so the refusal is the checker's
    // verdict, not bookkeeping. The native lane never runs: the VM
    // refusal stands alone.
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

/// The two-path DECISION is a pure library function, tested directly
/// through the library this crate links: agreement settles, a checker
/// the emitter cannot lower is vm-only with the refusal preserved, and
/// a disagreement is a compiler defect, not settled (there is no
/// accept direction: nothing turns a native rejection into a
/// settlement).
#[test]
fn two_path_decision() {
    let mut probe = Probe::new("two_path: settled only on agreement");
    probe.demand(
        "agreement-settles",
        emath_cli::verify_cmd::two_path(Ok(true)) == emath_cli::verify_cmd::TwoPath::Settled,
        "a native verdict that agrees with the VM must settle the claim",
    );
    probe.demand(
        "disagreement-is-a-compiler-defect",
        emath_cli::verify_cmd::two_path(Ok(false)) == emath_cli::verify_cmd::TwoPath::CompilerDefect,
        "a native rejection of a VM-accepted claim must be a compiler defect, never settled",
    );
    match emath_cli::verify_cmd::two_path(Err("the emitter cannot lower `c`".to_string())) {
        emath_cli::verify_cmd::TwoPath::VmOnly(reason) => probe.demand(
            "unreachable-lane-is-vm-only-naming-the-refusal",
            reason.contains("cannot lower"),
            format!("the emit refusal must be preserved: {reason}"),
        ),
        other => probe.fail(
            "unreachable-lane-is-vm-only-naming-the-refusal",
            format!("an unreachable native lane must be vm-only, got {other:?}"),
        ),
    };
    probe.finish();
}
