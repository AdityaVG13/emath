//! Argument parsing and receipt types for `emath eval`.

use super::*;

/// Admitted world labels (same roster as genesis built-in worlds).
pub(super) const ADMITTED_WORLDS: [&str; 5] = [
    "free_symbolic",
    "Boolean_algebra",
    "modular_numeric",
    "one_point",
    "csa_seeded",
];

/// The World-IR builtin class worlds (`emath_world_ir::builtin_worlds()`):
/// declared candidate classes reachable from `eval --world` through the
/// WorldIr-driven adapter (`world_ir_eval`). Their names are the
/// declarations' own names, not the genesis fixture roster.
pub(super) const WORLD_IR_WORLD_NAMES: [&str; 8] = [
    "free-term",
    "finite-table",
    "commutative-monoid",
    "boolean-lattice",
    "integer-ring",
    "cyclic-group-z3",
    "matrix-2x2",
    "graph-union",
];

/// Default evaluation world when `--world` / `:world` is omitted.
pub(super) const DEFAULT_WORLD: &str = "free_symbolic";

pub(super) const UNKNOWN_REPL: &str = "unknown command; :portfolio :world <name> :explain :quit";

/// One VM evaluation with ADR-004 provenance on every print.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EvalReceipt {
    pub(super) answer: String,
    pub(super) world_name: String,
    pub(super) world_id: u64,
    pub(super) vm_steps: u64,
    pub(super) term_id: u64,
    pub(super) source_hash: u64,
    pub(super) valuation: &'static str,
    pub(super) lock_id: Option<u64>,
}

impl EvalReceipt {
    pub(super) fn with_lock(mut self, lock_id: u64) -> Self {
        self.lock_id = Some(lock_id);
        self
    }
}

pub(crate) fn dispatch_eval(args: EvalArgs) -> CliExit {
    let wants_function_lane = args.function.is_some() || !args.set.is_empty();
    if args.world.is_some() && wants_function_lane {
        return refuse_eval_coded(
            "E-EVAL-008",
            "`--world` selects a genesis world; it cannot be combined with `--function`/`--set`, which bind a standard function spec's inputs",
            args.json,
        );
    }
    // Source-format discrimination: genesis headers parse as genesis; any
    // standard `.emath` file (function, model, …) refuses there and takes
    // the function lane when function flags are present. A plain genesis
    // eval and `--world` on a non-genesis file keep the existing genesis
    // surface byte-for-byte.
    if genesis_cmd::analyze(&args.path).is_ok() {
        if wants_function_lane {
            return refuse_eval_coded(
                "E-EVAL-008",
                "`--function`/`--set` apply to standard function specs only; this is a genesis-format reference file",
                args.json,
            );
        }
        return eval_genesis(&args.path, args.world.as_deref(), args.json);
    }
    if args.world.is_some() {
        return eval_genesis(&args.path, args.world.as_deref(), args.json);
    }
    eval_function_spec(&args)
}

pub(crate) fn dispatch_repl(path: &Path) -> CliExit {
    repl_cmd(path)
}

pub(crate) struct EvalArgs {
    pub path: PathBuf,
    pub world: Option<String>,
    pub json: bool,
    /// `--function <name>`: named entrypoint selection for function specs.
    pub function: Option<String>,
    /// `--set name=value` bindings, in command-line order (duplicates are
    /// a typed E-EVAL-005 refusal, never a silent overwrite).
    pub set: Vec<(String, String)>,
}

pub(super) fn assign_once<T>(slot: &mut Option<T>, value: T) -> Option<()> {
    if slot.is_some() {
        None
    } else {
        *slot = Some(value);
        Some(())
    }
}

pub(crate) fn parse_repl_path(args: &[String]) -> Option<PathBuf> {
    let mut path = None;
    for arg in args {
        if arg.starts_with('-') && arg.as_str() != "-" {
            return None;
        }
        assign_once(&mut path, PathBuf::from(arg))?;
    }
    path
}

pub(crate) fn parse_eval_args(args: &[String]) -> Option<EvalArgs> {
    let mut path = None;
    let mut world = None;
    let mut json = false;
    let mut function = None;
    let mut set = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--world" => {
                index += 1;
                assign_once(&mut world, args.get(index)?.clone())?;
            }
            "--function" => {
                index += 1;
                assign_once(&mut function, args.get(index)?.clone())?;
            }
            "--set" => {
                index += 1;
                let raw = args.get(index)?;
                let Some(eq) = raw.find('=') else {
                    return None;
                };
                let name = raw[..eq].to_string();
                let value = raw[eq + 1..].to_string();
                if name.is_empty() || value.is_empty() {
                    return None;
                }
                set.push((name, value));
            }
            other if other.starts_with('-') && other != "-" => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
        index += 1;
    }
    Some(EvalArgs {
        path: path?,
        world,
        json,
        function,
        set,
    })
}

pub(super) fn refuse_eval(error: &str, json: bool) -> CliExit {
    let line = if error.starts_with("error:") {
        error.to_string()
    } else {
        format!("error: {error}")
    };
    eprintln!("{line}");
    if json {
        let (code, message) = split_error_code(&line).unwrap_or(("E-GEN-080", line.as_str()));
        print_json_diagnostics(
            "eval",
            false,
            &[json_diagnostic_entry(code, "error", message)],
        );
    }
    EXIT_REFUSED
}

/// Typed eval refusal with an explicit stable code: stderr line plus the
/// `--json` diagnostic envelope on stdout (like [`refuse_eval`], but the
/// code is never guessed from message text).
pub(super) fn refuse_eval_coded(code: &str, message: &str, json: bool) -> CliExit {
    eprintln!("error: {code}: {message}");
    if json {
        print_json_diagnostics(
            "eval",
            false,
            &[json_diagnostic_entry(code, "error", message)],
        );
    }
    EXIT_REFUSED
}

/// Non-comment content check (mirrors the `check` lane's `E-PKG-081`):
/// comment-only or blank sources have nothing to evaluate.
pub(super) fn has_declaration_content(source: &str) -> bool {
    source
        .lines()
        .map(str::trim)
        .any(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("//"))
}
