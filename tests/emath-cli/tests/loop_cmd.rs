//! `emath loop` end-to-end contract (bead emath-q3cqx): the line-REPL
//! command over the host core. A scripted session drives the valley
//! surface to the authored goal; a module with no session surface gets
//! the named lift diagnostic and a refusal exit; argument errors are
//! usage faults.

mod common;

use common::cli;
use emath_cli::{EXIT_OK, EXIT_REFUSED, EXIT_USAGE};
use emath_test_harness::Probe;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/constructor")
        .join(name)
}

fn module_fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/modules")
        .join(name)
}

fn script_file(label: &str, lines: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "emath_loop_{label}_{}.txt",
        std::process::id()
    ));
    std::fs::write(&path, lines).expect("write script");
    path
}

#[test]
fn loop_cmd() {
    let mut probe = Probe::new("emath loop: scripted sessions, lift diagnostics, usage faults");

    // A scripted valley session reaches the authored goal and exits 0.
    let script = script_file(
        "goal",
        "step 60\nstep 60\nstep 60\nquit\n",
    );
    let valley = fixture("research_step_valley.emath");
    let (text, code) = cli(&[
        "loop",
        &valley.display().to_string(),
        "--target",
        "StepValley",
        "--script",
        &script.display().to_string(),
    ]);
    probe.demand(
        "scripted-goal",
        code == EXIT_OK as i32 && text.contains("end batches 3 verdict goal_attained"),
        format!("exit {code}\n{text}"),
    );

    // The default budget applies when a step omits one.
    let script = script_file("default-budget", "step\nstep\nstep\nquit\n");
    let (text, code) = cli(&[
        "loop",
        &valley.display().to_string(),
        "--target",
        "StepValley",
        "--budget",
        "60",
        "--script",
        &script.display().to_string(),
    ]);
    probe.demand(
        "default-budget-goal",
        code == EXIT_OK as i32 && text.contains("end batches 3 verdict goal_attained"),
        format!("exit {code}\n{text}"),
    );

    // A single-surface module chooses its surface without --target.
    let script = script_file("auto", "run 3\nquit\n");
    let (text, code) = cli(&[
        "loop",
        &valley.display().to_string(),
        "--script",
        &script.display().to_string(),
    ]);
    probe.demand(
        "auto-target",
        code == EXIT_OK as i32 && text.contains("target StepValley"),
        format!("exit {code}\n{text}"),
    );

    // A two-surface module without --target refuses with the list.
    let targets = fixture("research_step_targets.emath");
    let (text, code) = cli(&[
        "loop",
        &targets.display().to_string(),
        "--script",
        &script.display().to_string(),
    ]);
    probe.demand(
        "ambiguous-refuses",
        code == EXIT_REFUSED as i32
            && text.contains("StepFitting")
            && text.contains("StepReach"),
        format!("exit {code}\n{text}"),
    );

    // A module with no session surface gets the lift diagnostic.
    let plain = module_fixture("research_loop_valley.emath");
    let (text, code) = cli(&[
        "loop",
        &plain.display().to_string(),
        "--script",
        &script.display().to_string(),
    ]);
    probe.demand(
        "lift-diagnostic",
        code == EXIT_REFUSED as i32
            && text.contains("no Step/Seed session surface")
            && text.contains("research_step_valley.emath"),
        format!("exit {code}\n{text}"),
    );

    // Missing arguments are usage faults.
    let (text, code) = cli(&["loop"]);
    probe.demand(
        "usage-fault",
        code == EXIT_USAGE as i32 && text.contains("emath loop"),
        format!("exit {code}\n{text}"),
    );

    // Piped stdin drives the same session as --script.
    let output = std::process::Command::new(common::emath_bin())
        .arg("loop")
        .arg(&valley)
        .arg("--target")
        .arg("StepValley")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write as _;
            child
                .stdin
                .take()
                .expect("stdin")
                .write_all(b"run 3\nquit\n")?;
            child.wait_with_output()
        })
        .expect("run piped emath loop");
    let piped = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    probe.demand(
        "stdin-session",
        output.status.code() == Some(EXIT_OK as i32)
            && piped.contains("end batches 3 verdict goal_attained"),
        format!("exit {:?}\n{piped}", output.status.code()),
    );

    probe.finish();
}
