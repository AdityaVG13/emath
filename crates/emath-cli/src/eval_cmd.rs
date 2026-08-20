//! `emath eval` / `emath repl`: receipt-carrying evaluation over the
//! semantic VM. Presentation only -- admission and evaluation reuse
//! `genesis_cmd::analyze` and `emath_genesis::run`.

use super::genesis_cmd::{self, Analysis};
use super::{EXIT_OK, EXIT_REFUSED, usage};
use emath_artifact::JsonWriter;
use emath_genesis::{
    BooleanAlienWorld, Environment, FreeTermWorld, ModularAlienWorld, OnePointWorld,
    SeededCsaWorld, VmBudget, VmOutcome, run as vm_run,
};
use emath_term::{Term, VariableId};
use emath_world_ir::WorldIr;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

/// Admitted world labels (same roster as genesis built-in worlds).
const ADMITTED_WORLDS: [&str; 5] = [
    "free_symbolic",
    "Boolean_algebra",
    "modular_numeric",
    "one_point",
    "csa_seeded",
];

/// Default evaluation world when `--world` / `:world` is omitted.
const DEFAULT_WORLD: &str = "free_symbolic";

const UNKNOWN_REPL: &str = "unknown command; :portfolio :world <name> :explain :quit";

/// One VM evaluation with the provenance ADR-004 requires on every print.
#[derive(Clone, Debug, PartialEq, Eq)]
struct EvalReceipt {
    answer: String,
    world_name: String,
    world_id: u64,
    vm_steps: u64,
    term_id: u64,
    source_hash: u64,
    valuation: &'static str,
    lock_id: Option<u64>,
}

impl EvalReceipt {
    fn with_lock(mut self, lock_id: u64) -> Self {
        self.lock_id = Some(lock_id);
        self
    }
}

pub fn dispatch_eval(args: &[String]) -> u8 {
    match parse_eval_args(args) {
        Some(parsed) => eval_cmd(&parsed.path, parsed.world.as_deref(), parsed.json),
        None => usage("eval <file.emath> [--world <name>] [--json]"),
    }
}

pub fn dispatch_repl(args: &[String]) -> u8 {
    let mut path = None;
    for arg in args {
        if arg.starts_with('-') && arg.as_str() != "-" {
            continue;
        }
        path = Some(PathBuf::from(arg));
    }
    match path {
        Some(path) => repl_cmd(&path),
        None => usage("repl <file.emath>"),
    }
}

struct EvalArgs {
    path: PathBuf,
    world: Option<String>,
    json: bool,
}

fn parse_eval_args(args: &[String]) -> Option<EvalArgs> {
    let mut path = None;
    let mut world = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--world" => {
                index += 1;
                world = Some(args.get(index)?.clone());
            }
            other if other.starts_with('-') && other != "-" => {}
            other => path = Some(PathBuf::from(other)),
        }
        index += 1;
    }
    Some(EvalArgs {
        path: path?,
        world,
        json,
    })
}

fn eval_cmd(path: &Path, world_name: Option<&str>, json: bool) -> u8 {
    let analysis = match genesis_cmd::analyze(path) {
        Ok(analysis) => analysis,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    let all_worlds = genesis_cmd::builtin_worlds(&analysis.inference.signature);
    let selection = match crate::meaning_cmd::resolve_locked_worlds(path, &analysis, all_worlds) {
        Ok(selection) => selection,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_REFUSED;
        }
    };
    if let Some(lock) = &selection.lock {
        if let Some(wanted) = world_name {
            match evaluate_named(&analysis, wanted) {
                Ok(receipt) if receipt.world_id == lock.fingerprint => {
                    emit_receipt(&receipt.with_lock(lock.lock_id), json);
                    return EXIT_OK;
                }
                Ok(_) => {
                    eprintln!(
                        "error: E-LOCK-004: --world `{wanted}` disagrees with locked fingerprint {:016x}; re-open the portfolio with `emath meaning unset`",
                        lock.fingerprint
                    );
                    return EXIT_REFUSED;
                }
                Err(error) => {
                    eprintln!("{error}");
                    return EXIT_REFUSED;
                }
            }
        }
        match evaluate_world(&analysis, &selection.worlds[0]) {
            receipt => {
                emit_receipt(&receipt.with_lock(lock.lock_id), json);
                EXIT_OK
            }
        }
    } else {
        let wanted = world_name.unwrap_or(DEFAULT_WORLD);
        match evaluate_named(&analysis, wanted) {
            Ok(receipt) => {
                emit_receipt(&receipt, json);
                EXIT_OK
            }
            Err(error) => {
                eprintln!("{error}");
                EXIT_REFUSED
            }
        }
    }
}

fn repl_cmd(path: &Path) -> u8 {
    let analysis = match genesis_cmd::analyze(path) {
        Ok(analysis) => analysis,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_REFUSED;
        }
    };
    let all_worlds = genesis_cmd::builtin_worlds(&analysis.inference.signature);
    let selection =
        match crate::meaning_cmd::resolve_locked_worlds(path, &analysis, all_worlds.clone()) {
            Ok(selection) => selection,
            Err(error) => {
                eprintln!("{error}");
                return EXIT_REFUSED;
            }
        };
    let worlds = if selection.lock.is_some() {
        selection.worlds.clone()
    } else {
        all_worlds
    };
    let default = worlds
        .first()
        .map(|world| world.name.as_str())
        .unwrap_or(DEFAULT_WORLD);
    let Ok(mut last) = evaluate_in_world(&analysis, &worlds, default) else {
        eprintln!("{}", unknown_world_error(default));
        return EXIT_REFUSED;
    };
    if let Some(lock) = &selection.lock {
        last = last.with_lock(lock.lock_id);
    }
    emit_receipt(&last, false);

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            return EXIT_REFUSED;
        };
        match parse_repl_line(&line) {
            ReplOp::Empty => {}
            ReplOp::Quit => return EXIT_OK,
            ReplOp::Unknown => println!("{UNKNOWN_REPL}"),
            ReplOp::Portfolio => print_portfolio(&analysis, &worlds),
            ReplOp::Explain => print_explain(&last),
            ReplOp::World(name) => match resolve_world_name(&name) {
                Ok(label) => {
                    if let Some(lock) = &selection.lock {
                        let locked_name = worlds[0].name.as_str();
                        if label != locked_name {
                            eprintln!(
                                "error: E-LOCK-004: :world `{label}` disagrees with locked `{locked_name}` ({:016x}); re-open the portfolio with `emath meaning unset`",
                                lock.fingerprint
                            );
                            continue;
                        }
                    }
                    match evaluate_in_world(&analysis, &worlds, label) {
                        Ok(receipt) => {
                            let receipt = match &selection.lock {
                                Some(lock) => receipt.with_lock(lock.lock_id),
                                None => receipt,
                            };
                            emit_receipt(&receipt, false);
                            last = receipt;
                        }
                        Err(error) => eprintln!("{error}"),
                    }
                }
                Err(error) => eprintln!("{error}"),
            },
        }
    }
    EXIT_OK
}

#[derive(Debug, PartialEq, Eq)]
enum ReplOp {
    Empty,
    Quit,
    Portfolio,
    Explain,
    World(String),
    Unknown,
}

fn parse_repl_line(line: &str) -> ReplOp {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ReplOp::Empty;
    }
    if trimmed == ":quit" {
        return ReplOp::Quit;
    }
    if trimmed == ":portfolio" {
        return ReplOp::Portfolio;
    }
    if trimmed == ":explain" {
        return ReplOp::Explain;
    }
    if let Some(rest) = trimmed.strip_prefix(":world") {
        let mut parts = rest.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some(name), None) => ReplOp::World(name.to_string()),
            _ => ReplOp::Unknown,
        }
    } else {
        ReplOp::Unknown
    }
}

fn resolve_world_name(name: &str) -> Result<&'static str, String> {
    ADMITTED_WORLDS
        .iter()
        .copied()
        .find(|label| *label == name)
        .ok_or_else(|| unknown_world_error(name))
}

fn unknown_world_error(name: &str) -> String {
    format!("error: E-GEN-092: unknown world `{name}`")
}

fn evaluate_named(analysis: &Analysis, name: &str) -> Result<EvalReceipt, String> {
    let label = resolve_world_name(name)?;
    let worlds = genesis_cmd::builtin_worlds(&analysis.inference.signature);
    evaluate_in_world(analysis, &worlds, label)
}

fn evaluate_in_world(
    analysis: &Analysis,
    worlds: &[WorldIr],
    name: &str,
) -> Result<EvalReceipt, String> {
    let world = worlds
        .iter()
        .find(|world| world.name == name)
        .ok_or_else(|| unknown_world_error(name))?;
    Ok(evaluate_world(analysis, world))
}

/// Same per-world environment construction as genesis `evaluated_answer`.
fn evaluate_world(analysis: &Analysis, world: &WorldIr) -> EvalReceipt {
    let canonical = analysis.term.canonical();
    let free_env: Environment<Term> = [
        (
            VariableId("a".to_string()),
            Term::Variable(VariableId("a".to_string())),
        ),
        (
            VariableId("b".to_string()),
            Term::Variable(VariableId("b".to_string())),
        ),
    ]
    .into();
    let boolean_env: Environment<bool> = [
        (VariableId("a".to_string()), true),
        (VariableId("b".to_string()), false),
    ]
    .into();
    let modular_env: Environment<i64> = [
        (VariableId("a".to_string()), 4),
        (VariableId("b".to_string()), 7),
    ]
    .into();
    let budget = VmBudget::seed_default();
    let (answer, valuation, vm_steps) = match world.name.as_str() {
        "free_symbolic" => match vm_run(&analysis.term, &FreeTermWorld, &free_env, &budget) {
            Ok(VmOutcome::Complete { value, steps, .. }) => {
                (value.canonical(), "fixture_free", steps)
            }
            Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
        },
        "Boolean_algebra" => {
            match vm_run(&analysis.term, &BooleanAlienWorld, &boolean_env, &budget) {
                Ok(VmOutcome::Complete { value, steps, .. }) => {
                    (value.to_string(), "fixture_boolean", steps)
                }
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        "modular_numeric" => {
            match vm_run(&analysis.term, &ModularAlienWorld, &modular_env, &budget) {
                Ok(VmOutcome::Complete { value, steps, .. }) => {
                    (value.to_string(), "fixture_modular", steps)
                }
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        "one_point" => {
            #[allow(clippy::zero_sized_map_values)]
            let env: Environment<()> = analysis
                .inference
                .variables
                .iter()
                .map(|variable| (variable.clone(), ()))
                .collect();
            match vm_run(&analysis.term, &OnePointWorld, &env, &budget) {
                Ok(VmOutcome::Complete { steps, .. }) => ("•".to_string(), "one_point", steps),
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        "csa_seeded" => {
            let csa = SeededCsaWorld::baseline();
            let env: Environment<u64> = analysis
                .inference
                .variables
                .iter()
                .map(|variable| (variable.clone(), csa.variable_value(&variable.0)))
                .collect();
            match vm_run(&analysis.term, &csa, &env, &budget) {
                Ok(VmOutcome::Complete { value, steps, .. }) => {
                    (format!("{value:016x}"), "csa_baseline_seed", steps)
                }
                Ok(VmOutcome::Suspended(_)) | Err(_) => (canonical, "structural", 0),
            }
        }
        _ => (canonical, "structural", 0),
    };
    EvalReceipt {
        answer,
        world_name: world.name.clone(),
        world_id: world.identity().0,
        vm_steps,
        term_id: analysis.term_id,
        source_hash: analysis.source_hash,
        valuation,
        lock_id: None,
    }
}

fn emit_receipt(receipt: &EvalReceipt, json: bool) {
    if json {
        print!("{}", render_json(receipt));
    } else {
        print!("{}", render_text(receipt));
    }
}

fn render_text(receipt: &EvalReceipt) -> String {
    match receipt.lock_id {
        Some(lock_id) => format!(
            "value {}\nworld {} {:016x}\nvm_steps {}\nprovenance user-locked\nlock_id {:016x}\n",
            receipt.answer, receipt.world_name, receipt.world_id, receipt.vm_steps, lock_id
        ),
        None => format!(
            "value {}\nworld {} {:016x}\nvm_steps {}\n",
            receipt.answer, receipt.world_name, receipt.world_id, receipt.vm_steps
        ),
    }
}

fn render_json(receipt: &EvalReceipt) -> String {
    let mut object = JsonWriter::object();
    object.string("schema", "emath.eval-answer");
    object.int("schema_version", 1);
    object.string("answer", &receipt.answer);
    object.string("world_name", &receipt.world_name);
    object.string("world_id", &format!("{:016x}", receipt.world_id));
    object.int("vm_steps", receipt.vm_steps);
    object.int("term_id", receipt.term_id);
    object.int("source_hash", receipt.source_hash);
    if let Some(lock_id) = receipt.lock_id {
        object.string("meaning_provenance", "user-locked");
        object.string("lock_id", &format!("{lock_id:016x}"));
    }
    object.finish()
}

fn print_portfolio(analysis: &Analysis, worlds: &[WorldIr]) {
    for world in worlds {
        let receipt = evaluate_world(analysis, world);
        println!(
            "world {} {:016x} answer {}",
            receipt.world_name, receipt.world_id, receipt.answer
        );
    }
}

fn print_explain(receipt: &EvalReceipt) {
    println!(
        "world {} {:016x}\nvm_steps {}\nvaluation {}",
        receipt.world_name, receipt.world_id, receipt.vm_steps, receipt.valuation
    );
}
