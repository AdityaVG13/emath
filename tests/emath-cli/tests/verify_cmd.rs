//! `emath verify` end-to-end: a library holding one claim of each
//! producer kind (construction witness, exhaustion receipt, certified
//! bound) verifies with one row per claim, where a row reads settled
//! only when the reference VM and the emitted Rust AGREE on the
//! checker's verdict; a claim whose checker the emitter cannot lower
//! is still reported vm-only with the emit refusal named, not settled,
//! not a crash; a tampered certificate in any kind refuses with the
//! claim named; a claim whose stored content id does not match its
//! content refuses; a claim whose checker the library neither defines
//! nor uses refuses; and `--json` emits the same rows as the table.

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
    // per claim. Every checker lowers and the VM and the emitted Rust
    // agree on their verdicts, so all three rows read settled (bead
    // emath-cfe89 widened the artifact tree with record literals and
    // nested captures, so the bound claim's certificate template now
    // lowers and its checker settles instead of reading vm-only).
    let (text, code) = verify(&dir, "claims.emath", &[]);
    probe.demand(
        "witness-claim-settles-on-agreement",
        code == EXIT_OK as i32
            && text.contains("witness_claim kind=0 standing=5 checker=witness_checker conditions=5 verdict=settled native=true"),
        format!("exit {code}\n{text}"),
    );
    probe.demand(
        "exhaustion-claim-settles",
        code == EXIT_OK as i32
            && text.contains("exhaustion_claim kind=1 standing=3 checker=exhaustion_checker conditions=2 verdict=settled native=true"),
        format!("exit {code}\n{text}"),
    );
    probe.demand(
        "bound-claim-settles",
        code == EXIT_OK as i32
            && text.contains("certified_bound_claim kind=2 standing=4 checker=bound_checker")
            && text.contains("verdict=settled native=true"),
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
            && text.contains("\"verdict\":\"settled\",\"native\":true")
            && text.contains("\"name\":\"certified_bound_claim\"")
            && text.contains("\"verdict\":\"settled\",\"native\":true")
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

/// Bounded verify notes (bead emath-agq2i): a run over a library with
/// unlowerable claims must not print the crate-wide emitter report per
/// claim, and a diagnostic must name the expression form and span, not
/// the Debug tree of the whole Expr subtree. Every refusal stays
/// named: each claim's note names the function that failed to lower,
/// and the shared emitter refusals appear once per run. The fixture
/// carries THREE unlowerable claims sharing one refusal (Float64
/// templates the artifact lanes refuse): with a single claim,
/// "once per run" and "once per claim" are indistinguishable, so the
/// dedup law could not fail. bench_179's checkers lower since
/// emath-cfe89 widened the artifact tree, so they no longer exercise
/// this lane.
#[test]
fn verify_notes_are_bounded() {
    let mut probe = Probe::new("emath verify: emitter refusals stay bounded and named");
    let dir = std::env::temp_dir().join(format!("emath_verify_unlowerable_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::copy(
        workspace().join("tests/fixtures/verify/unlowerable_checkers.emath"),
        dir.join("unlowerable_checkers.emath"),
    )
    .expect("copy fixture");
    let (text, code) = verify(&dir, "unlowerable_checkers.emath", &[]);
    probe.demand("verify-succeeds", code == EXIT_OK as i32, format!("exit {code}\n{text}"));

    // Every unlowerable claim names its function: exactly one note per
    // unlowerable claim, no more (the checker's own refusal is folded
    // into its claim's note, not a second one).
    let cannot_lower = text.matches("the emitter cannot lower `").count();
    probe.demand(
        "every-unlowerable-claim-still-named",
        cannot_lower == 3,
        format!("expected 3 cannot-lower notes (one per unlowerable claim), got {cannot_lower}"),
    );

    // The shared emitter refusals print once per run, not once per
    // claim: the float-template refusal (the shared emitter report)
    // must not repeat per vm-only row. The counted string appears only
    // in the shared report — the per-claim notes name the function and
    // carry no reason text — so the count is the shared report's.
    let shared = text.matches("float template").count();
    probe.demand(
        "shared-refusals-once-per-run",
        shared == 1,
        format!("the shared emitter refusal printed {shared} times"),
    );

    // The whole run stays bounded: no Debug tree of an Expr subtree in
    // any note.
    let spanned = text.matches("Span { file: FileId").count();
    probe.demand(
        "no-expr-debug-tree-in-notes",
        spanned == 0,
        format!("{spanned} raw span dumps in the run's output"),
    );
    probe.demand(
        "run-output-bounded",
        text.len() < 16 * 1024,
        format!("the verify run printed {} bytes", text.len()),
    );
    probe.finish();
}

/// `--work N` budgets each claim's producer evaluation and checker
/// replay, as `emath test --work` budgets an example (bead
/// emath-8cz8x). The bench_179 receipts exceed the default 1,000,000
/// work units, so under the default budget the run refuses the first
/// oversized claim by name with the named code and the named budget
/// (never the bare VM error), and at a stated budget every claim
/// verifies.
#[test]
fn verify_work_budget() {
    let mut probe = Probe::new("emath verify --work: per-claim budget for large certificates");
    let file = "tests/fixtures/invention/bench_179/library_large.emath";

    // The default budget refuses the first oversized claim: the named
    // code, the claim, and the 1,000,000 budget it exhausted.
    let (text, code) = verify(&workspace(), file, &[]);
    probe.demand(
        "default-budget-refuses-by-name",
        code == EXIT_REFUSED as i32
            && text.contains("E-VERIFY-WORK")
            && text.contains("bench179_claim_15")
            && text.contains("1000000"),
        format!("exit {code}\n{text}"),
    );

    // A stated budget verifies the whole library: the last claim's row
    // is present and no refusal stands.
    let (text, code) = verify(&workspace(), file, &["--work", "40000000"]);
    probe.demand(
        "stated-work-verifies-every-claim",
        code == EXIT_OK as i32 && text.contains("bench179_claim_20") && text.contains("verdict="),
        format!("exit {code}\n{text}"),
    );

    // `--work 0` is refused at parse, as in `emath run`.
    let (text, code) = verify(&workspace(), file, &["--work", "0"]);
    probe.demand(
        "work-zero-refuses-at-parse",
        code != EXIT_OK as i32 && text.contains("usage"),
        format!("exit {code}\n{text}"),
    );

    probe.finish();
}

/// A generic receipt recheck over a node-decoded certificate settles
/// natively (bead emath-886vm). The checker decodes the certificate
/// with an unapplied `quote.evaluate`, projects the receipt's fields,
/// and hands the decoded receipt to the generic `recheck_receipt<S>`
/// beside a concrete `Extend<Int>` problem: the two arguments must
/// unify S to one concrete instantiation and the decoded receipt must
/// cross into it checked (the declared record carrier wins over the
/// dynamic node at the frame binding). The claim reads settled only
/// when the emitted artifact compiles and agrees with the VM; a
/// merged-instantiation render (the defect: one body carrying two
/// S bindings) leaves the artifact uncompilable and the claim vm-only.
///
/// The second claim pins the substitution edge (rework 2): its
/// certificate is built by `quote.substitute` over a quoted list
/// literal, and the VM's `substitute_path` keeps a list a list - the
/// emitted `substitute_tree` must keep `CodeTree::Sequence` a
/// sequence too, or the decoded certificate loses `length` and the
/// checker faults while the VM answers.
#[test]
fn generic_recheck_settles() {
    let mut probe = Probe::new("emath verify: generic receipt recheck over a decoded certificate");
    let dir = std::env::temp_dir().join(format!("emath_verify_generic_recheck_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::copy(
        workspace().join("tests/fixtures/verify/generic_recheck.emath"),
        dir.join("generic_recheck.emath"),
    )
    .expect("copy fixture");
    let (text, code) = verify(&dir, "generic_recheck.emath", &[]);
    probe.demand(
        "generic-recheck-settles-natively",
        code == EXIT_OK as i32 && text.contains("generic_recheck_claim") && text.contains("verdict=settled native=true"),
        format!("exit {code}\n{text}"),
    );
    let settled = text.matches("verdict=settled native=true").count();
    probe.demand(
        "substitution-list-settles-natively",
        text.contains("substitution_list_claim") && settled == 2,
        format!("expected both claims settled native=true, got {settled}\n{text}"),
    );
    probe.finish();
}
