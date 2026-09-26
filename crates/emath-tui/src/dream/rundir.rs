//! The `emath dream` run directory: the headless driver process, its
//! control channel, checkpoints, the resume and fork laws, export, and
//! listing.
//!
//! Layout (`.emath/runs/<name>/`): `journal.jsonl` (canonical),
//! `metrics.jsonl` (measurement), `run.json` (context),
//! `control/requests.jsonl` + `control/acks.jsonl` (context),
//! `driver.lock` / `driver.log` (context), `checkpoints/bNNNNNN.scratch`
//! (canonical bytes), `level/level_001.emath` (canonical bytes),
//! `world/` (the launch module and its local imports, copied at
//! creation, plus `level_NNN.emath` from the level chain; canonical
//! bytes), `inbox/*.jsonl` (evidence written by others; read only).
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use emath_artifact::{parse_json_document, JsonValue};
use emath_exec_ir::constructor_layer::{render_scratch_value, CValue};

use super::journal::{
    complete_lines, diff_archive, io_fault, project_record, read_events, JournalEvent,
    JournalWriter, Obj, RecordView, JOURNAL_SCHEMA,
};
use super::chain::{self, ChainSeam};
use super::inbox;
use super::ledger::{ledger_path, Ledger};
use super::{declares_emission_seam, DreamConfig, DreamLadder, DreamRun};
use crate::host::{value_sequence, verdict_name, FactDraw, HostFault, LoopHost};

/// The schema `run.json` declares.
pub const RUN_SCHEMA: &str = "emath.dream.run.v1";

/// A run's configuration: the canonical dream settings (echoed in the
/// journal's `start` event) plus context-only ceilings and retention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunConfig {
    /// The module file (absolute once the run is created).
    pub module: PathBuf,
    /// The Step function (resolved at creation).
    pub target: Option<String>,
    pub budget0: i128,
    pub budget_step: i128,
    pub plateau_close: u32,
    pub max_batches: u64,
    /// A logical-unit ceiling: when the resume law's raise would pass
    /// it, the run ends `budget_exhausted` instead of raising.
    pub max_units: Option<i128>,
    pub checkpoint_every: u64,
    pub keep_checkpoints: usize,
    /// Engine steps per Seed/Step evaluation (`DreamConfig::work`).
    pub work: u64,
}

impl RunConfig {
    /// The defaults (`DreamConfig::default()` for the dream settings).
    #[must_use]
    pub fn new(module: PathBuf) -> Self {
        let dream = DreamConfig::default();
        Self {
            module,
            target: None,
            budget0: dream.budget0,
            budget_step: dream.budget_step,
            plateau_close: dream.plateau_close,
            max_batches: dream.max_batches,
            max_units: None,
            checkpoint_every: 25,
            keep_checkpoints: 8,
            work: dream.work,
        }
    }

    fn dream(&self) -> DreamConfig {
        DreamConfig {
            budget0: self.budget0,
            budget_step: self.budget_step,
            max_batches: self.max_batches,
            plateau_close: self.plateau_close,
            out_dir: None,
            work: self.work,
        }
    }

    fn validate(&self) -> Result<(), HostFault> {
        if self.budget0 < 1
            || self.budget_step < 1
            || self.plateau_close < 1
            || self.checkpoint_every < 1
            || self.keep_checkpoints < 1
            || self.work < 1
        {
            return Err(HostFault::fault(
                "dream_config",
                "budget, budget-step, plateau-close, checkpoint-every, keep-checkpoints, and work \
                 must each be at least 1",
            ));
        }
        Ok(())
    }
}

/// How a run ended (context: `run.json`; the canonical `end` event
/// carries outcome and code, the human message lives here).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ended {
    pub outcome: String,
    pub code: Option<String>,
    pub message: String,
}

/// `run.json`.
#[derive(Clone, Debug)]
pub struct RunInfo {
    pub config: RunConfig,
    /// The resolved Step function.
    pub target: String,
    pub argv: Vec<String>,
    pub created_unix_ms: u128,
    pub ended: Option<Ended>,
}

/// One run directory.
#[derive(Clone, Debug)]
pub struct RunDir {
    pub dir: PathBuf,
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

fn write_atomic(path: &Path, text: &str) -> Result<(), HostFault> {
    let tmp = path.with_extension("tmp-write");
    fs::write(&tmp, text).map_err(|err| io_fault("dream_io", &tmp, &err))?;
    fs::rename(&tmp, path).map_err(|err| io_fault("dream_io", path, &err))
}

fn append_line(path: &Path, line: &str) -> Result<(), HostFault> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| io_fault("dream_io", path, &err))?;
    let mut text = line.to_string();
    text.push('\n');
    file.write_all(text.as_bytes()).map_err(|err| io_fault("dream_io", path, &err))
}

fn obj_get<'a>(entries: &'a [(String, JsonValue)], key: &str) -> Option<&'a JsonValue> {
    entries.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

fn json_i128(value: Option<&JsonValue>) -> Option<i128> {
    match value {
        Some(JsonValue::Num(text)) => text.parse().ok(),
        _ => None,
    }
}

fn json_string(value: Option<&JsonValue>) -> Option<String> {
    match value {
        Some(JsonValue::Str(text)) => Some(text.clone()),
        _ => None,
    }
}

impl RunDir {
    #[must_use]
    pub fn new(dir: &Path) -> Self {
        Self { dir: dir.to_path_buf() }
    }

    /// The evaluation ledger for `facts_id`, beside the runs
    /// (`<root>/ledger/`).
    #[must_use]
    pub fn ledger(&self, facts_id: &str) -> PathBuf {
        ledger_path(self.dir.parent().unwrap_or(Path::new(".")), facts_id)
    }

    /// Open the ledger for the run's host and carry everything in it.
    fn carry_ledger(&self, host: &LoopHost) -> Result<Option<Ledger>, HostFault> {
        let Some(id) = host.fact_id() else {
            return Ok(None);
        };
        let mut ledger = Ledger::new(self.ledger(id), id);
        host.carry_facts(ledger.sync()?);
        Ok(Some(ledger))
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.dir
            .file_name()
            .map_or_else(|| self.dir.display().to_string(), |name| name.to_string_lossy().into_owned())
    }

    #[must_use]
    pub fn journal(&self) -> PathBuf {
        self.dir.join("journal.jsonl")
    }

    #[must_use]
    pub fn metrics(&self) -> PathBuf {
        self.dir.join("metrics.jsonl")
    }

    #[must_use]
    pub fn run_json(&self) -> PathBuf {
        self.dir.join("run.json")
    }

    #[must_use]
    pub fn requests(&self) -> PathBuf {
        self.dir.join("control").join("requests.jsonl")
    }

    #[must_use]
    pub fn acks(&self) -> PathBuf {
        self.dir.join("control").join("acks.jsonl")
    }

    #[must_use]
    pub fn lock(&self) -> PathBuf {
        self.dir.join("driver.lock")
    }

    #[must_use]
    pub fn log(&self) -> PathBuf {
        self.dir.join("driver.log")
    }

    #[must_use]
    pub fn checkpoints(&self) -> PathBuf {
        self.dir.join("checkpoints")
    }

    #[must_use]
    pub fn level(&self) -> PathBuf {
        self.dir.join("level")
    }

    #[must_use]
    pub fn inbox(&self) -> PathBuf {
        self.dir.join("inbox")
    }

    /// The run's own copy of the launch module and its local imports.
    #[must_use]
    pub fn world(&self) -> PathBuf {
        self.dir.join("world")
    }

    /// Level 1's source: the world copy (runs created before the copy
    /// existed read the launch module).
    fn level_one(&self, info: &RunInfo) -> PathBuf {
        let copy = info.config.module.file_name().map(|name| self.world().join(name));
        match copy {
            Some(copy) if copy.is_file() => copy,
            _ => info.config.module.clone(),
        }
    }

    #[must_use]
    pub fn checkpoint(&self, batch: u64) -> PathBuf {
        self.checkpoints().join(format!("b{batch:06}.scratch"))
    }

    /// The checkpoints on disk: `(batch, path, bytes)`, oldest first.
    #[must_use]
    pub fn list_checkpoints(&self) -> Vec<(u64, PathBuf, u64)> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(self.checkpoints()) else {
            return out;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(batch) = name
                .strip_prefix('b')
                .and_then(|rest| rest.strip_suffix(".scratch"))
                .and_then(|digits| digits.parse::<u64>().ok())
            else {
                continue;
            };
            let bytes = entry.metadata().map_or(0, |meta| meta.len());
            out.push((batch, entry.path(), bytes));
        }
        out.sort_by_key(|(batch, _, _)| *batch);
        out
    }

    /// Whether this directory is a dream run (`run.json` with our schema).
    #[must_use]
    pub fn is_run(&self) -> bool {
        fs::read_to_string(self.run_json()).is_ok_and(|text| text.contains(RUN_SCHEMA))
    }

    /// Read `run.json`.
    pub fn info(&self) -> Result<RunInfo, HostFault> {
        let path = self.run_json();
        let text = fs::read_to_string(&path).map_err(|err| io_fault("dream_run_missing", &path, &err))?;
        let json = parse_json_document(&text)
            .map_err(|err| HostFault::fault("dream_run_json", format!("{}: {err:?}", path.display())))?;
        let JsonValue::Obj(entries) = json else {
            return Err(HostFault::fault("dream_run_json", "run.json is not an object"));
        };
        if json_string(obj_get(&entries, "schema")).as_deref() != Some(RUN_SCHEMA) {
            return Err(HostFault::fault("dream_run_json", format!("run.json does not declare {RUN_SCHEMA}")));
        }
        let config = match obj_get(&entries, "config") {
            Some(JsonValue::Obj(config)) => config.clone(),
            _ => return Err(HostFault::fault("dream_run_json", "run.json has no config")),
        };
        let module = PathBuf::from(json_string(obj_get(&entries, "module")).unwrap_or_default());
        let mut cfg = RunConfig::new(module);
        let target = json_string(obj_get(&entries, "target")).unwrap_or_default();
        cfg.target = Some(target.clone());
        let int = |key: &str| json_i128(obj_get(&config, key));
        cfg.budget0 = int("budget0").unwrap_or(cfg.budget0);
        cfg.budget_step = int("budget_step").unwrap_or(cfg.budget_step);
        cfg.plateau_close = int("plateau_close").and_then(|v| u32::try_from(v).ok()).unwrap_or(cfg.plateau_close);
        cfg.max_batches = int("max_batches").and_then(|v| u64::try_from(v).ok()).unwrap_or(cfg.max_batches);
        cfg.max_units = int("max_units");
        cfg.checkpoint_every = int("checkpoint_every").and_then(|v| u64::try_from(v).ok()).unwrap_or(cfg.checkpoint_every);
        cfg.keep_checkpoints = int("keep_checkpoints").and_then(|v| usize::try_from(v).ok()).unwrap_or(cfg.keep_checkpoints);
        cfg.work = int("work").and_then(|v| u64::try_from(v).ok()).unwrap_or(cfg.work);
        let argv = match obj_get(&entries, "argv") {
            Some(JsonValue::Arr(items)) => items.iter().filter_map(|item| json_string(Some(item))).collect(),
            _ => Vec::new(),
        };
        let ended = match obj_get(&entries, "ended") {
            Some(JsonValue::Obj(ended)) => Some(Ended {
                outcome: json_string(obj_get(ended, "outcome")).unwrap_or_default(),
                code: json_string(obj_get(ended, "code")),
                message: json_string(obj_get(ended, "message")).unwrap_or_default(),
            }),
            _ => None,
        };
        Ok(RunInfo {
            config: cfg,
            target,
            argv,
            created_unix_ms: json_i128(obj_get(&entries, "created_unix_ms")).and_then(|v| u128::try_from(v).ok()).unwrap_or(0),
            ended,
        })
    }

    /// Write `run.json` atomically.
    pub fn write_info(&self, info: &RunInfo) -> Result<(), HostFault> {
        let c = &info.config;
        let config = Obj::new()
            .int("budget0", c.budget0)
            .int("budget_step", c.budget_step)
            .int("plateau_close", c.plateau_close)
            .int("max_batches", c.max_batches)
            .opt_int("max_units", c.max_units)
            .int("checkpoint_every", c.checkpoint_every)
            .int("keep_checkpoints", c.keep_checkpoints)
            .int("work", c.work);
        let argv: Vec<String> = info.argv.iter().map(|arg| emath_core::json_quote(arg)).collect();
        let ended = match &info.ended {
            Some(ended) => Obj::new()
                .str("outcome", &ended.outcome)
                .opt_str("code", ended.code.as_deref())
                .str("message", &ended.message)
                .render(),
            None => "null".into(),
        };
        let text = Obj::new()
            .str("schema", RUN_SCHEMA)
            .str("module", &c.module.display().to_string())
            .str("target", &info.target)
            .raw("config", config.render())
            .array("argv", &argv)
            .int("created_unix_ms", info.created_unix_ms)
            .str("engine", "vm")
            .raw("ended", ended)
            .render();
        write_atomic(&self.run_json(), &(text + "\n"))
    }

    /// The pid in `driver.lock`, if any.
    #[must_use]
    pub fn lock_pid(&self) -> Option<u32> {
        fs::read_to_string(self.lock()).ok()?.trim().parse().ok()
    }
}

/// Whether a process is alive (`kill -0`; non-unix hosts assume alive).
#[must_use]
pub fn pid_alive(pid: u32) -> bool {
    if cfg!(unix) {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    } else {
        true
    }
}

/// Resolve `<run>`: a directory holding `run.json`, or a name under `root`.
pub fn resolve_run(root: &Path, spec: &str) -> Result<RunDir, HostFault> {
    let direct = RunDir::new(Path::new(spec));
    if direct.is_run() {
        return Ok(direct);
    }
    let named = RunDir::new(&root.join(spec));
    if named.is_run() {
        return Ok(named);
    }
    Err(HostFault::fault(
        "dream_run_missing",
        format!("no dream run `{spec}` (looked in {} and at the path itself)", root.display()),
    ))
}

/// The first free default run name under `root`: `<target-lowercase>-NNN`
/// (SPEC §3.1). A fork with no `--run` gets the same name.
pub fn next_run_name(root: &Path, target: &str) -> String {
    let base = target.to_lowercase();
    (1u32..)
        .map(|n| format!("{base}-{n:03}"))
        .find(|name| !root.join(name).exists())
        .unwrap_or(base)
}

fn prepare_dir(root: &Path, name: Option<&str>, target: &str, overwrite: bool) -> Result<RunDir, HostFault> {
    fs::create_dir_all(root).map_err(|err| io_fault("dream_io", root, &err))?;
    let name = match name {
        Some(name) => {
            if name.is_empty()
                || name.contains('/')
                || name.contains('\\')
                || name.starts_with('.')
                || name == "ledger"
            {
                return Err(HostFault::fault("dream_run_name", format!("`{name}` is not a run name")));
            }
            name.to_string()
        }
        None => {
            // Parallel launches race for the same free name: claiming it
            // with an atomic create_dir gives each launch its own.
            let base = target.to_lowercase();
            for n in 1u32.. {
                let name = format!("{base}-{n:03}");
                match fs::create_dir(root.join(&name)) {
                    Ok(()) => return finish_dir(RunDir::new(&root.join(&name))),
                    Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(err) => return Err(io_fault("dream_io", &root.join(&name), &err)),
                }
            }
            unreachable!("u32 run names exhausted")
        }
    };
    let run = RunDir::new(&root.join(&name));
    if run.dir.exists() {
        if !overwrite {
            return Err(HostFault::fault(
                "dream_run_exists",
                format!("run `{name}` already exists at {}; pass --overwrite or choose --run", run.dir.display()),
            ));
        }
        if !run.is_run() {
            return Err(HostFault::fault(
                "dream_run_exists",
                format!("{} exists and is not a dream run; refusing to overwrite it", run.dir.display()),
            ));
        }
        // Run output is scratch data: overwriting a dream run replaces
        // its directory (never anything that is not a dream run).
        fs::remove_dir_all(&run.dir).map_err(|err| io_fault("dream_io", &run.dir, &err))?;
    }
    match fs::create_dir(&run.dir) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(HostFault::fault(
                "dream_run_exists",
                format!("run `{name}` was created concurrently at {}; choose --run", run.dir.display()),
            ));
        }
        Err(err) => return Err(io_fault("dream_io", &run.dir, &err)),
    }
    finish_dir(run)
}

fn finish_dir(run: RunDir) -> Result<RunDir, HostFault> {
    for dir in [run.dir.join("control"), run.checkpoints()] {
        fs::create_dir_all(&dir).map_err(|err| io_fault("dream_io", &dir, &err))?;
    }
    Ok(run)
}

/// Create a run directory for `config` (the module is opened once to
/// resolve the target and refuse early).
pub fn create_run(
    root: &Path,
    name: Option<&str>,
    overwrite: bool,
    mut config: RunConfig,
    argv: Vec<String>,
) -> Result<RunDir, HostFault> {
    config.validate()?;
    let module = std::path::absolute(&config.module).map_err(|err| io_fault("loop_read", &config.module, &err))?;
    let host = LoopHost::open(&module, config.target.as_deref())?;
    let target = host.surface().step.clone();
    config.module = module;
    config.target = Some(target.clone());
    let run = prepare_dir(root, name, &target, overwrite)?;
    chain::copy_world(&config.module, &run.world())?;
    run.write_info(&RunInfo {
        config,
        target,
        argv,
        created_unix_ms: now_ms(),
        ended: None,
    })?;
    Ok(run)
}

// ---- control channel ----------------------------------------------------

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Append one control request; returns its id.
pub fn send_request(run: &RunDir, action: &str, arg: Option<i128>) -> Result<String, HostFault> {
    let n = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
    let id = format!("{}-{n}", std::process::id());
    let control = run.dir.join("control");
    fs::create_dir_all(&control).map_err(|err| io_fault("dream_io", &control, &err))?;
    let line = Obj::new().str("id", &id).str("action", action).opt_int("arg", arg).render();
    append_line(&run.requests(), &line)?;
    Ok(id)
}

/// One control acknowledgement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ack {
    pub id: String,
    pub outcome: String,
    pub batch: u64,
    pub code: Option<String>,
}

fn parse_objects(path: &Path) -> Vec<Vec<(String, JsonValue)>> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    complete_lines(&text)
        .into_iter()
        .filter_map(|line| match parse_json_document(line) {
            Ok(JsonValue::Obj(entries)) => Some(entries),
            _ => None,
        })
        .collect()
}

/// Every acknowledgement so far.
#[must_use]
pub fn read_acks(run: &RunDir) -> Vec<Ack> {
    parse_objects(&run.acks())
        .into_iter()
        .map(|entries| Ack {
            id: json_string(obj_get(&entries, "id")).unwrap_or_default(),
            outcome: json_string(obj_get(&entries, "outcome")).unwrap_or_default(),
            batch: json_i128(obj_get(&entries, "batch")).and_then(|b| u64::try_from(b).ok()).unwrap_or(0),
            code: json_string(obj_get(&entries, "code")),
        })
        .collect()
}

/// One control request as the viewer wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: String,
    pub action: String,
    pub arg: Option<i128>,
}

/// Every request so far.
#[must_use]
pub fn read_requests(run: &RunDir) -> Vec<Request> {
    parse_objects(&run.requests())
        .into_iter()
        .map(|entries| Request {
            id: json_string(obj_get(&entries, "id")).unwrap_or_default(),
            action: json_string(obj_get(&entries, "action")).unwrap_or_default(),
            arg: json_i128(obj_get(&entries, "arg")),
        })
        .collect()
}

// ---- the driver ---------------------------------------------------------

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

/// The events a resume or fork at batch `c` keeps: no `end`, and
/// nothing that belongs to a batch after `c`.
fn kept_at(events: &[JournalEvent], c: u64) -> Vec<&JournalEvent> {
    events
        .iter()
        .filter(|event| event.t != "end" && event.batch().is_none_or(|batch| batch <= c))
        .collect()
}

/// The ladder of the level `kept` ends in (`kept` starts at that
/// level's `seed` or `level_next`).
fn ladder_of(kept: &[&JournalEvent], budget0: i128) -> DreamLadder {
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

fn truncate_metrics(path: &Path, c: u64) -> Result<(), HostFault> {
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

fn resumable(events: &[JournalEvent]) -> Result<(), HostFault> {
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
        let archive = value_sequence(session.state(), "archive")?
            .iter()
            .map(project_record)
            .collect::<Result<Vec<_>, _>>()?;
        let incumbent = project_record(&session.incumbent_record()?)?;
        journal.emit(
            "seed",
            Obj::new()
                .int("batch", 0)
                .int("used", session.state_int("used")?)
                .int("watermark", run.watermark())
                .raw("incumbent", incumbent)
                .array("archive", &archive)
                .ints("case_ids", &session.case_ids()?),
        )?;
        write_atomic(&dir.metrics(), "")?;
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
        })
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
                chain::verify(&path, &self.info.target).map(|()| literal)
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
        self.ledger = self.dir.carry_ledger(run.host())?;
        self.run = run;
        self.level = level;
        self.base = batch;
        self.level_path = path;
        let session = self.run.session();
        let archive = value_sequence(session.state(), "archive")?
            .iter()
            .map(project_record)
            .collect::<Result<Vec<_>, _>>()?;
        let incumbent = project_record(&session.incumbent_record()?)?;
        let used = session.state_int("used")?;
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
        let prev_archive = value_sequence(self.run.session().state(), "archive")?;
        let prev_cases = self.run.session().case_ids()?;
        let prev_used = self.run.session().state_int("used")?;
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
        let next_archive = value_sequence(session.state(), "archive")?;
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
        let incumbent = project_record(&session.incumbent_record()?)?;
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
            let incumbent = project_record(&self.run.session().incumbent_record()?)?;
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

// ---- fork ---------------------------------------------------------------

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

// ---- export and list ----------------------------------------------------

/// A value as the export and the dashboard spell it: exact integers,
/// `n/d` rationals (an integer when `d = 1`), booleans, text; anything
/// else in the scratch interchange.
#[must_use]
pub fn display_value(value: &CValue) -> String {
    match value {
        CValue::Int(n) => n.to_string(),
        CValue::Rat { num, den } => {
            if den.to_string() == "1" {
                num.to_string()
            } else {
                format!("{num}/{den}")
            }
        }
        CValue::Bool(b) => b.to_string(),
        CValue::Str(text) => text.to_string(),
        other => render_scratch_value(other).unwrap_or_else(|_| "?".into()),
    }
}

fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

fn join_ints(values: &[i128]) -> String {
    values.iter().map(ToString::to_string).collect::<Vec<_>>().join(";")
}

/// Export a run's `batches` or `records` table as `csv` or `jsonl`.
pub fn export_run(run: &RunDir, table: &str, format: &str) -> Result<String, HostFault> {
    let events = read_events(&run.journal())?;
    let (columns, rows): (Vec<String>, Vec<Vec<String>>) = match table {
        "batches" => {
            let columns = [
                "batch", "verdict", "verdict_name", "used", "delta_used", "watermark", "plateau_streak",
                "incumbent_key", "incumbent_score", "promoted", "quarantined", "case_count", "archive_len",
            ];
            let rows = events
                .iter()
                .filter(|e| e.t == "batch")
                .map(|e| {
                    let incumbent = e.record("incumbent").unwrap_or_default();
                    let field = |name: &str| incumbent.get(name).map(display_value).unwrap_or_default();
                    vec![
                        e.uint("batch").unwrap_or(0).to_string(),
                        e.int("verdict").unwrap_or(0).to_string(),
                        e.str("verdict_name").unwrap_or("").to_string(),
                        e.int("used").unwrap_or(0).to_string(),
                        e.int("delta_used").unwrap_or(0).to_string(),
                        e.int("watermark").unwrap_or(0).to_string(),
                        e.int("plateau_streak").unwrap_or(0).to_string(),
                        field("key"),
                        field("score"),
                        join_ints(&e.ints("promoted")),
                        join_ints(&e.ints("quarantined")),
                        e.ints("case_ids").len().to_string(),
                        e.int("archive_len").unwrap_or(0).to_string(),
                    ]
                })
                .collect();
            (columns.iter().map(|c| (*c).to_string()).collect(), rows)
        }
        "records" => {
            let mut latest: BTreeMap<String, RecordView> = BTreeMap::new();
            let mut order: Vec<String> = Vec::new();
            let mut take = |record: RecordView| {
                let key = record.get("key").map(display_value).unwrap_or_default();
                if !latest.contains_key(&key) {
                    order.push(key.clone());
                }
                latest.insert(key, record);
            };
            for event in &events {
                match event.t.as_str() {
                    "seed" => event.records("archive").into_iter().for_each(&mut take),
                    "records" => {
                        event.records("added").into_iter().for_each(&mut take);
                        event.records("changed").into_iter().for_each(&mut take);
                    }
                    _ => {}
                }
            }
            let names: BTreeSet<String> = latest.values().flat_map(|r| r.keys().cloned()).collect();
            let mut columns: Vec<String> = vec!["key".into()];
            columns.extend(names.into_iter().filter(|name| name != "key"));
            let rows = order
                .iter()
                .map(|key| {
                    let record = &latest[key];
                    columns.iter().map(|c| record.get(c).map(display_value).unwrap_or_default()).collect()
                })
                .collect();
            (columns, rows)
        }
        other => {
            return Err(HostFault::fault("dream_export", format!("unknown table `{other}` (batches or records)")));
        }
    };
    let mut out = String::new();
    match format {
        "csv" => {
            out.push_str(&columns.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
            out.push('\n');
            for row in &rows {
                out.push_str(&row.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
                out.push('\n');
            }
        }
        "jsonl" => {
            for row in &rows {
                let fields: Vec<String> = columns
                    .iter()
                    .zip(row)
                    .map(|(c, v)| format!("{}:{}", emath_core::json_quote(c), emath_core::json_quote(v)))
                    .collect();
                out.push('{');
                out.push_str(&fields.join(","));
                out.push_str("}\n");
            }
        }
        other => {
            return Err(HostFault::fault("dream_export", format!("unknown format `{other}` (csv or jsonl)")));
        }
    }
    Ok(out)
}

/// One `emath dream list` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSummary {
    pub name: String,
    pub target: String,
    /// The outcome, or `running` / `stalled`.
    pub status: String,
    pub last_batch: u64,
    pub incumbent_score: Option<String>,
}

/// Summarise every run under `root`, sorted by name.
#[must_use]
pub fn list_runs(root: &Path) -> Vec<RunSummary> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let run = RunDir::new(&entry.path());
        let Ok(info) = run.info() else {
            continue;
        };
        let events = read_events(&run.journal()).unwrap_or_default();
        let last = events.iter().rev().find(|e| e.t == "batch");
        let end = events.iter().rev().find(|e| e.t == "end");
        let status = match (end, &info.ended) {
            (Some(end), _) => end.str("outcome").unwrap_or("ended").to_string(),
            (None, Some(ended)) => ended.outcome.clone(),
            (None, None) => match run.lock_pid() {
                Some(pid) if pid_alive(pid) => "running".into(),
                _ => "stalled".into(),
            },
        };
        let incumbent_score = last
            .or_else(|| events.iter().find(|e| e.t == "seed"))
            .and_then(|e| e.record("incumbent"))
            .and_then(|r| r.get("score").map(display_value));
        out.push(RunSummary {
            name: run.name(),
            target: info.target,
            status,
            last_batch: last.and_then(JournalEvent::batch).unwrap_or(0),
            incumbent_score,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
