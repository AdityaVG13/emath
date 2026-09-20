//! Line-REPL core acceptance (bead emath-q3cqx).
//!
//! `run_repl` drives an opened session surface over any line reader
//! (stdin or a script file) with a deterministic transcript: the same
//! session script produces byte-identical output on every run. The
//! laws pinned here:
//!   - the valley script walks to the authored goal with exact batch
//!     lines (verdict vocabulary, used, incumbent, promoted, frozen
//!     cases, archive length);
//!   - byte-determinism: two runs of the same script are identical;
//!   - save/load through REPL commands resumes exactly where the
//!     straight run is;
//!   - grow-case is the host curriculum seam: a frozen case id shows
//!     in the next batch line, duplicates refuse by name, and the
//!     next batch re-scores the archive (the freshness law);
//!   - a budget halt prints `budget_exhausted` and the next step
//!     resumes (the replay fixture's discipline);
//!   - unknown commands and engine faults print named errors and the
//!     session continues; quit ends with a summary line;
//!   - export-native emits and builds the native epoch host (artifact
//!     crate + sibling bin); the receipt names both crates and the
//!     binary (cross-lane parity is export_native.rs's acceptance);

use std::io::BufReader;

use emath_test_harness::Probe;
use emath_tui::host::LoopHost;
use emath_tui::repl::{run_repl, ReplConfig};

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/constructor")
        .join(name)
}

fn valley_host() -> LoopHost {
    LoopHost::open(&fixture_path("research_step_valley.emath"), Some("StepValley"))
        .expect("open valley")
}

/// Run a script against a fresh session, returning the transcript.
fn drive(host: &LoopHost, lines: &str) -> (String, i128) {
    let mut input = BufReader::new(lines.as_bytes());
    let mut out = Vec::new();
    let config = ReplConfig {
        default_budget: 60,
        interactive: false,
    };
    let outcome = run_repl(host, &config, &mut input, &mut out).expect("repl run");
    (
        String::from_utf8(out).expect("utf8 transcript"),
        outcome.verdict,
    )
}

#[test]
fn repl_sessions() {
    let mut probe = Probe::new("line-REPL: deterministic scripted loop sessions");

    // 1. The valley script walks to the authored goal.
    probe.case("repl-valley-script-to-goal", |p| {
        let (transcript, verdict) = drive(
            &valley_host(),
            "step 60\nstep 60\nstep 60\nshow\nquit\n",
        );
        p.demand("goal-verdict", verdict == 1, format!("{transcript}"));
        p.demand(
            "batch1-line",
            transcript.contains(
                "batch 1 verdict plateau used 6 incumbent key 0 score 4/1 \
                 promoted [] quarantined [] cases [] archive 4",
            ),
            transcript.clone(),
        );
        p.demand(
            "batch3-line",
            transcript.contains(
                "batch 3 verdict goal_attained used 33 incumbent key 7 score 6/1 \
                 promoted [7] quarantined [] cases [] archive 8",
            ),
            transcript.clone(),
        );
        p.demand(
            "show-incumbent",
            transcript.contains("incumbent key 7 value 7/1 score 6/1 tier 1 accepted true"),
            transcript.clone(),
        );
        p.demand(
            "end-summary",
            transcript.contains("end batches 3 verdict goal_attained"),
            transcript.clone(),
        );
    });

    // 2. Byte-determinism: the same script twice is identical.
    probe.case("repl-determinism", |p| {
        let (first, _) = drive(&valley_host(), "run 3\nquit\n");
        let (second, _) = drive(&valley_host(), "run 3\nquit\n");
        p.demand("byte-identical", first == second, format!("{first}\n---\n{second}"));
    });

    // 3. Save through the REPL, load in a fresh session, resume to the
    //    same goal as the straight run.
    probe.case("repl-save-load-resume", |p| {
        let host = valley_host();
        let scratch = std::env::temp_dir()
            .join(format!("emath_repl_scratch_{}.json", std::process::id()));
        let (transcript, _) = drive(
            &host,
            &format!("step 60\nstep 60\nsave {}\nquit\n", scratch.display()),
        );
        p.demand(
            "saved-line",
            transcript.contains("saved revision 2"),
            transcript.clone(),
        );
        let (resumed, verdict) = drive(
            &host,
            &format!("load {}\nstep 60\nquit\n", scratch.display()),
        );
        p.demand("loaded-line", resumed.contains("loaded revision 2"), resumed.clone());
        p.demand(
            "resumed-goal",
            verdict == 1 && resumed.contains("batch 3 verdict goal_attained"),
            resumed.clone(),
        );
    });

    // 4. grow-case: the host curriculum seam.
    probe.case("repl-grow-case", |p| {
        let (transcript, _) = drive(
            &valley_host(),
            "step 60\ngrow-case 5\nstep 60\ngrow-case 5\nquit\n",
        );
        p.demand(
            "frozen-line",
            transcript.contains("case 5 frozen"),
            transcript.clone(),
        );
        p.demand(
            "case-in-batch-line",
            transcript.contains("cases [5] archive 7"),
            transcript.clone(),
        );
        p.demand(
            "duplicate-refused",
            transcript.contains("error loop_case_duplicate: case 5 is already frozen"),
            transcript.clone(),
        );
    });

    // 5. A budget halt prints budget_exhausted; the next step resumes.
    probe.case("repl-budget-halt-resume", |p| {
        let (transcript, _) = drive(&valley_host(), "step 4\nstep 60\nquit\n");
        p.demand(
            "halt-line",
            transcript.contains("batch 1 verdict budget_exhausted"),
            transcript.clone(),
        );
        p.demand(
            "resumed-after-halt",
            transcript.contains("batch 2 verdict"),
            transcript.clone(),
        );
    });

    // 6. Unknown commands and engine faults are named errors; the
    //    session continues. Comments and blank lines are ignored.
    probe.case("repl-errors-continue", |p| {
        let (transcript, verdict) = drive(
            &valley_host(),
            "# annotate\n\nbogus-command\nstep 0\nrun\nquit\n",
        );
        p.demand(
            "unknown-command",
            transcript.contains("error loop_command: unknown command `bogus-command`"),
            transcript.clone(),
        );
        p.demand(
            "engine-fault-passthrough",
            transcript.contains("error bad_budget"),
            transcript.clone(),
        );
        p.demand(
            "session-survived",
            verdict == 1 && transcript.contains("batch 3 verdict goal_attained"),
            transcript.clone(),
        );
    });

    // 7. export-native emits and builds the native epoch host
    //    (bead emath-8k3zw): receipt lines name both crates and the
    //    binary, and the binary exists.
    probe.case("repl-export-native", |p| {
        let out_dir = std::env::temp_dir().join(format!(
            "emath_repl_export_{}_{}",
            std::process::id(),
            line!()
        ));
        let script = format!("export-native {}\nquit\n", out_dir.display());
        let (transcript, _) = drive(&valley_host(), &script);
        p.demand(
            "export-receipt",
            transcript.contains("exported epoch-host") && transcript.contains("built "),
            transcript.clone(),
        );
        let binary = std::fs::read_dir(out_dir.join("epoch-host"))
            .map(|_| true)
            .unwrap_or(false);
        p.demand(
            "host-crate-written",
            binary && transcript.contains(&out_dir.display().to_string()),
            transcript.clone(),
        );
    });

    probe.finish();
}
