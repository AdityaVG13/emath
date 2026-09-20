//! Native epoch export acceptance (bead emath-8k3zw).
//!
//! `export_native` emits two crates: the self-contained artifact crate
//! (the math, typed closure ABI, std-only) and a standalone epoch host
//! bin that drives it over the artifact ABI. The laws pinned here:
//!   - the export emits and compiles: artifact crate with the session
//!     surface, epoch host crate as its sibling, release binary built;
//!   - cross-lane scratch parity: the native lane's `emath.scratch.v1`
//!     checkpoint is byte-identical to the VM lane's for the same
//!     module, surface, and budget schedule;
//!   - the native lane is deterministic: two runs write identical
//!     bytes;
//!   - parity is schedule-sensitive: a different schedule diverges
//!     (the comparison is not a file compared with itself);
//!   - the native transcript carries the same seed/batch/end lines as
//!     the REPL plus a measured timing line - timing is reported,
//!     never claimed as a ratio;
//!   - argument faults are typed refusals, not silent defaults.

use std::path::{Path, PathBuf};
use std::process::Command;

use emath_test_harness::Probe;
use emath_tui::export::{export_native, ExportReport};
use emath_tui::host::LoopHost;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/constructor")
        .join(name)
}

fn valley_host() -> LoopHost {
    LoopHost::open(&fixture_path("research_step_valley.emath"), Some("StepValley"))
        .expect("open valley")
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("emath_export_{label}_{}", std::process::id()))
}

/// The VM lane: step `batches` times at `budget`, save the checkpoint.
fn vm_lane_scratch(host: &LoopHost, batches: usize, budget: i128, path: &Path) {
    let mut session = host.begin().expect("vm begin");
    for _ in 0..batches {
        session.step(host, budget).expect("vm step");
    }
    session.save(host, path).expect("vm save");
}

/// Run the exported binary on a schedule, returning (exit code, stdout,
/// stderr, scratch text).
fn native_lane_run(
    report: &ExportReport,
    batches: u32,
    budget: i64,
    scratch: &Path,
) -> (Option<i32>, String, String, String) {
    let output = Command::new(&report.binary_path)
        .arg("--batches")
        .arg(batches.to_string())
        .arg("--budget")
        .arg(budget.to_string())
        .arg("--scratch")
        .arg(scratch)
        .output()
        .expect("run epoch host bin");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let text = std::fs::read_to_string(scratch).unwrap_or_default();
    (output.status.code(), stdout, stderr, text)
}

#[test]
fn export_native_sessions() {
    let mut probe = Probe::new("native epoch export: artifact ABI host bin with cross-lane parity");

    // 1. The export emits both crates and builds the binary.
    probe.case("export-emits-and-builds", |p| {
        let out_dir = temp_path("out");
        let report = export_native(&valley_host(), &out_dir).expect("export");
        let manifest = std::fs::read_to_string(report.artifact_dir.join("Cargo.toml"))
            .expect("artifact manifest");
        p.demand(
            "artifact-crate",
            manifest.contains("[package]") && manifest.contains("edition"),
            manifest.clone(),
        );
        let lib = std::fs::read_to_string(report.artifact_dir.join("src/lib.rs"))
            .expect("artifact lib");
        p.demand(
            "surface-native",
            lib.contains("pub fn StepValley") && lib.contains("pub fn SeedValley"),
            format!("lib head: {}", &lib[..lib.len().min(400)]),
        );
        let host_main =
            std::fs::read_to_string(report.host_dir.join("src/main.rs")).expect("host main");
        p.demand(
            "host-mirrors-scratch-contract",
            host_main.contains("emath.scratch.v1") && host_main.contains("loop_ledger_law"),
            format!("main head: {}", &host_main[..host_main.len().min(400)]),
        );
        p.demand(
            "binary-built",
            report.binary_path.is_file(),
            report.binary_path.display().to_string(),
        );
    });

    // 2. Cross-lane parity: identical schedules, byte-identical
    //    checkpoints. This is the bead's acceptance.
    probe.case("cross-lane-parity", |p| {
        let report = export_native(&valley_host(), &temp_path("parity")).expect("export");
        let native_path = temp_path("native.json");
        let (code, stdout, stderr, native) =
            native_lane_run(&report, 3, 60, &native_path);
        p.demand(
            "native-run-ok",
            code == Some(0) && stdout.contains("end batches 3 verdict goal_attained"),
            format!("exit {code:?}\n{stdout}\n{stderr}"),
        );
        let vm_path = temp_path("vm.json");
        vm_lane_scratch(&valley_host(), 3, 60, &vm_path);
        let vm = std::fs::read_to_string(&vm_path).expect("vm scratch");
        p.demand(
            "scratch-bytes-equal",
            native == vm,
            format!("--- native ---\n{native}\n--- vm ---\n{vm}"),
        );
    });

    // 3. The native lane is deterministic.
    probe.case("native-determinism", |p| {
        let report = export_native(&valley_host(), &temp_path("det")).expect("export");
        let first_path = temp_path("det1.json");
        let second_path = temp_path("det2.json");
        let (_, _, _, first) = native_lane_run(&report, 3, 60, &first_path);
        let (_, _, _, second) = native_lane_run(&report, 3, 60, &second_path);
        p.demand(
            "identical-runs",
            first == second && !first.is_empty(),
            format!("{first}\nvs\n{second}"),
        );
    });

    // 4. Parity is schedule-sensitive: a shorter VM schedule must NOT
    //    equal the native three-batch scratch (the comparison is real).
    probe.case("parity-is-schedule-sensitive", |p| {
        let report = export_native(&valley_host(), &temp_path("sched")).expect("export");
        let native_path = temp_path("sched_native.json");
        let (_, _, _, native) = native_lane_run(&report, 3, 60, &native_path);
        let vm_path = temp_path("sched_vm.json");
        vm_lane_scratch(&valley_host(), 2, 60, &vm_path);
        let vm = std::fs::read_to_string(&vm_path).expect("vm scratch");
        p.demand(
            "different-schedules-diverge",
            native != vm,
            format!("three-batch native unexpectedly equals two-batch vm\n{native}"),
        );
    });

    // 5. The transcript mirrors the REPL's lines and reports timing.
    probe.case("transcript-and-timing", |p| {
        let report = export_native(&valley_host(), &temp_path("tr")).expect("export");
        let (_, stdout, _, _) =
            native_lane_run(&report, 3, 60, &temp_path("tr.json"));
        p.demand(
            "seed-line",
            stdout.contains("seed batch 0 verdict running used 1 incumbent key 0 score 4/1 mode 1"),
            stdout.clone(),
        );
        p.demand(
            "batch-lines",
            stdout.contains("batch 1 verdict plateau used 6")
                && stdout.contains("batch 3 verdict goal_attained used 33"),
            stdout.clone(),
        );
        p.demand(
            "timing-measured",
            stdout.contains("timing native_step_total_ns "),
            stdout.clone(),
        );
    });

    // 6. Argument faults are typed refusals.
    probe.case("usage-typed-refusals", |p| {
        let report = export_native(&valley_host(), &temp_path("usage")).expect("export");
        let output = Command::new(&report.binary_path).output().expect("run bare bin");
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        p.demand(
            "missing-args-refuse",
            output.status.code() == Some(1) && stderr.contains("error:"),
            format!("exit {:?}\n{stderr}", output.status.code()),
        );
    });

    // 7. The export pairs with THIS session: a module edited on disk
    //    after the host opened refuses by name. The VM lane steps the
    //    open-time admission; an export over edited bytes would pair a
    //    stale meaning id with new math - never a silent re-pairing.
    probe.case("module-changed-refuses-export", |p| {
        let module = temp_path("changed.emath");
        std::fs::copy(fixture_path("research_step_valley.emath"), &module)
            .expect("copy fixture");
        let host = LoopHost::open(&module, Some("StepValley")).expect("open copy");
        let out_dir = temp_path("changed_out");
        let before = export_native(&host, &out_dir);
        p.demand("unchanged-copy-exports", before.is_ok(), format!("{before:?}"));
        let source = std::fs::read_to_string(&module).expect("read copy");
        let edited = source + "\nemath function ChangedProbe:\n    inputs:\n        unused: Int\n    outputs:\n        result: Int\n    definitions:\n        result = 0\n";
        std::fs::write(&module, edited).expect("edit copy");
        let after = export_native(&host, &out_dir);
        let refusal = after.err().map(|fault| (fault.code, fault.message));
        p.demand(
            "changed-module-refuses",
            refusal.as_ref().is_some_and(|(code, message)| {
                code == "loop_export_emit" && message.contains("changed since the session opened")
            }),
            format!("{refusal:?}"),
        );
    });

    // 8. The native lane carries the second real surface: the fitting
    //    target (a local closure bound by a call to an arrow-output
    //    function and applied inside call arguments). Its artifact
    //    must compile and its checkpoint must match the VM lane's.
    probe.case("targets-surface-exports", |p| {
        let host = LoopHost::open(
            &fixture_path("research_step_targets.emath"),
            Some("StepFitting"),
        )
        .expect("open fitting");
        let report = export_native(&host, &temp_path("targets")).expect("export fitting");
        let native_path = temp_path("targets_native.json");
        let (code, stdout, stderr, native) = native_lane_run(&report, 2, 60, &native_path);
        p.demand(
            "native-run-ok",
            code == Some(0) && stdout.contains("end batches 2 verdict goal_attained"),
            format!("exit {code:?}\n{stdout}\n{stderr}"),
        );
        let vm_path = temp_path("targets_vm.json");
        vm_lane_scratch(&host, 2, 60, &vm_path);
        let vm = std::fs::read_to_string(&vm_path).expect("vm scratch");
        p.demand(
            "scratch-bytes-equal",
            native == vm,
            format!("--- native ---\n{native}\n--- vm ---\n{vm}"),
        );
    });

    probe.finish();
}
