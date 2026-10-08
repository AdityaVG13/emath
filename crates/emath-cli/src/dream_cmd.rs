//! `emath dream` (bead emath-3vium, SPEC
//! `internal/design/dream-dashboard/SPEC.md` §4): start, watch, resume,
//! fork, list and export research runs. A run is a driver process that
//! writes `.emath/runs/<name>/`; the dashboard only reads it. Closing
//! the dashboard never stops the run.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

#[cfg(feature = "dashboard")]
use emath_tui::dashboard::app::{watch, WatchOptions};
use emath_tui::dashboard::canvas::THEMES;
use emath_tui::dashboard::model::{dec4, units};
use emath_tui::dashboard::view::PanelId;
use emath_tui::dashboard::watch::RunWatch;
use emath_tui::dream::driver::{drive, spawn_driver};
use emath_tui::dream::export::{export_run, list_runs};
use emath_tui::dream::resume::{check_resumable, fork_run};
use emath_tui::dream::rundir::{create_run, resolve_run, RunConfig, RunDir};
use emath_tui::host::HostFault;

use crate::{CliExit, EXIT_OK, EXIT_REFUSED};

pub(crate) const USAGE: &str = "dream <file.emath> [--target StepFn] [--run NAME] [--overwrite] [--budget N] [--budget-step N] [--plateau-close N] [--max-batches N] [--max-units N] [--checkpoint-every N] [--keep-checkpoints N] [--work N] [--worlds w1,w2,...] [--detach] [--theme aurora|nordic|paper] [--hide p1,p2] [--no-measure] [--root DIR] | dream watch|resume|fork|export <run> | dream list";

/// Viewer flags shared by every attaching subcommand.
/// Parsed and validated even without the dashboard so a bad flag is
/// refused the same way in both builds.
#[derive(Clone, Debug, Default)]
#[cfg_attr(not(feature = "dashboard"), allow(dead_code))]
pub(crate) struct ViewFlags {
    theme: usize,
    hidden: Vec<PanelId>,
    no_measure: bool,
}

/// A parsed `emath dream` request.
#[derive(Clone, Debug)]
pub(crate) enum DreamRequest {
    Start { config: Box<RunConfig>, name: Option<String>, overwrite: bool, detach: bool, view: ViewFlags, root: PathBuf, argv: Vec<String> },
    Watch { run: String, view: ViewFlags, root: PathBuf },
    List { root: PathBuf },
    Resume { run: String, max_batches: Option<u64>, max_units: Option<i128>, work: Option<u64>, detach: bool, view: ViewFlags, root: PathBuf },
    Fork { run: String, at: Option<u64>, name: Option<String>, max_batches: Option<u64>, max_units: Option<i128>, work: Option<u64>, detach: bool, view: ViewFlags, root: PathBuf },
    Export { run: String, format: String, table: String, out: Option<PathBuf>, root: PathBuf },
    Drive { dir: PathBuf },
}

const VALUED: &[&str] = &[
    "--target", "--run", "--budget", "--budget-step", "--plateau-close", "--max-batches", "--max-units", "--checkpoint-every",
    "--keep-checkpoints", "--theme", "--hide", "--at", "--format", "--table", "-o", "--root", "--work", "--worlds",
];

struct Args {
    positional: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

impl Args {
    fn parse(rest: &[String]) -> Option<Self> {
        let mut positional = Vec::new();
        let mut flags = Vec::new();
        let mut i = 0;
        while i < rest.len() {
            let arg = &rest[i];
            if VALUED.contains(&arg.as_str()) {
                i += 1;
                flags.push((arg.clone(), Some(rest.get(i)?.clone())));
            } else if arg.starts_with("--") {
                flags.push((arg.clone(), None));
            } else {
                positional.push(arg.clone());
            }
            i += 1;
        }
        Some(Args { positional, flags })
    }
    fn value(&self, name: &str) -> Option<&str> {
        self.flags.iter().rev().find(|(k, _)| k == name).and_then(|(_, v)| v.as_deref())
    }
    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(k, _)| k == name)
    }
    fn num<T: std::str::FromStr>(&self, name: &str) -> Result<Option<T>, ()> {
        match self.value(name) {
            None => Ok(None),
            Some(text) => text.parse().map(Some).map_err(|_| ()),
        }
    }
    fn view(&self) -> Option<ViewFlags> {
        let theme = match self.value("--theme") {
            None => 0,
            Some(name) => THEMES.iter().position(|t| *t == name)?,
        };
        let mut hidden = Vec::new();
        if let Some(list) = self.value("--hide") {
            for name in list.split(',').filter(|n| !n.is_empty()) {
                hidden.push(PanelId::from_name(name.trim())?);
            }
        }
        Some(ViewFlags { theme, hidden, no_measure: self.has("--no-measure") })
    }
    fn root(&self) -> PathBuf {
        match self.value("--root") {
            Some(root) => PathBuf::from(root),
            None => std::env::var_os("EMATH_DREAM_ROOT").map_or_else(|| PathBuf::from(".emath").join("runs"), PathBuf::from),
        }
    }
}

impl DreamRequest {
    /// Parse the arguments after `dream`. `None` is a usage fault.
    pub(crate) fn parse(rest: &[String]) -> Option<Self> {
        let args = Args::parse(rest)?;
        let view = args.view()?;
        let root = args.root();
        let first = args.positional.first()?.as_str();
        let one = || if args.positional.len() == 2 { Some(args.positional[1].clone()) } else { None };
        Some(match first {
            "__drive" => DreamRequest::Drive { dir: PathBuf::from(one()?) },
            "watch" => DreamRequest::Watch { run: one()?, view, root },
            "list" if args.positional.len() == 1 => DreamRequest::List { root },
            "resume" => DreamRequest::Resume {
                run: one()?,
                max_batches: args.num("--max-batches").ok()?,
                max_units: args.num("--max-units").ok()?,
                work: args.num("--work").ok()?,
                detach: args.has("--detach"),
                view,
                root,
            },
            "fork" => DreamRequest::Fork {
                run: one()?,
                at: args.num("--at").ok()?,
                name: args.value("--run").map(str::to_string),
                max_batches: args.num("--max-batches").ok()?,
                max_units: args.num("--max-units").ok()?,
                work: args.num("--work").ok()?,
                detach: args.has("--detach"),
                view,
                root,
            },
            "export" => DreamRequest::Export {
                run: one()?,
                format: args.value("--format")?.to_string(),
                table: args.value("--table").unwrap_or("batches").to_string(),
                out: args.value("-o").map(PathBuf::from),
                root,
            },
            _ if args.positional.len() == 1 => {
                let mut config = RunConfig::new(PathBuf::from(first));
                config.target = args.value("--target").map(str::to_string);
                if let Some(v) = args.num("--budget").ok()? {
                    config.budget0 = v;
                }
                if let Some(v) = args.num("--budget-step").ok()? {
                    config.budget_step = v;
                }
                if let Some(v) = args.num("--plateau-close").ok()? {
                    config.plateau_close = v;
                }
                if let Some(v) = args.num("--max-batches").ok()? {
                    config.max_batches = v;
                }
                config.max_units = args.num("--max-units").ok()?;
                if let Some(v) = args.num("--checkpoint-every").ok()? {
                    config.checkpoint_every = v;
                }
                if let Some(v) = args.num("--keep-checkpoints").ok()? {
                    config.keep_checkpoints = v;
                }
                if let Some(v) = args.num("--work").ok()? {
                    config.work = v;
                }
                if let Some(list) = args.value("--worlds") {
                    config.worlds = list.split(',').map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect();
                }
                DreamRequest::Start {
                    config: Box::new(config),
                    name: args.value("--run").map(str::to_string),
                    overwrite: args.has("--overwrite"),
                    detach: args.has("--detach"),
                    view,
                    root,
                    argv: rest.to_vec(),
                }
            }
            _ => return None,
        })
    }
}

fn refuse(fault: &HostFault) -> CliExit {
    eprintln!("error {}: {}", fault.code, fault.message);
    EXIT_REFUSED
}

fn exe() -> Result<PathBuf, HostFault> {
    std::env::current_exe().map_err(|err| HostFault { code: "dream_io".into(), message: format!("cannot locate the emath executable: {err}") })
}

/// Start the driver for `run`, then attach unless detached.
fn launch(run: RunDir, detach: bool, view: ViewFlags, root: &Path) -> CliExit {
    let exe = match exe() {
        Ok(exe) => exe,
        Err(fault) => return refuse(&fault),
    };
    if let Err(fault) = spawn_driver(&exe, &run) {
        return refuse(&fault);
    }
    let name = run.name();
    if detach || !std::io::stdout().is_terminal() {
        println!("{name}");
        eprintln!("driver started · emath dream watch {name}");
        return EXIT_OK;
    }
    attach(run, view, root)
}

#[cfg(not(feature = "dashboard"))]
fn attach(run: RunDir, _view: ViewFlags, _root: &Path) -> CliExit {
    if std::io::stdout().is_terminal() {
        eprintln!("note: this emath was built without the live dashboard (cargo build -p emath-cli --features dashboard)");
    }
    snapshot(run)
}

#[cfg(feature = "dashboard")]
fn attach(run: RunDir, view: ViewFlags, root: &Path) -> CliExit {
    let name = run.name();
    if !std::io::stdout().is_terminal() {
        return snapshot(run);
    }
    let opts = WatchOptions { theme: view.theme, hidden: view.hidden, no_measure: view.no_measure, exe: exe().ok(), root: root.to_path_buf() };
    match watch(run, opts) {
        Ok(()) => {
            eprintln!("detached · the run continues · emath dream watch {name}");
            EXIT_OK
        }
        Err(err) => {
            eprintln!("error dream_terminal: {err}");
            EXIT_REFUSED
        }
    }
}

/// A plain-text status (non-terminal stdout).
fn snapshot(run: RunDir) -> CliExit {
    let mut watch = RunWatch::open(run);
    watch.refresh();
    watch.poll();
    let model = &watch.model;
    let (badge, _) = watch.badge().text();
    println!("{badge}");
    println!("L{} · batch {}", model.level, model.last_batch());
    match (model.incumbent_rec().and_then(|r| r.score()), model.incumbent) {
        (Some(score), Some(key)) => println!("incumbent #{key}  {}", dec4(score)),
        _ => println!("incumbent -"),
    }
    println!("budget {} / {} u", units(model.used()), units(model.watermark()));
    for line in model.log.iter().rev().take(5).collect::<Vec<_>>().into_iter().rev() {
        let text: String = line.parts.iter().map(|p| p.0.as_str()).collect();
        println!("b{} {text}", line.batch);
    }
    EXIT_OK
}

/// Run `emath dream`.
pub(crate) fn run(request: DreamRequest) -> CliExit {
    match request {
        DreamRequest::Drive { dir } => match drive(&dir) {
            Ok(()) => EXIT_OK,
            Err(fault) => refuse(&fault),
        },
        DreamRequest::Start { config, name, overwrite, detach, view, root, argv } => {
            match create_run(&root, name.as_deref(), overwrite, *config, argv) {
                Ok(run) => launch(run, detach, view, &root),
                Err(fault) => refuse(&fault),
            }
        }
        DreamRequest::Watch { run, view, root } => match resolve_run(&root, &run) {
            Ok(run) => attach(run, view, &root),
            Err(fault) => refuse(&fault),
        },
        DreamRequest::List { root } => {
            for run in list_runs(&root) {
                let score = run.incumbent_score.unwrap_or_else(|| "-".into());
                println!("{}  {}  {}  b{}  {}", run.name, run.target, run.status, run.last_batch, score);
            }
            EXIT_OK
        }
        DreamRequest::Resume { run, max_batches, max_units, work, detach, view, root } => {
            let run = match resolve_run(&root, &run) {
                Ok(run) => run,
                Err(fault) => return refuse(&fault),
            };
            if let Err(fault) = check_resumable(&run) {
                return refuse(&fault);
            }
            if max_batches.is_some() || max_units.is_some() || work.is_some() {
                let mut info = match run.info() {
                    Ok(info) => info,
                    Err(fault) => return refuse(&fault),
                };
                if let Some(n) = max_batches {
                    info.config.max_batches = n;
                }
                if max_units.is_some() {
                    info.config.max_units = max_units;
                }
                if let Some(n) = work {
                    info.config.work = n;
                }
                if let Err(fault) = run.write_info(&info) {
                    return refuse(&fault);
                }
            }
            launch(run, detach, view, &root)
        }
        DreamRequest::Fork { run, at, name, max_batches, max_units, work, detach, view, root } => {
            let source = match resolve_run(&root, &run) {
                Ok(run) => run,
                Err(fault) => return refuse(&fault),
            };
            let forked = match fork_run(&root, &source, at, name.as_deref(), max_batches, max_units) {
                Ok(forked) => forked,
                Err(fault) => return refuse(&fault),
            };
            if let Some(n) = work {
                let written = forked.info().and_then(|mut info| {
                    info.config.work = n;
                    forked.write_info(&info)
                });
                if let Err(fault) = written {
                    return refuse(&fault);
                }
            }
            launch(forked, detach, view, &root)
        }
        DreamRequest::Export { run, format, table, out, root } => {
            let run = match resolve_run(&root, &run) {
                Ok(run) => run,
                Err(fault) => return refuse(&fault),
            };
            match export_run(&run, &table, &format) {
                Ok(text) => match out {
                    Some(path) => match std::fs::write(&path, text) {
                        Ok(()) => EXIT_OK,
                        Err(err) => {
                            eprintln!("error dream_io: {}: {err}", path.display());
                            EXIT_REFUSED
                        }
                    },
                    None => {
                        print!("{text}");
                        EXIT_OK
                    }
                },
                Err(fault) => refuse(&fault),
            }
        }
    }
}
