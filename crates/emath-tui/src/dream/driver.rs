//! The headless `emath dream` driver over one run directory, and the
//! detached process that hosts it.
//!
//! The driver adds no loop policy: the module owns acceptance,
//! charging, freshness, promotion, and verdicts; [`DreamRun`] owns the
//! dream laws. This file owns journaling, checkpoints, and control, all
//! applied at batch boundaries only (a batch in flight always commits).
//!
//! Determinism: pausing, resuming, detaching, attaching, checkpoint
//! requests, and viewer count never change `journal.jsonl`; stop +
//! resume and crash + resume reproduce the uninterrupted journal.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use emath_exec_ir::constructor_layer::CValue;

use super::chain::{self, ChainSeam, KeepSeam};
use super::inbox;
use super::journal::{
    diff_archive, io_fault, project_record, read_events, JournalEvent, JournalWriter, Obj, JOURNAL_SCHEMA,
};
use super::ledger::Ledger;
use super::resume::{kept_at, ladder_of, resumable, truncate_metrics};
use super::rundir::{
    append_line, read_acks, read_requests, write_atomic, Ended, Request, RunConfig, RunDir, RunInfo,
};
use super::{declares_emission_seam, DreamConfig, DreamRun};
use crate::host::{state_archive, verdict_name, FactDraw, HostFault, LoopHost, LoopSession};

/// The headless driver over one run directory.
pub struct Driver {
    dir: RunDir,
    info: RunInfo,
    run: DreamRun,
    journal: JournalWriter,
    paused: bool,
    ended: bool,
    acked: BTreeSet<String>,
    started: Instant,
    last_checkpoint: Option<u64>,
    ledger: Option<Ledger>,
    evidence: Vec<CValue>,
    /// Evidence events so far (the next one's `source` is one more).
    sources: i128,
    taken: BTreeSet<String>,
    /// Inbox files a resume dropped, and the boundary that first took them.
    deferred: BTreeMap<String, u64>,
    /// The level being driven (1 = the world copy of the launch module).
    level: u32,
    /// Journal batches committed before this level began.
    base: u64,
    level_path: PathBuf,
    emits: bool,
    chain: Option<ChainSeam>,
    keep: Option<KeepSeam>,
    /// Each name journaled as kept, with the content id last kept under
    /// it (the run library holds that definition).
    kept: BTreeMap<String, String>,
}

/// The incumbent as a journal value (`null` for a plain loop state).
fn incumbent_json(session: &LoopSession) -> Result<String, HostFault> {
    match session.incumbent_record()? {
        Some(record) => project_record(&record),
        None => Ok("null".into()),
    }
}

fn archive_json(session: &LoopSession) -> Result<Vec<String>, HostFault> {
    state_archive(session.state())?.iter().map(project_record).collect()
}

fn config_obj(config: &RunConfig) -> Obj {
    Obj::new()
        .int("budget0", config.budget0)
        .int("budget_step", config.budget_step)
        .int("plateau_close", config.plateau_close)
        .int("max_batches", config.max_batches)
        .opt_int("max_units", config.max_units)
        .int("checkpoint_every", config.checkpoint_every)
}

impl Driver {
    /// Open the driver over `dir`: a fresh journal starts the run; an
    /// existing one resumes it under the resume law.
    pub fn open(dir: &Path) -> Result<Self, HostFault> {
        let dir = RunDir::new(dir);
        let info = dir.info()?;
        info.config.validate()?;
        let events = read_events(&dir.journal())?;
        if events.is_empty() {
            Self::fresh(dir, info)
        } else {
            Self::resume(dir, info, events)
        }
    }

    fn dream_config(dir: &RunDir, info: &RunInfo, emits: bool) -> DreamConfig {
        let mut cfg = info.config.dream();
        if emits {
            cfg.out_dir = Some(dir.level());
        }
        cfg
    }

    fn fresh(dir: RunDir, mut info: RunInfo) -> Result<Self, HostFault> {
        let module = dir.level_one(&info);
        let probe = LoopHost::open(&module, Some(&info.target))?;
        let emits = declares_emission_seam(&probe);
        drop(probe);
        let cfg = Self::dream_config(&dir, &info, emits);
        let run = DreamRun::open(&module, Some(&info.target), &cfg)?;
        let chain = chain::resolve(run.host())?;
        let keep = chain::resolve_keep(run.host())?;
        let ledger = dir.carry_ledger(run.host())?;
        let mut journal = JournalWriter::append(&dir.journal(), 0)?;
        let identity = run.host().identity().clone();
        journal.emit(
            "start",
            Obj::new()
                .str("schema", JOURNAL_SCHEMA)
                .str("meaning_id", &identity.meaning_id)
                .str("language_id", &identity.language_id)
                .str("target", &identity.target)
                .int("level", 1)
                .raw("config", config_obj(&info.config).render())
                .bool("emission", run.emits()),
        )?;
        let session = run.session();
        let archive = archive_json(session)?;
        let incumbent = incumbent_json(session)?;
        journal.emit(
            "seed",
            Obj::new()
                .int("batch", 0)
                .int("used", session.used()?)
                .int("watermark", run.watermark())
                .raw("incumbent", incumbent)
                .array("archive", &archive)
                .ints("case_ids", &session.case_ids()?),
        )?;
        write_atomic(&dir.metrics(), "")?;
        info.ended = None;
        dir.write_info(&info)?;
        let acked = read_acks(&dir).into_iter().map(|ack| ack.id).collect();
        let mut driver = Self {
            dir,
            info,
            run,
            journal,
            paused: false,
            ended: false,
            acked,
            started: Instant::now(),
            last_checkpoint: None,
            ledger,
            evidence: Vec::new(),
            sources: 0,
            taken: BTreeSet::new(),
            deferred: BTreeMap::new(),
            level: 1,
            base: 0,
            level_path: module,
            emits,
            chain,
            keep,
            kept: BTreeMap::new(),
        };
        driver.keep_new(0)?;
        Ok(driver)
    }

    /// Keep what the loop reports that the run library does not hold
    /// yet (a new name, or new content under a kept name) into the run
    /// library, one `kept` event each.
    fn keep_new(&mut self, batch: u64) -> Result<(), HostFault> {
        let Some(seam) = &self.keep else {
            return Ok(());
        };
        let library = self.dir.dir.join(chain::KEPT_LIBRARY);
        let origin = format!("dream level {} batch {batch}", self.level);
        let kept = seam.keep(self.run.host(), self.run.session().state(), &library, &self.kept, &origin)?;
        for (name, content) in kept {
            self.journal.emit(
                "kept",
                Obj::new()
                    .int("batch", batch)
                    .str("name", &name)
                    .str("content", &content)
                    .str("file", chain::KEPT_LIBRARY),
            )?;
            self.kept.insert(name, content);
        }
        Ok(())
    }

    fn resume(dir: RunDir, mut info: RunInfo, events: Vec<JournalEvent>) -> Result<Self, HostFault> {
        resumable(&events)?;
        let last_batch = events.iter().filter(|e| e.t == "batch").filter_map(JournalEvent::batch).max().unwrap_or(0);
        let checkpoint = dir.list_checkpoints().into_iter().filter(|(b, _, _)| *b <= last_batch).last();
        let c = checkpoint.as_ref().map_or(0, |(b, _, _)| *b);
        let kept = kept_at(&events, c);
        // The level in force at `c`: the last kept `level_next`, else level 1.
        let level_at = kept.iter().rposition(|e| e.t == "level_next");
        let (level, base, level_path) = match level_at.map(|i| kept[i]) {
            Some(e) => (
                e.uint("level").and_then(|l| u32::try_from(l).ok()).unwrap_or(1),
                e.batch().unwrap_or(0),
                dir.dir.join(e.str("file").unwrap_or_default()),
            ),
            None => (1, 0, dir.level_one(&info)),
        };
        let ladder = ladder_of(&kept[level_at.unwrap_or(0)..], info.config.budget0);
        let emits = events.first().and_then(|e| e.bool("emission")).unwrap_or(false);
        let mut text = String::new();
        for event in &kept {
            text.push_str(&event.line);
            text.push('\n');
        }
        let last_seq = kept.last().map_or(0, |e| e.seq);
        let steers: Vec<i128> = kept
            .iter()
            .filter(|e| e.t == "steer" && e.batch() == Some(c) && e.str("action") == Some("grow_case"))
            .filter_map(|e| e.int("case_id"))
            .collect();
        // A resumable journal holds no `level` event, so a level file on
        // disk belongs to a close that crashed before it was journaled:
        // it is scratch output of an uncommitted batch, re-emitted below.
        let stale_level = dir.level().join("level_001.emath");
        if stale_level.exists() {
            fs::remove_file(&stale_level).map_err(|err| io_fault("dream_io", &stale_level, &err))?;
        }
        let cfg = Self::dream_config(&dir, &info, emits);
        // A level that began at `c` has committed nothing yet: its start
        // is its Seed, and the checkpoint at `c` belongs to the level before.
        let mut run = match &checkpoint {
            Some((_, path, _)) if !(level > 1 && c == base) => {
                DreamRun::restore(&level_path, Some(&info.target), &cfg, path, ladder)?
            }
            _ => DreamRun::open(&level_path, Some(&info.target), &cfg)?,
        };
        let chain = chain::resolve(run.host())?;
        let keep = chain::resolve_keep(run.host())?;
        let kept_names: BTreeMap<String, String> = kept
            .iter()
            .filter(|e| e.t == "kept")
            .filter_map(|e| Some((e.str("name")?.to_string(), e.str("content")?.to_string())))
            .collect();
        let ledger = dir.carry_ledger(run.host())?;
        let (evidence, taken) = inbox::replay(&kept)?;
        let sources = kept.iter().filter(|e| e.t == "evidence").count() as i128;
        // Files the dropped tail took wait for the boundary that took
        // them, so stop + resume reproduces the uninterrupted journal.
        let deferred: BTreeMap<String, u64> = events
            .iter()
            .filter(|e| matches!(e.t.as_str(), "evidence" | "evidence_rejected"))
            .filter_map(|e| Some((e.str("file")?.to_string(), e.batch()?)))
            .filter(|(_, batch)| *batch > c)
            .collect();
        run.host_mut().set_evidence(evidence.clone());
        // Steering applied at batch `c` after its checkpoint was written
        // is journal data the checkpoint does not carry: re-apply it.
        for case_id in steers {
            if !run.session().case_ids()?.contains(&case_id) {
                run.session_mut().grow_case(case_id)?;
            }
        }
        write_atomic(&dir.journal(), &text)?;
        truncate_metrics(&dir.metrics(), c)?;
        let journal = JournalWriter::append(&dir.journal(), last_seq)?;
        info.ended = None;
        dir.write_info(&info)?;
        let acked = read_acks(&dir).into_iter().map(|ack| ack.id).collect();
        Ok(Self {
            dir,
            info,
            run,
            journal,
            paused: false,
            ended: false,
            acked,
            started: Instant::now(),
            last_checkpoint: checkpoint.map(|(b, _, _)| b),
            ledger,
            evidence,
            sources,
            taken,
            deferred,
            level,
            base,
            level_path,
            emits,
            chain,
            keep,
            kept: kept_names,
        })
    }

    /// The journal batch: committed batches over every level.
    fn batch(&self) -> u64 {
        self.base + self.run.batches()
    }

    /// At a close, write the next level from `dream_next` and continue
    /// on it once it verifies. `false` when the level closes the run:
    /// no seam, a fixed point, or a rejected file (journaled).
    fn next_level(&mut self, batch: u64) -> Result<bool, HostFault> {
        let Some(seam) = &self.chain else {
            return Ok(false);
        };
        let (carry, next) = seam.values(self.run.host(), self.run.session().state())?;
        if carry == next {
            return Ok(false);
        }
        let level = self.level + 1;
        let file = format!("world/level_{level:03}.emath");
        let path = self.dir.dir.join(&file);
        let written = chain::literal(&next).and_then(|literal| {
            let source = fs::read_to_string(&self.level_path).map_err(|err| err.to_string())?;
            let text = seam.rewrite(&source, &literal)?;
            Ok((literal, text))
        });
        let verdict = match written {
            Ok((literal, text)) => {
                fs::create_dir_all(self.dir.world()).map_err(|err| io_fault("dream_io", &self.dir.world(), &err))?;
                write_atomic(&path, &text)?;
                chain::verify(&path, &self.info.target, self.info.config.work).map(|()| literal)
            }
            Err(reason) => Err(reason),
        };
        let literal = match verdict {
            Ok(literal) => literal,
            Err(reason) => {
                self.journal.emit(
                    "level_rejected",
                    Obj::new().int("batch", batch).int("level", level).str("file", &file).str("reason", &reason),
                )?;
                return Ok(false);
            }
        };
        self.flush_facts(batch)?;
        let cfg = Self::dream_config(&self.dir, &self.info, self.emits);
        let mut run = DreamRun::open(&path, Some(&self.info.target), &cfg)?;
        run.host_mut().set_evidence(self.evidence.clone());
        self.chain = chain::resolve(run.host())?;
        self.keep = chain::resolve_keep(run.host())?;
        self.ledger = self.dir.carry_ledger(run.host())?;
        self.run = run;
        self.level = level;
        self.base = batch;
        self.level_path = path;
        let session = self.run.session();
        let archive = archive_json(session)?;
        let incumbent = incumbent_json(session)?;
        let used = session.used()?;
        let case_ids = session.case_ids()?;
        self.journal.emit(
            "level_next",
            Obj::new()
                .int("batch", batch)
                .int("level", level)
                .str("file", &file)
                .str("carry", &literal)
                .int("used", used)
                .int("watermark", self.run.watermark())
                .raw("incumbent", incumbent)
                .array("archive", &archive)
                .ints("case_ids", &case_ids),
        )?;
        self.keep_new(batch)?;
        Ok(true)
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    #[must_use]
    pub fn is_ended(&self) -> bool {
        self.ended
    }

    /// Committed batches.
    #[must_use]
    pub fn batches(&self) -> u64 {
        self.batch()
    }

    /// The run directory.
    #[must_use]
    pub fn dir(&self) -> &RunDir {
        &self.dir
    }

    /// Write a checkpoint at the current batch (atomic), then apply the
    /// retention law (newest `keep_checkpoints`).
    pub fn checkpoint(&mut self) -> Result<(), HostFault> {
        let batch = self.batch();
        let path = self.dir.checkpoint(batch);
        if self.last_checkpoint == Some(batch) && path.exists() {
            return Ok(());
        }
        fs::create_dir_all(self.dir.checkpoints()).map_err(|err| io_fault("dream_io", &self.dir.checkpoints(), &err))?;
        let tmp = path.with_extension("scratch.tmp");
        self.run.session().save(self.run.host(), &tmp)?;
        fs::rename(&tmp, &path).map_err(|err| io_fault("dream_io", &path, &err))?;
        self.last_checkpoint = Some(batch);
        let all = self.dir.list_checkpoints();
        let keep = self.info.config.keep_checkpoints;
        if all.len() > keep {
            for (_, old, _) in &all[..all.len() - keep] {
                // Retention: run output is scratch data.
                let _ = fs::remove_file(old);
            }
        }
        Ok(())
    }

    /// Append the facts computed since the last flush (labelled with
    /// `batch`) and carry what sibling drivers appended meanwhile.
    fn flush_facts(&mut self, batch: u64) -> Result<FactDraw, HostFault> {
        let facts = self.run.host().take_facts();
        if let Some(ledger) = &mut self.ledger {
            let siblings = ledger.append(&facts.fresh, &self.dir.name(), batch)?;
            self.run.host().carry_facts(siblings);
        }
        Ok(facts)
    }

    /// Carry what sibling drivers appended since the last flush.
    fn pull_facts(&mut self) -> Result<(), HostFault> {
        if let Some(ledger) = &mut self.ledger {
            let siblings = ledger.sync()?;
            self.run.host().carry_facts(siblings);
        }
        Ok(())
    }

    /// Flush on the way out of the driver, whatever ended it.
    fn flush_on_exit(&mut self) -> Result<(), HostFault> {
        self.flush_facts(self.batch() + 1).map(|_| ())
    }

    fn end(&mut self, outcome: &str, code: Option<&str>, message: String) -> Result<(), HostFault> {
        if self.ended {
            return Ok(());
        }
        let flushed = self.flush_facts(self.batch());
        self.checkpoint()?;
        self.journal.emit(
            "end",
            Obj::new()
                .int("batch", self.batch())
                .str("outcome", outcome)
                .opt_str("code", code),
        )?;
        self.info.ended = Some(Ended {
            outcome: outcome.into(),
            code: code.map(str::to_string),
            message,
        });
        self.dir.write_info(&self.info)?;
        self.ended = true;
        flushed.map(|_| ())
    }

    fn end_fault(&mut self, fault: &HostFault) -> Result<(), HostFault> {
        self.end("fault", Some(&fault.code), fault.message.clone())
    }

    /// Take every ready inbox file, journal it, and bind the grown
    /// evidence to the Step. Runs at a batch boundary, never mid-batch.
    fn take_inbox(&mut self) -> Result<(), HostFault> {
        let batch = self.batch();
        let files: Vec<_> = inbox::pending(&self.dir.inbox(), &self.taken)?
            .into_iter()
            .filter(|file| self.deferred.get(&file.name).is_none_or(|at| *at <= batch))
            .collect();
        if files.is_empty() {
            return Ok(());
        }
        let takes_evidence = self.run.host().takes_evidence();
        for file in files {
            self.taken.insert(file.name.clone());
            let source = self.sources + 1;
            let parsed = if takes_evidence {
                inbox::parse_file(&file.text, source)
            } else {
                Err("the Step declares no evidence input".to_string())
            };
            match parsed {
                Ok(observations) => {
                    let rendered = inbox::render(&observations)?;
                    self.journal.emit(
                        "evidence",
                        Obj::new()
                            .int("batch", batch)
                            .str("file", &file.name)
                            .int("source", source)
                            .array("observations", &rendered),
                    )?;
                    self.evidence.extend(observations);
                    self.sources = source;
                }
                Err(reason) => {
                    self.journal.emit(
                        "evidence_rejected",
                        Obj::new().int("batch", batch).str("file", &file.name).str("reason", &reason),
                    )?;
                }
            }
        }
        self.run.host_mut().set_evidence(self.evidence.clone());
        Ok(())
    }

    /// Drive exactly one batch and journal it (or end the run when the
    /// batch cap is reached).
    pub fn run_batch(&mut self) -> Result<(), HostFault> {
        if self.ended {
            return Ok(());
        }
        if self.batch() >= self.info.config.max_batches {
            let message = format!("stopped at the batch cap ({} batches)", self.info.config.max_batches);
            return self.end("batch_cap", None, message);
        }
        self.take_inbox()?;
        let prev_archive = state_archive(self.run.session().state())?;
        let prev_cases = self.run.session().case_ids()?;
        let prev_used = self.run.session().used()?;
        let watermark = self.run.watermark();
        self.pull_facts()?;
        let began = Instant::now();
        let outcome = match self.run.advance() {
            Ok(outcome) => outcome,
            Err(fault) => {
                // Facts the faulted batch completed are still true.
                self.flush_facts(self.batch() + 1)?;
                return self.end_fault(&fault);
            }
        };
        let batch_ms = began.elapsed().as_millis();
        let entry = &outcome.entry;
        let batch = self.base + entry.batch;
        let session = self.run.session();
        let next_archive = state_archive(session.state())?;
        let diff = diff_archive(&prev_archive, &next_archive)?;
        if !diff.is_empty() {
            self.journal.emit(
                "records",
                Obj::new().int("batch", batch).array("added", &diff.added).array("changed", &diff.changed),
            )?;
        }
        let cases = session.case_ids()?;
        let grown: Vec<i128> = cases.iter().filter(|c| !prev_cases.contains(c)).copied().collect();
        if !grown.is_empty() {
            self.journal.emit("cases", Obj::new().int("batch", batch).ints("added", &grown))?;
        }
        let incumbent = incumbent_json(session)?;
        let seq = self.journal.emit(
            "batch",
            Obj::new()
                .int("batch", batch)
                .int("verdict", entry.verdict)
                .str("verdict_name", verdict_name(entry.verdict))
                .int("used", entry.used)
                .int("delta_used", entry.used - prev_used)
                .int("watermark", watermark)
                .bool("halted", entry.halted)
                .int("plateau_streak", outcome.plateau_streak)
                .raw("incumbent", incumbent)
                .ints("promoted", &entry.promoted)
                .ints("quarantined", &entry.quarantined)
                .ints("case_ids", &entry.case_ids)
                .int("archive_len", entry.archive_len),
        )?;
        if let Err(fault) = self.keep_new(batch) {
            return self.end_fault(&fault);
        }
        let facts = self.flush_facts(batch)?;
        let mut metrics = Obj::new()
            .int("batch", batch)
            .int("seq", seq)
            .int("wall_ms", self.started.elapsed().as_millis())
            .int("batch_ms", batch_ms);
        metrics = if self.ledger.is_some() {
            metrics
                .int("cache_lookups", facts.hits + facts.fresh.len() as u64)
                .int("cache_hits", facts.hits)
                .int("computed_units", facts.fresh.len())
        } else {
            metrics.null("cache_lookups").null("cache_hits").null("computed_units")
        };
        let metrics = metrics.render();
        append_line(&self.dir.metrics(), &metrics)?;
        if let Some((from, to)) = outcome.raised {
            if let Some(ceiling) = self.info.config.max_units {
                if to > ceiling {
                    let message = format!("the resume law's raise to {to} units would pass the {ceiling}-unit ceiling");
                    return self.end("budget_exhausted", None, message);
                }
            }
            self.journal.emit("budget", Obj::new().int("batch", batch).int("from", from).int("to", to))?;
        }
        if batch % self.info.config.checkpoint_every == 0 {
            self.checkpoint()?;
        }
        if let Some(reason) = outcome.close {
            if self.next_level(batch)? {
                return Ok(());
            }
            let finished = match self.run.finish() {
                Ok(finished) => finished,
                Err(fault) => return self.end_fault(&fault),
            };
            let Some(level) = finished.level else {
                return self.end_fault(&HostFault::fault("dream_close", "the level closed without a level record"));
            };
            let incumbent = incumbent_json(self.run.session())?;
            self.journal.emit(
                "level",
                Obj::new()
                    .int("batch", batch)
                    .str("reason", reason)
                    .raw("incumbent", incumbent)
                    .ints("case_ids", &level.case_ids)
                    .int("used", level.used)
                    .int("batches", level.batches)
                    .opt_str("file", level.path.as_ref().map(|_| "level/level_001.emath"))
                    .bool("certificate", level.certificate),
            )?;
            let message = format!("level closed: {reason}");
            return self.end(reason, None, message);
        }
        Ok(())
    }

    fn apply(&mut self, request: &Request) -> Result<(String, Option<String>), HostFault> {
        let applied = || Ok(("applied".to_string(), None));
        let refused = |code: &str| Ok(("refused".to_string(), Some(code.to_string())));
        if self.ended {
            return refused("dream_run_ended");
        }
        match request.action.as_str() {
            "pause" => {
                if !self.paused {
                    self.paused = true;
                    self.checkpoint()?;
                }
                applied()
            }
            "resume" => {
                self.paused = false;
                applied()
            }
            "step" => {
                if !self.paused {
                    return refused("dream_not_paused");
                }
                self.run_batch()?;
                applied()
            }
            "grow_case" => {
                let Some(case_id) = request.arg else {
                    return refused("dream_control_arg");
                };
                match self.run.session_mut().grow_case(case_id) {
                    Ok(()) => {
                        self.journal.emit(
                            "steer",
                            Obj::new()
                                .int("batch", self.batch())
                                .str("action", "grow_case")
                                .int("case_id", case_id),
                        )?;
                        applied()
                    }
                    Err(fault) => refused(&fault.code),
                }
            }
            "stop" => {
                let message = format!("stopped by request at batch {}", self.batch());
                self.end("stopped", None, message)?;
                applied()
            }
            "checkpoint" => {
                self.checkpoint()?;
                applied()
            }
            _ => refused("dream_control_unknown"),
        }
    }

    /// Apply every unacknowledged control request, in file order.
    pub fn poll_control(&mut self) -> Result<(), HostFault> {
        for request in read_requests(&self.dir) {
            if request.id.is_empty() || self.acked.contains(&request.id) {
                continue;
            }
            let (outcome, code) = self.apply(&request)?;
            let line = Obj::new()
                .str("id", &request.id)
                .str("outcome", &outcome)
                .int("batch", self.batch())
                .opt_str("code", code.as_deref())
                .render();
            append_line(&self.dir.acks(), &line)?;
            self.acked.insert(request.id);
        }
        Ok(())
    }

    /// Drive until the run ends: control at every boundary, 100 ms
    /// polls while paused.
    pub fn run(&mut self) -> Result<(), HostFault> {
        loop {
            self.poll_control()?;
            if self.ended {
                return Ok(());
            }
            if self.paused {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            self.run_batch()?;
        }
    }
}

/// The driver process body (`emath dream __drive <dir>`): lock, open,
/// run, unlock. A fault after the journal started is journaled as
/// `end fault`; every fault is recorded in `run.json`.
pub fn drive(dir: &Path) -> Result<(), HostFault> {
    let run = RunDir::new(dir);
    let path = run.lock();
    let io = |err: std::io::Error| io_fault("dream_io", &path, &err);
    // The OS lock, not the pid text, decides liveness: it is released
    // when the driver dies, however it dies. The pid is for viewers.
    let mut lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path).map_err(io)?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            let pid = run.lock_pid().map_or_else(|| "unknown".to_string(), |pid| pid.to_string());
            return Err(HostFault::fault(
                "dream_run_live",
                format!("run `{}` already has a live driver (pid {pid})", run.name()),
            ));
        }
        Err(std::fs::TryLockError::Error(err)) => return Err(io(err)),
    }
    lock.set_len(0).map_err(io)?;
    lock.write_all(std::process::id().to_string().as_bytes()).map_err(io)?;
    let result = Driver::open(dir).and_then(|mut driver| {
        let ran = driver.run();
        let flushed = driver.flush_on_exit();
        ran.and(flushed)
    });
    if let Err(fault) = &result {
        record_fault(&run, fault);
    }
    let _ = fs::remove_file(&path);
    drop(lock);
    result
}

fn record_fault(run: &RunDir, fault: &HostFault) {
    if let Ok(events) = read_events(&run.journal()) {
        if let Some(last) = events.last() {
            if last.t != "end" {
                let batch = events.iter().filter(|e| e.t == "batch").filter_map(JournalEvent::batch).max().unwrap_or(0);
                if let Ok(mut journal) = JournalWriter::append(&run.journal(), last.seq) {
                    let _ = journal.emit(
                        "end",
                        Obj::new().int("batch", batch).str("outcome", "fault").opt_str("code", Some(&fault.code)),
                    );
                }
            }
        }
    }
    if let Ok(mut info) = run.info() {
        info.ended = Some(Ended {
            outcome: "fault".into(),
            code: Some(fault.code.clone()),
            message: fault.message.clone(),
        });
        let _ = run.write_info(&info);
    }
}

/// Start the driver as a detached process of `exe` (`emath dream __drive
/// <dir>`), stdout and stderr to `driver.log`, in its own process group
/// so the viewer's signals never reach it.
pub fn spawn_driver(exe: &Path, run: &RunDir) -> Result<u32, HostFault> {
    let log = File::create(run.log()).map_err(|err| io_fault("dream_io", &run.log(), &err))?;
    let err_log = log.try_clone().map_err(|err| io_fault("dream_io", &run.log(), &err))?;
    let mut command = Command::new(exe);
    command
        .arg("dream")
        .arg("__drive")
        .arg(&run.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err_log));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command.spawn().map_err(|err| io_fault("dream_spawn", exe, &err))?;
    Ok(child.id())
}

