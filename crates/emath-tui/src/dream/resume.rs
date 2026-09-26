//! The resume and fork laws: which journal events a restart at a
//! checkpoint keeps, the ladder it restores, which ended runs may
//! resume, and forking a run at a checkpoint into a new run.

use std::fs;
use std::path::Path;

use emath_artifact::{parse_json_document, JsonValue};

use super::chain;
use super::journal::{complete_lines, io_fault, read_events, JournalEvent};
use super::rundir::{
    json_i128, now_ms, obj_get, pid_alive, prepare_dir, write_atomic, RunDir, RunInfo,
};
use super::DreamLadder;
use crate::host::HostFault;

/// The events a resume or fork at batch `c` keeps: no `end`, and
/// nothing that belongs to a batch after `c`.
pub(super) fn kept_at(events: &[JournalEvent], c: u64) -> Vec<&JournalEvent> {
    events
        .iter()
        .filter(|event| event.t != "end" && event.batch().is_none_or(|batch| batch <= c))
        .collect()
}

/// The ladder of the level `kept` ends in (`kept` starts at that
/// level's `seed` or `level_next`).
pub(super) fn ladder_of(kept: &[&JournalEvent], budget0: i128) -> DreamLadder {
    let seed_watermark = kept
        .iter()
        .find(|e| e.t == "seed" || e.t == "level_next")
        .and_then(|e| e.int("watermark"));
    let watermark = kept
        .iter()
        .rev()
        .find(|e| e.t == "budget")
        .and_then(|e| e.int("to"))
        .or(seed_watermark)
        .unwrap_or(budget0);
    let plateau_streak = kept
        .iter()
        .rev()
        .find(|e| e.t == "batch")
        .and_then(|e| e.uint("plateau_streak"))
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(0);
    let resumes = kept.iter().filter(|e| e.t == "budget").count() as u64;
    DreamLadder {
        watermark,
        plateau_streak,
        resumes,
    }
}

pub(super) fn truncate_metrics(path: &Path, c: u64) -> Result<(), HostFault> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(());
    };
    let mut out = String::new();
    for line in complete_lines(&text) {
        let keep = match parse_json_document(line) {
            Ok(JsonValue::Obj(entries)) => json_i128(obj_get(&entries, "batch"))
                .and_then(|b| u64::try_from(b).ok())
                .is_some_and(|b| b <= c),
            _ => false,
        };
        if keep {
            out.push_str(line);
            out.push('\n');
        }
    }
    write_atomic(path, &out)
}

/// Refuse to resume a run whose driver is alive or whose journal is
/// closed (SPEC §4 `resume`, §6.3).
pub fn check_resumable(run: &RunDir) -> Result<(), HostFault> {
    if let Some(pid) = run.lock_pid() {
        if pid != std::process::id() && pid_alive(pid) {
            return Err(HostFault::fault("dream_run_live", format!("driver pid {pid} is still running this run")));
        }
    }
    resumable(&read_events(&run.journal())?)
}

pub(super) fn resumable(events: &[JournalEvent]) -> Result<(), HostFault> {
    if events.iter().any(|e| e.t == "level") {
        return Err(HostFault::fault("dream_run_closed", "the level closed; a closed run cannot resume (fork it instead)"));
    }
    if let Some(end) = events.iter().rev().find(|e| e.t == "end") {
        let outcome = end.str("outcome").unwrap_or("");
        // A fault `budget_exhausted` is the engine's per-evaluation work
        // limit: a resource bound, and the faulted batch never
        // committed, so the run resumes (with a larger --work).
        let work_fault = outcome == "fault" && end.str("code") == Some("budget_exhausted");
        if !work_fault && !matches!(outcome, "stopped" | "batch_cap" | "budget_exhausted") {
            return Err(HostFault::fault(
                "dream_run_closed",
                format!(
                    "the run ended `{outcome}`; only stopped, batch_cap, budget_exhausted, and \
                     work-limit faulted runs resume"
                ),
            ));
        }
    }
    Ok(())
}

/// Fork `source` at its newest checkpoint at or before `at` (default:
/// its latest checkpoint) into a new run under `root`. Context ceilings
/// may be overridden; the canonical config is the source's.
pub fn fork_run(
    root: &Path,
    source: &RunDir,
    at: Option<u64>,
    name: Option<&str>,
    max_batches: Option<u64>,
    max_units: Option<i128>,
) -> Result<RunDir, HostFault> {
    let info = source.info()?;
    let events = read_events(&source.journal())?;
    let checkpoints = source.list_checkpoints();
    let pick = match at {
        Some(at) => checkpoints.iter().filter(|(b, _, _)| *b <= at).last(),
        None => checkpoints.last(),
    }
    .cloned()
    .ok_or_else(|| {
        HostFault::fault(
            "dream_no_checkpoint",
            format!("run `{}` has no checkpoint at or before the fork point", source.name()),
        )
    })?;
    let (c, path, _) = pick;
    let run = prepare_dir(root, name, &info.target, false)?;
    chain::copy_dir(&source.world(), &run.world())?;
    fs::copy(&path, run.checkpoint(c)).map_err(|err| io_fault("dream_io", &path, &err))?;
    let mut text = String::new();
    for event in kept_at(&events, c) {
        text.push_str(&event.line);
        text.push('\n');
    }
    write_atomic(&run.journal(), &text)?;
    if let Ok(metrics) = fs::read_to_string(source.metrics()) {
        write_atomic(&run.metrics(), &metrics)?;
        truncate_metrics(&run.metrics(), c)?;
    }
    let mut config = info.config.clone();
    if let Some(max_batches) = max_batches {
        config.max_batches = max_batches;
    }
    if max_units.is_some() {
        config.max_units = max_units;
    }
    run.write_info(&RunInfo {
        config,
        target: info.target.clone(),
        argv: vec![format!("fork of {} at b{c}", source.name())],
        created_unix_ms: now_ms(),
        ended: None,
    })?;
    Ok(run)
}

