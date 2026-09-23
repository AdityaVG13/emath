//! `emath experiment`: build, run and measure an explicit baseline and
//! candidate, then print the authored evaluator's decision.
//!
//! Output is line-oriented and ordered; timings are raw measurements.
//! The checkpoint named on the last line is the machine-readable record.
//! Ctrl-C kills the host and its running child together; the next run
//! with the same `--state` records the interrupted pair as uncertain and
//! reruns it.

use std::path::PathBuf;

use emath_tui::experiment::process::CancelToken;
use emath_tui::experiment::{ExperimentConfig, ExperimentHost, ExperimentStatus, render_value};

use crate::{CliExit, EXIT_OK, EXIT_REFUSED};

pub(crate) const USAGE: &str =
    "experiment <manifest.json> [--state dir] [--audit-ledger path] [--stop-after-pairs N]";

#[derive(Clone, Debug)]
pub(crate) struct ExperimentRequest {
    pub manifest: PathBuf,
    pub state: Option<PathBuf>,
    pub audit_ledger: Option<PathBuf>,
    pub stop_after_pairs: Option<u64>,
}

impl ExperimentRequest {
    pub(crate) fn parse(rest: &[String]) -> Option<Self> {
        let mut manifest = None;
        let mut state = None;
        let mut audit_ledger = None;
        let mut stop_after_pairs = None;
        let mut i = 0;
        while i < rest.len() {
            match rest[i].as_str() {
                "--state" => {
                    i += 1;
                    state = Some(PathBuf::from(rest.get(i)?));
                }
                "--audit-ledger" => {
                    i += 1;
                    audit_ledger = Some(PathBuf::from(rest.get(i)?));
                }
                "--stop-after-pairs" => {
                    i += 1;
                    stop_after_pairs = Some(rest.get(i)?.parse().ok()?);
                }
                other => {
                    if other.starts_with('-') || manifest.is_some() {
                        return None;
                    }
                    manifest = Some(PathBuf::from(other));
                }
            }
            i += 1;
        }
        Some(Self { manifest: manifest?, state, audit_ledger, stop_after_pairs })
    }
}

pub(crate) fn run(request: ExperimentRequest) -> CliExit {
    let host = match ExperimentHost::admit(&request.manifest) {
        Ok(host) => host,
        Err(fault) => {
            eprintln!("error {}: {}", fault.code, fault.message);
            return EXIT_REFUSED;
        }
    };
    let root = PathBuf::from("target").join("emath-experiment");
    let config = ExperimentConfig {
        state_dir: request
            .state
            .unwrap_or_else(|| root.join(&host.manifest().experiment_id)),
        audit_ledger: request.audit_ledger.unwrap_or_else(|| root.join("audit-ledger.json")),
        stop_after_pairs: request.stop_after_pairs,
        cancel: CancelToken::new(),
    };
    println!("experiment {} identity {}", host.manifest().experiment_id, host.identity());
    let (baseline, candidate) = host.source_digests();
    println!("source baseline {baseline}");
    println!("source candidate {candidate}");
    println!("workload {}", host.workload_digest());
    for (key, value) in host.environment() {
        println!("env {key} {}", value.replace('\n', " | "));
    }
    for item in host.enforcement() {
        println!("limit {} {}", item.limit, item.status);
    }
    let report = match host.run(&config) {
        Ok(report) => report,
        Err(fault) => {
            eprintln!("error {}: {}", fault.code, fault.message);
            return EXIT_REFUSED;
        }
    };
    let state = &report.state;
    for build in &state.builds {
        println!(
            "build {} session {} build_ns {} binary {}",
            build.arm, build.session, build.build_ns, build.binary_digest
        );
    }
    for warmup in &state.warmups {
        println!("warmup session {} {} wall_ns {}", warmup.session, warmup.arm, warmup.wall_ns);
    }
    for pair in &state.pairs {
        println!(
            "pair {} {} baseline_ns {} candidate_ns {} session {}",
            pair.index,
            if pair.candidate_first { "candidate_first" } else { "baseline_first" },
            pair.baseline_ns,
            pair.candidate_ns,
            pair.session
        );
    }
    for attempt in &state.attempts {
        println!("attempt {} session {} {} (not counted)", attempt.index, attempt.session, attempt.status);
    }
    println!("outputs_stable {}", state.outputs_stable);
    let exit = match &report.status {
        ExperimentStatus::Decided { replayed } => {
            println!("status decided{}", if *replayed { " (replayed from checkpoint)" } else { "" });
            if let Some(decision) = &state.decision {
                println!("decision {}", render_value(decision));
            }
            EXIT_OK
        }
        ExperimentStatus::Stopped => {
            println!("status stopped (resume with the same --state)");
            EXIT_OK
        }
        ExperimentStatus::Cancelled => {
            println!("status cancelled (resume with the same --state)");
            EXIT_REFUSED
        }
        ExperimentStatus::Failed { arm, kind, detail } => {
            println!("status failed {arm} {kind}: {detail}");
            EXIT_REFUSED
        }
        ExperimentStatus::BudgetExhausted => {
            println!("status budget_exhausted (no decision; measurements are retained)");
            EXIT_REFUSED
        }
    };
    println!("checkpoint {}", report.checkpoint.display());
    println!("audit_ledger {}", config.audit_ledger.display());
    exit
}
