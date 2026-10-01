//! The `emath dream` run directory: its configuration, `run.json`,
//! paths, creation, and the control channel. The driver process lives
//! in [`super::driver`], the resume and fork laws in [`super::resume`],
//! export and listing in [`super::export`].
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
//! Control requests are applied by the driver at batch boundaries only;
//! writing one never changes `journal.jsonl`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use emath_artifact::{parse_json_document, JsonValue};
use emath_exec_ir::constructor_layer::CarrierWorld;

use super::chain;
use super::journal::{complete_lines, io_fault, Obj};
use super::ledger::{ledger_path, Ledger};
use super::DreamConfig;
use crate::host::{HostFault, LoopHost};

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
    /// The carrier worlds `quote.worlds()` returns to the module's
    /// judges; empty leaves the engine default (every world).
    pub worlds: Vec<String>,
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
            worlds: Vec::new(),
        }
    }

    pub(super) fn dream(&self) -> DreamConfig {
        DreamConfig {
            budget0: self.budget0,
            budget_step: self.budget_step,
            max_batches: self.max_batches,
            plateau_close: self.plateau_close,
            out_dir: None,
            work: self.work,
        }
    }

    pub(super) fn validate(&self) -> Result<(), HostFault> {
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
        self.carrier_worlds().map(|_| ())
    }

    /// The configured worlds, each name checked against the engine's.
    pub fn carrier_worlds(&self) -> Result<Vec<CarrierWorld>, HostFault> {
        self.worlds
            .iter()
            .map(|name| {
                CarrierWorld::parse(name).ok_or_else(|| {
                    let known: Vec<&str> = CarrierWorld::ALL.iter().map(|w| w.name()).collect();
                    HostFault::fault(
                        "dream_config",
                        format!("unknown world `{name}` in --worlds; worlds are {}", known.join(", ")),
                    )
                })
            })
            .collect()
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

pub(super) fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

pub(super) fn write_atomic(path: &Path, text: &str) -> Result<(), HostFault> {
    let tmp = path.with_extension("tmp-write");
    fs::write(&tmp, text).map_err(|err| io_fault("dream_io", &tmp, &err))?;
    fs::rename(&tmp, path).map_err(|err| io_fault("dream_io", path, &err))
}

pub(super) fn append_line(path: &Path, line: &str) -> Result<(), HostFault> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| io_fault("dream_io", path, &err))?;
    let mut text = line.to_string();
    text.push('\n');
    file.write_all(text.as_bytes()).map_err(|err| io_fault("dream_io", path, &err))
}

pub(super) fn obj_get<'a>(entries: &'a [(String, JsonValue)], key: &str) -> Option<&'a JsonValue> {
    entries.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

pub(super) fn json_i128(value: Option<&JsonValue>) -> Option<i128> {
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
    pub(super) fn carry_ledger(&self, host: &LoopHost) -> Result<Option<Ledger>, HostFault> {
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
    pub(super) fn level_one(&self, info: &RunInfo) -> PathBuf {
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
        cfg.worlds = json_string(obj_get(&config, "worlds"))
            .map(|list| list.split(',').filter(|w| !w.is_empty()).map(str::to_string).collect())
            .unwrap_or_default();
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
            .int("work", c.work)
            .opt_str("worlds", worlds_text(c).as_deref());
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

pub(super) fn prepare_dir(root: &Path, name: Option<&str>, target: &str, overwrite: bool) -> Result<RunDir, HostFault> {
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

/// The configured world list as `run.json` and the journal spell it.
pub(super) fn worlds_text(config: &RunConfig) -> Option<String> {
    (!config.worlds.is_empty()).then(|| config.worlds.join(","))
}
