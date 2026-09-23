//! Experiment host acceptance: real builds, real processes, real clocks,
//! authored decisions (examples/hotpath-experiment).
//!
//! Timing-dependent assertions use a workload where the baseline (trial
//! division to 10^6) is tens of milliseconds slower than the candidate
//! (one sieve); every other assertion is about host facts, not speed.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use emath_tui::experiment::process::CancelToken;
use emath_tui::experiment::{
    ExperimentConfig, ExperimentHost, ExperimentReport, ExperimentStatus, candidate_first, read_ledger,
};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repo root")
}

fn example(name: &str) -> String {
    repo().join("examples/hotpath-experiment").join(name).display().to_string()
}

fn hotpath_evaluator() -> (String, &'static str) {
    (
        repo().join("language/templates/research_targets/rust_hotpath.emath").display().to_string(),
        "HotpathEvaluation",
    )
}

struct Lab {
    dir: PathBuf,
}

impl Lab {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "emath_experiment_{label}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("lab dir");
        std::fs::write(dir.join("workload.txt"), "1000000\n").expect("workload");
        Self { dir }
    }

    fn manifest(&self, name: &str, candidate: &str, pairs: u64, trial_ms: u64, evaluator: (String, &str)) -> PathBuf {
        let path = self.dir.join(format!("{name}.json"));
        let text = format!(
            r#"{{"schema": "emath.experiment.v1", "experiment_id": {name:?},
  "baseline": {{"crate": {baseline:?}}}, "candidate": {{"crate": {candidate:?}}},
  "workload": "workload.txt",
  "evaluator": {{"module": {module:?}, "function": {function:?}}},
  "measurement": {{"pairs": {pairs}, "warmup": 1, "seed": 7}},
  "budget": {{"experiment_ms": 240000, "build_ms": 120000, "trial_ms": {trial_ms}, "output_bytes": 4096}},
  "access": {{"network": false, "filesystem_write": false, "require_isolation": false}}}}"#,
            baseline = example("baseline"),
            candidate = example(candidate),
            module = evaluator.0,
            function = evaluator.1,
        );
        std::fs::write(&path, text).expect("manifest");
        path
    }

    fn config(&self, state: &str) -> ExperimentConfig {
        ExperimentConfig {
            state_dir: self.dir.join(state),
            audit_ledger: self.dir.join("audit-ledger.json"),
            stop_after_pairs: None,
            cancel: CancelToken::new(),
        }
    }
}

fn run(manifest: &Path, config: &ExperimentConfig) -> ExperimentReport {
    ExperimentHost::admit(manifest)
        .expect("admit")
        .run(config)
        .expect("run")
}

#[test]
fn real_hotpath_experiment_decides_then_replays() {
    let lab = Lab::new("real");
    let manifest = lab.manifest("real", "candidate", 4, 10_000, hotpath_evaluator());
    let config = lab.config("state");
    let report = run(&manifest, &config);
    assert_eq!(report.status, ExperimentStatus::Decided { replayed: false });
    let state = &report.state;
    assert_eq!(state.pairs.iter().map(|p| p.index).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    for pair in &state.pairs {
        assert_eq!(pair.candidate_first, candidate_first(7, pair.index), "recorded order is the seeded schedule");
        assert!(pair.baseline_ns > 0 && pair.candidate_ns > 0);
    }
    assert_eq!(state.warmups.len(), 2, "one warmup per arm, recorded, not sampled");
    assert_eq!(state.builds.iter().map(|b| b.arm.as_str()).collect::<Vec<_>>(), vec!["baseline", "candidate"]);
    assert!(state.builds.iter().all(|b| b.build_ns > 0 && b.binary_digest.len() == 64));
    assert_eq!(state.baseline_outputs.as_deref(), Some(&[78498][..]));
    assert_eq!(state.baseline_outputs, state.candidate_outputs);
    assert!(state.outputs_stable);
    assert_eq!(report.decision_status(), Some(1), "correct and faster in every pair: {:?}", state.pairs);
    let ledger = read_ledger(&config.audit_ledger).expect("ledger");
    assert_eq!(ledger.len(), 1);
    assert_eq!(ledger[0].state, "consumed");

    let again = run(&manifest, &config);
    assert_eq!(again.status, ExperimentStatus::Decided { replayed: true });
    assert_eq!(again.state.pairs, state.pairs, "a replay adds no measurements");
    assert_eq!(again.decision_status(), Some(1));
}

#[test]
fn faster_wrong_candidate_is_rejected() {
    let lab = Lab::new("wrong");
    let manifest = lab.manifest("wrong", "wrong_candidate", 2, 10_000, hotpath_evaluator());
    let report = run(&manifest, &lab.config("state"));
    assert_eq!(report.status, ExperimentStatus::Decided { replayed: false });
    assert_eq!(report.state.candidate_outputs.as_deref(), Some(&[78499][..]));
    assert!(report.state.pairs.iter().all(|p| p.candidate_ns < p.baseline_ns), "it really is faster");
    assert_eq!(report.decision_status(), Some(-1));
}

#[test]
fn audit_reuse_a_b_a_is_never_fresh() {
    let lab = Lab::new("aba");
    let a = lab.manifest("a", "candidate", 2, 10_000, hotpath_evaluator());
    let b = lab.manifest("b", "wrong_candidate", 2, 10_000, hotpath_evaluator());
    let first = run(&a, &lab.config("a1"));
    assert_eq!(first.decision_status(), Some(1));
    let audit = ExperimentHost::admit(&a).expect("admit").audit_id();
    let second = run(&b, &lab.config("b"));
    assert_eq!(second.decision_status(), Some(-1));
    let third = run(&a, &lab.config("a2"));
    assert_eq!(third.decision_status(), Some(-1), "the same audit consumed earlier cannot promote again");
    let ledger = read_ledger(&lab.dir.join("audit-ledger.json")).expect("ledger");
    assert!(ledger.iter().all(|e| e.audit_id == audit && e.state == "consumed"));
    assert_eq!(ledger.len(), 3, "every evaluation of the audit is recorded");
}

#[test]
fn timeout_is_a_failure_not_a_decision() {
    let lab = Lab::new("timeout");
    let manifest = lab.manifest("timeout", "hang_candidate", 2, 400, hotpath_evaluator());
    let config = lab.config("state");
    let report = run(&manifest, &config);
    match &report.status {
        ExperimentStatus::Failed { arm, kind, .. } => {
            assert_eq!((arm.as_str(), kind.as_str()), ("candidate", "timeout"));
        }
        other => panic!("expected a timeout failure, got {other:?}"),
    }
    assert!(report.state.decision.is_none());
    assert!(report.state.pairs.is_empty());
    assert!(!config.audit_ledger.exists(), "no evaluation, so no audit consumed");
    let resumed = run(&manifest, &config);
    assert_eq!(resumed.status, report.status, "a recorded failure is terminal, not retried silently");
}

#[test]
fn cancellation_kills_the_running_child() {
    let lab = Lab::new("cancel");
    let manifest = lab.manifest("cancel", "hang_candidate", 2, 120_000, hotpath_evaluator());
    let host = ExperimentHost::admit(&manifest).expect("admit");
    let config = lab.config("state");
    let cancel = config.cancel.clone();
    let checkpoint = config.state_dir.join("checkpoint.scratch.json");
    let watcher = std::thread::spawn(move || {
        // Cancel once both builds are committed (the host is running the
        // hanging candidate), not while cargo is still compiling.
        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline {
            if std::fs::read_to_string(&checkpoint).is_ok_and(|t| t.matches("\"ArmBuild\"").count() >= 2) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        std::thread::sleep(Duration::from_millis(300));
        cancel.cancel();
        Instant::now()
    });
    let report = host.run(&config).expect("run");
    let cancelled_at = watcher.join().expect("watcher");
    assert_eq!(report.status, ExperimentStatus::Cancelled);
    assert!(cancelled_at.elapsed() < Duration::from_secs(5), "the child was killed promptly, not waited out");
    assert!(report.state.decision.is_none());
    assert!(report.state.pairs.is_empty());
}

#[test]
fn interrupted_resume_counts_each_pair_once() {
    let lab = Lab::new("resume");
    let manifest = lab.manifest("resume", "candidate", 4, 10_000, hotpath_evaluator());
    let mut config = lab.config("state");
    config.stop_after_pairs = Some(1);
    let stopped = run(&manifest, &config);
    assert_eq!(stopped.status, ExperimentStatus::Stopped);
    assert_eq!(stopped.state.pairs.len(), 1);

    // Simulate a host that died after launching pair 1 but before
    // committing it: the journal says pending, no measurement exists.
    let checkpoint = config.state_dir.join("checkpoint.scratch.json");
    let text = std::fs::read_to_string(&checkpoint).expect("checkpoint");
    assert!(text.contains("\"pending\": -1"));
    std::fs::write(&checkpoint, text.replace("\"pending\": -1", "\"pending\": 1")).expect("inject pending");

    config.stop_after_pairs = None;
    let resumed = run(&manifest, &config);
    assert_eq!(resumed.status, ExperimentStatus::Decided { replayed: false });
    let state = &resumed.state;
    assert_eq!(state.pairs.iter().map(|p| p.index).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    assert_eq!(state.pairs[0].session, 1);
    assert!(state.pairs[1..].iter().all(|p| p.session == 2));
    assert_eq!(state.attempts.len(), 1);
    assert_eq!((state.attempts[0].index, state.attempts[0].status.as_str()), (1, "uncertain"));
    assert_eq!(state.warmups.len(), 4, "each session warms up again and records it");
}

#[test]
fn changed_workload_or_evaluator_cannot_resume() {
    let lab = Lab::new("identity");
    let manifest = lab.manifest("identity", "candidate", 3, 10_000, hotpath_evaluator());
    let mut config = lab.config("state");
    config.stop_after_pairs = Some(1);
    assert_eq!(run(&manifest, &config).status, ExperimentStatus::Stopped);

    let other_evaluator = (
        repo().join("tests/fixtures/constructor/experiment_latency_only.emath").display().to_string(),
        "LatencyOnly",
    );
    let swapped = lab.manifest("identity", "candidate", 3, 10_000, other_evaluator);
    let err = ExperimentHost::admit(&swapped).expect("admit").run(&config).expect_err("evaluator swap");
    assert_eq!(err.code, "experiment_identity");

    let restored = lab.manifest("identity", "candidate", 3, 10_000, hotpath_evaluator());
    std::fs::write(lab.dir.join("workload.txt"), "900000\n").expect("edit workload");
    let err = ExperimentHost::admit(&restored).expect("admit").run(&config).expect_err("workload swap");
    assert_eq!(err.code, "experiment_identity");
}

#[test]
fn evaluator_inputs_must_be_host_facts() {
    let lab = Lab::new("facts");
    let (module, _) = hotpath_evaluator();
    // HotpathDecision takes `reference_outputs` and supplied observations:
    // the host has no such facts and must not fill them with Absent.
    let manifest = lab.manifest("facts", "candidate", 1, 10_000, (module, "HotpathDecision"));
    let err = ExperimentHost::admit(&manifest).expect_err("unknown input");
    assert_eq!(err.code, "experiment_evaluator");
    assert!(err.message.contains("reference_outputs"));
}
