//! Buffer-carrier sieve through the real lanes:
//! `emath run --json` answers exactly with work telemetry, a tiny
//! `--work` budget suspends mid-sieve and writes a checkpoint, and
//! `emath step` resumes to the same exact answer — proving the ENCODE
//! checkpoint decision (buffers serialize with aliasing fidelity).
//! Failure-first: `buffer` is an unbound name on the pre-carrier
//! engine, so every case faults before the seam exists.

mod common;
use emath_cli::{EXIT_OK, EXIT_PARTIAL};
use emath_test_harness::Probe;

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("emath-buffer-sieve-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn probe() {
    let mut p = Probe::new("the sieve runs, suspends, and resumes exactly through the lanes");
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/constructor/buffer_sieve.emath"
    );

    p.case("run-answers-with-work-telemetry", |p| {
        let (text, code) = common::cli(&[
            "run",
            fixture,
            "--function",
            "SieveSum",
            "--set",
            "limit=100",
            "--work",
            "2000000",
            "--json",
        ]);
        p.eq("exit", code, EXIT_OK as i32);
        p.contains("exact answer", &text, "1060");
        p.contains("work telemetry", &text, "\"work_consumed\"");
    });

    p.case("tiny-budget-suspends-and-writes-checkpoint", |p| {
        let out = scratch_dir("suspended");
        let (text, code) = common::cli(&[
            "run",
            fixture,
            "--function",
            "SieveSum",
            "--set",
            "limit=100",
            "--work",
            "200",
            "--out",
            &out.to_string_lossy(),
        ]);
        p.eq("exit", code, EXIT_PARTIAL as i32);
        p.contains("suspended receipt", &text, "suspended");
        let checkpoint = out.join("constructor-checkpoint.json");
        p.demand(
            "checkpoint-written",
            checkpoint.is_file(),
            "a suspended buffer run must write its checkpoint (ENCODE decision)",
        );
        let (text, code) = common::cli(&[
            "step",
            &checkpoint.to_string_lossy(),
            "--work",
            "2000000",
        ]);
        p.eq("resume-exit", code, EXIT_OK as i32);
        p.contains("resumed-exact", &text, "1060");
    });

    p.finish();
}
