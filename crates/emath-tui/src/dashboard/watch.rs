//! Incremental watching of a run directory (SPEC §6.2, §13): the
//! journal, metrics and control files are read from their last byte
//! offset, complete lines only; the RUN badge is derived from the
//! journal, the driver's liveness and the control acks.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use emath_artifact::{parse_json_document, JsonValue};

use super::model::RunModel;
use super::view::RunBadge;
use crate::dream::journal::JournalEvent;
use crate::dream::rundir::{pid_alive, read_acks, read_requests, Ack, Ended, Request, RunDir, RunInfo};

/// Where a file is read from: its identity (device, inode where the
/// platform has one) and the byte offset after the last complete line.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cursor {
    id: Option<(u64, u64)>,
    offset: u64,
}

fn file_id(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some((meta.dev(), meta.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        None
    }
}

/// New complete lines of `path` after the cursor; advances it. Returns
/// `true` with the lines when the file was replaced (the driver rewrites
/// journal and metrics atomically on resume) or shrank: the lines are
/// then the whole new file and the caller must refold from scratch.
/// Identity is read from the opened handle, so a rename between the
/// check and the read cannot pair a stale offset with the new file.
pub fn tail_lines(path: &Path, cursor: &mut Cursor) -> (Vec<String>, bool) {
    let Ok(mut file) = File::open(path) else { return (Vec::new(), false) };
    let Ok(meta) = file.metadata() else { return (Vec::new(), false) };
    let id = file_id(&meta);
    let replaced = (cursor.offset > 0 && cursor.id.is_some() && id != cursor.id) || meta.len() < cursor.offset;
    if replaced {
        cursor.offset = 0;
    }
    cursor.id = id;
    if meta.len() == cursor.offset || file.seek(SeekFrom::Start(cursor.offset)).is_err() {
        return (Vec::new(), replaced);
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return (Vec::new(), replaced);
    }
    let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else { return (Vec::new(), replaced) };
    cursor.offset += end as u64 + 1;
    let lines = String::from_utf8_lossy(&bytes[..end]).lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect();
    (lines, replaced)
}

/// The viewer's live picture of one run directory.
#[derive(Debug)]
pub struct RunWatch {
    pub run: RunDir,
    pub info: Option<RunInfo>,
    pub model: RunModel,
    /// `(batch, wall_ms)` of the newest metrics lines.
    pub metrics: Vec<(u64, u64)>,
    pub acks: Vec<Ack>,
    pub requests: Vec<Request>,
    pub checkpoints: Vec<(u64, String, u64)>,
    /// `None` until the first liveness probe.
    pub driver_alive: Option<bool>,
    /// Seqs ingested by the latest `poll`.
    pub new_seqs: Vec<u64>,
    /// A journal line that failed to parse (shown, never skipped silently).
    pub fault: Option<String>,
    journal: Cursor,
    metrics_cursor: Cursor,
}

impl RunWatch {
    pub fn open(run: RunDir) -> Self {
        let info = run.info().ok();
        RunWatch {
            run,
            info,
            model: RunModel::new(),
            metrics: Vec::new(),
            acks: Vec::new(),
            requests: Vec::new(),
            checkpoints: Vec::new(),
            driver_alive: None,
            new_seqs: Vec::new(),
            fault: None,
            journal: Cursor::default(),
            metrics_cursor: Cursor::default(),
        }
    }

    /// Ingest new journal and metrics lines and re-read the control
    /// files; returns whether anything changed.
    pub fn poll(&mut self) -> bool {
        self.new_seqs.clear();
        let mut changed = false;
        let (lines, replaced) = tail_lines(&self.run.journal(), &mut self.journal);
        if replaced {
            self.model = RunModel::new();
            self.fault = None;
            changed = true;
        }
        for line in lines {
            match JournalEvent::parse(&line) {
                Ok(event) => {
                    self.new_seqs.push(event.seq);
                    self.model.ingest(&event);
                    changed = true;
                }
                Err(fault) => self.fault = Some(format!("{}: {}", fault.code, fault.message)),
            }
        }
        let (lines, replaced) = tail_lines(&self.run.metrics(), &mut self.metrics_cursor);
        if replaced {
            self.metrics.clear();
        }
        for line in lines {
            if let Ok(JsonValue::Obj(entries)) = parse_json_document(&line) {
                let num = |key: &str| {
                    entries.iter().find(|(k, _)| k == key).and_then(|(_, v)| match v {
                        JsonValue::Num(n) => n.parse::<u64>().ok(),
                        _ => None,
                    })
                };
                if let (Some(batch), Some(wall)) = (num("batch"), num("wall_ms")) {
                    self.metrics.push((batch, wall));
                    changed = true;
                }
            }
        }
        if self.metrics.len() > 64 {
            self.metrics.drain(..self.metrics.len() - 64);
        }
        let acks = read_acks(&self.run);
        let requests = read_requests(&self.run);
        if acks.len() != self.acks.len() || requests.len() != self.requests.len() {
            changed = true;
        }
        self.acks = acks;
        self.requests = requests;
        changed
    }

    /// Re-probe the driver's liveness, `run.json` and the checkpoints.
    pub fn refresh(&mut self) {
        self.info = self.run.info().ok();
        self.driver_alive = Some(self.run.lock_pid().is_some_and(pid_alive));
        self.checkpoints = self
            .run
            .list_checkpoints()
            .into_iter()
            .map(|(batch, path, bytes)| (batch, path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), bytes))
            .collect();
    }

    pub fn badge(&self) -> RunBadge {
        derive_badge(&self.model, self.info.as_ref().and_then(|i| i.ended.as_ref()), self.driver_alive, &self.acks, &self.requests)
    }

    /// Batches per second over the newest metrics (a measurement).
    pub fn bps(&self) -> Option<f64> {
        let tail: Vec<&(u64, u64)> = self.metrics.iter().rev().take(20).collect();
        let (newest, oldest) = (tail.first()?, tail.last()?);
        let span = newest.1.checked_sub(oldest.1)?;
        (tail.len() > 1 && span > 0).then(|| (tail.len() - 1) as f64 / (span as f64 / 1000.0))
    }

    /// Driver wall time at the newest metrics line (a measurement).
    pub fn elapsed_secs(&self) -> Option<u64> {
        self.metrics.last().map(|m| m.1 / 1000)
    }
}

/// The RUN tile's state, in SPEC §6.2 precedence.
pub fn derive_badge(model: &RunModel, ended: Option<&Ended>, alive: Option<bool>, acks: &[Ack], requests: &[Request]) -> RunBadge {
    if let Some(end) = &model.end {
        return RunBadge::Ended { outcome: end.outcome.clone(), code: end.code.clone() };
    }
    if let Some(end) = ended {
        return RunBadge::Ended { outcome: end.outcome.clone(), code: end.code.clone() };
    }
    match alive {
        None => return RunBadge::Starting,
        Some(false) => return RunBadge::Stalled,
        Some(true) => {}
    }
    let action_of = |id: &str| requests.iter().find(|r| r.id == id).map(|r| r.action.as_str());
    let applied_acks: Vec<&Ack> = acks.iter().filter(|a| a.outcome == "applied").collect();
    let last_pause = applied_acks.iter().rposition(|a| action_of(&a.id) == Some("pause"));
    let last_resume = applied_acks.iter().rposition(|a| action_of(&a.id) == Some("resume"));
    if let Some(p) = last_pause {
        if last_resume.is_none_or(|r| r < p) {
            let batch = applied_acks.last().map_or(model.last_batch(), |a| a.batch).max(model.last_batch());
            return RunBadge::Paused(batch);
        }
    }
    let acked = |id: &str| acks.iter().any(|a| a.id == id);
    if requests.iter().any(|r| r.action == "stop" && !acked(&r.id)) {
        return RunBadge::Stopping;
    }
    if requests.iter().any(|r| r.action == "pause" && !acked(&r.id)) {
        return RunBadge::Pausing;
    }
    if !model.started {
        return RunBadge::Starting;
    }
    RunBadge::Running
}
