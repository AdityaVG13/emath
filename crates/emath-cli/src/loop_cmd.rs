//! `emath loop` (bead emath-q3cqx): the research-loop line REPL.
//!
//! One command drives every authored session surface: interactive
//! stdin (prompted when stdin is a TTY), a `--script` file of REPL
//! commands (deterministic transcripts, CI-friendly), or piped stdin
//! (scripted mode, no prompt). Host faults print as
//! `error <code>: <message>` lines and the session continues; only a
//! failed session start refuses the process.

use std::fs::File;
use std::io::{self, BufReader, IsTerminal};
use std::path::PathBuf;

use emath_tui::host::LoopHost;
use emath_tui::repl::{run_repl, ReplConfig};

use crate::{CliExit, EXIT_OK, EXIT_REFUSED};

pub(crate) const USAGE: &str = "loop <file.emath> [--target StepFn] [--budget N] [--script path]";

/// The budget a `step`/`run` uses when the command line and the REPL
/// command both omit one.
const DEFAULT_BUDGET: i128 = 60;

/// A parsed `emath loop` request.
#[derive(Clone, Debug)]
pub(crate) struct LoopRequest {
    /// The `.emath` module carrying the session surface.
    pub path: PathBuf,
    /// The Step function when the module carries more than one surface.
    pub target: Option<String>,
    /// The default per-batch budget.
    pub budget: i128,
    /// A file of REPL commands; absent means stdin.
    pub script: Option<PathBuf>,
}

impl LoopRequest {
    /// Parse the arguments after `loop`. `None` is a usage fault.
    pub(crate) fn parse(rest: &[String]) -> Option<Self> {
        let mut path: Option<PathBuf> = None;
        let mut target: Option<String> = None;
        let mut budget: Option<i128> = None;
        let mut script: Option<PathBuf> = None;
        let mut i = 0;
        while i < rest.len() {
            let arg = rest[i].as_str();
            match arg {
                "--target" => {
                    i += 1;
                    target = Some(rest.get(i)?.clone());
                }
                "--budget" => {
                    i += 1;
                    budget = Some(rest.get(i)?.parse().ok()?);
                }
                "--script" => {
                    i += 1;
                    script = Some(PathBuf::from(rest.get(i)?.clone()));
                }
                other => {
                    if other.starts_with('-') || path.is_some() {
                        return None;
                    }
                    path = Some(PathBuf::from(other));
                }
            }
            i += 1;
        }
        Some(Self {
            path: path?,
            target,
            budget: budget.unwrap_or(DEFAULT_BUDGET),
            script,
        })
    }
}

/// Run `emath loop`.
pub(crate) fn run(request: LoopRequest) -> CliExit {
    let host = match LoopHost::open(&request.path, request.target.as_deref()) {
        Ok(host) => host,
        Err(fault) => {
            eprintln!("error {}: {}", fault.code, fault.message);
            return EXIT_REFUSED;
        }
    };
    let interactive = request.script.is_none() && io::stdin().is_terminal();
    let config = ReplConfig {
        default_budget: request.budget,
        interactive,
    };
    let outcome = if let Some(script) = &request.script {
        let file = match File::open(script) {
            Ok(file) => file,
            Err(err) => {
                eprintln!("error E-CLI-IO: cannot open script {script:?}: {err}");
                return EXIT_REFUSED;
            }
        };
        let mut input = BufReader::new(file);
        let mut out = io::stdout().lock();
        run_repl(&host, &config, &mut input, &mut out)
    } else {
        let mut input = BufReader::new(io::stdin().lock());
        let mut out = io::stdout().lock();
        run_repl(&host, &config, &mut input, &mut out)
    };
    match outcome {
        Ok(_) => EXIT_OK,
        Err(fault) => {
            eprintln!("error {}: {}", fault.code, fault.message);
            EXIT_REFUSED
        }
    }
}
