//! `emath eval` / `emath repl`: receipt-carrying evaluation.
//!
//! Two lanes, discriminated by source format and flags:
//! - Genesis-format reference files (`world …:` headers) evaluate on the
//!   semantic VM through `genesis_cmd::analyze` + `emath_genesis::run`
//!   under `--world` (default `free_symbolic`).
//! - Standard function-spec `.emath` files execute an admitted `emath
//!   function` declaration through the GENERIC stack — sema admission,
//!   `definition_order` / `lower_definition` EMIR lowering, reference-VM
//!   evaluation — and return a deterministic `emath.eval-function`
//!   receipt (or a typed E-EVAL-* refusal). No genesis-only fallback, no
//!   second evaluator, no domain branch.

use super::genesis_cmd::{self, Analysis};
use super::{
    json_diagnostic_entry, json_diagnostics_entries, print_diagnostics, print_json_diagnostics,
    split_error_code, CliExit, EXIT_OK, EXIT_REFUSED,
};
use emath_artifact::JsonWriter;
use emath_core::limits::Limits;
use emath_exec_ir::interp::Value;
use emath_exec_ir::runner::{eval_definitions_values, run_declaration, TestVerdict};
use emath_genesis::{
    run as vm_run, BooleanAlienWorld, Environment, FreeTermWorld, ModularAlienWorld, OnePointWorld,
    SeededCsaWorld, VmBudget, VmOutcome,
};
use emath_ir::TypeNode;
use emath_sema::session::CompilerSession;
use emath_term::{Term, VariableId};
use emath_world_ir::WorldIr;
use std::collections::BTreeMap;
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

/// One VM evaluation with ADR-004 provenance on every print.
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

fn assign_once<T>(slot: &mut Option<T>, value: T) -> Option<()> {
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

fn refuse_eval(error: &str, json: bool) -> CliExit {
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
fn refuse_eval_coded(code: &str, message: &str, json: bool) -> CliExit {
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
fn has_declaration_content(source: &str) -> bool {
    source
        .lines()
        .map(str::trim)
        .any(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("//"))
}

/// The function-spec lane: admit the source, select one `emath function`
/// entrypoint, bind every declared input from `--set`, lower through
/// EMIR, evaluate on the reference VM, and emit the `emath.eval-function`
/// receipt. Failures are typed E-EVAL-* refusals; no partial authority.
fn eval_function_spec(args: &EvalArgs) -> CliExit {
    let source = match std::fs::read_to_string(&args.path) {
        Ok(source) if has_declaration_content(&source) => source,
        Ok(_) => {
            return refuse_eval_coded(
                "E-PKG-081",
                &format!("source has no declarations ({})", args.path.display()),
                args.json,
            );
        }
        Err(_) => {
            return refuse_eval_coded(
                "E-PKG-080",
                &format!("cannot read source file ({})", args.path.display()),
                args.json,
            );
        }
    };
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(&args.path.display().to_string(), &source);
    if result.diagnostics.has_errors() {
        print_diagnostics(&result.diagnostics);
        if args.json {
            print_json_diagnostics("eval", false, &json_diagnostics_entries(&result.diagnostics));
        }
        return EXIT_REFUSED;
    }
    let package = result.package;
    let declaration = match select_entrypoint(args, &package) {
        Ok(declaration) => declaration,
        Err((code, message)) => return refuse_eval_coded(code, &message, args.json),
    };
    if !declaration.state.is_empty() {
        return refuse_eval_coded(
            "E-EVAL-001",
            &format!(
                "entrypoint `{}` is stateful; `emath eval` executes only stateless function declarations",
                declaration.name.leaf()
            ),
            args.json,
        );
    }
    // Inputs must be Float64 or Vector[Float64]: the reference-VM value
    // vocabulary this surface binds is deliberately narrow (scalar and
    // flat vector). Anything else is E-EVAL-006.
    for field in &declaration.inputs {
        let supported = match package.ty(field.ty) {
            Some(TypeNode::Float64) => true,
            Some(TypeNode::Vector { element, .. }) => matches!(&**element, TypeNode::Float64),
            _ => false,
        };
        if !supported {
            return refuse_eval_coded(
                "E-EVAL-006",
                &format!(
                    "input `{}` has a type `emath eval` cannot bind (Float64 and Vector[Float64] only)",
                    field.name
                ),
                args.json,
            );
        }
    }
    // Binding source: explicit `--set` when present, otherwise the
    // spec's own oracle — the single worked example's `given` bindings
    // (deterministic, nothing invented; a function with zero inputs
    // evaluates with an empty binding map).
    let mut inputs_from = "set".to_string();
    let mut bindings: BTreeMap<String, Value> = BTreeMap::new();
    if args.set.is_empty() {
        (bindings, inputs_from) = match oracle_bindings(&package, declaration) {
            Ok((bindings, source)) => (bindings, source),
            Err((code, message)) => return refuse_eval_coded(code, &message, args.json),
        };
    } else {
        // `--set` closure: duplicates and undeclared names are
        // E-EVAL-005, malformed values are E-EVAL-005, values that
        // mismatch the declared slot shape are E-EVAL-006.
        let mut seen = std::collections::BTreeSet::new();
        for (name, _) in &args.set {
            if !seen.insert(name) {
                return refuse_eval_coded(
                    "E-EVAL-005",
                    &format!("duplicate `--set` binding for input `{name}`"),
                    args.json,
                );
            }
        }
        for (name, raw) in &args.set {
            if !declaration.inputs.iter().any(|field| field.name == *name) {
                return refuse_eval_coded(
                    "E-EVAL-005",
                    &format!(
                        "`--set` names `{name}`, which is not a declared input of `{}`",
                        declaration.name.leaf()
                    ),
                    args.json,
                );
            }
            let Some(value) = parse_set_value(raw) else {
                return refuse_eval_coded(
                    "E-EVAL-005",
                    &format!(
                        "cannot parse `--set {name}={raw}` as a finite-number scalar or `[vector]`"
                    ),
                    args.json,
                );
            };
            bindings.insert(name.clone(), value);
        }
    }
    for field in &declaration.inputs {
        if let Some(value) = bindings.get(&field.name) {
            let mismatch = match package.ty(field.ty) {
                Some(TypeNode::Float64) => !matches!(value, Value::F64(_)),
                Some(TypeNode::Vector { .. }) => !matches!(value, Value::Vector(_)),
                _ => true,
            };
            if mismatch {
                return refuse_eval_coded(
                    "E-EVAL-006",
                    &format!(
                        "`--set {}` value does not match the declared input type",
                        field.name
                    ),
                    args.json,
                );
            }
        }
    }
    let missing: Vec<String> = declaration
        .inputs
        .iter()
        .map(|field| field.name.clone())
        .filter(|name| !bindings.contains_key(name))
        .collect();
    if !missing.is_empty() {
        return refuse_eval_coded(
            "E-EVAL-004",
            &format!(
                "missing input binding(s): {} (bind every declared input with `--set name=value`, or give the spec a single worked example to evaluate)",
                missing.join(", ")
            ),
            args.json,
        );
    }
    let meaning_id = match package.meaning_id(&[]) {
        Ok(id) => id,
        Err(error) => {
            return refuse_eval_coded(
                "E-EVAL-007",
                &format!("meaning identity refused: {error:?}"),
                args.json,
            );
        }
    };
    let empty_state = BTreeMap::new();
    let definitions =
        match eval_definitions_values(&package, declaration, &bindings, &empty_state) {
            Ok(definitions) => definitions,
            Err(verdict) => {
                return refuse_eval_coded(
                    "E-EVAL-007",
                    &format!(
                        "evaluation refused: {}",
                        verdict.reason_text().unwrap_or_else(|| verdict.to_string())
                    ),
                    args.json,
                );
            }
        };
    // Declared outputs, in declaration order, each with a computed
    // definition; a declared output with no computed definition is
    // simply absent from the receipt (nothing computes it).
    let outputs: Vec<(String, String)> = declaration
        .outputs
        .iter()
        .filter_map(|field| {
            definitions
                .get(&field.name)
                .map(|value| (field.name.clone(), value.to_string()))
        })
        .collect();
    let inputs: Vec<(String, String)> = bindings
        .iter()
        .map(|(name, value)| (name.clone(), value.to_string()))
        .collect();
    let entrypoint = if args.function.is_some() {
        "named"
    } else {
        "sole"
    };
    emit_function_receipt(
        declaration.name.leaf(),
        entrypoint,
        &inputs_from,
        &inputs,
        &outputs,
        &meaning_id.to_string(),
        args.json,
    );
    EXIT_OK
}

/// Spec-oracle bindings for a plain eval (no `--set`): the file's own
/// single worked example supplies the inputs — its `given` values run
/// through the same generic lowering, and the example's expect verdict
/// must pass. Deterministic and never invented; a spec with several
/// examples (E-EVAL-003) or with inputs but none (E-EVAL-004) must
/// bind explicitly instead.
fn oracle_bindings(
    package: &emath_ir::SemanticPackage,
    declaration: &emath_ir::Declaration,
) -> Result<(BTreeMap<String, Value>, String), (&'static str, String)> {
    if declaration.tests.len() == 1 {
        let report = run_declaration(package, declaration);
        let run = report.tests.first().ok_or((
            "E-EVAL-007",
            "the spec's single example did not produce a run".to_string(),
        ))?;
        match &run.verdict {
            TestVerdict::Passed | TestVerdict::Computed => Ok((
                run.given.clone(),
                format!("example:{}", run.name),
            )),
            verdict => Err((
                "E-EVAL-007",
                format!(
                    "the spec's example `{}` did not pass: {}",
                    run.name,
                    verdict.reason_text().unwrap_or_else(|| verdict.to_string())
                ),
            )),
        }
    } else if declaration.tests.is_empty() {
        if declaration.inputs.is_empty() {
            Ok((BTreeMap::new(), "none".to_string()))
        } else {
            Err((
                "E-EVAL-004",
                "this function declares inputs but has no worked example to supply them; bind them with `--set name=value`"
                    .to_string(),
            ))
        }
    } else {
        Err((
            "E-EVAL-003",
            format!(
                "the file carries {} worked examples; select the entrypoint inputs explicitly with `--set name=value` (each example may bind different values)",
                declaration.tests.len()
            ),
        ))
    }
}

/// Select the evaluable entrypoint: exactly one function declaration, or
/// `--function <name>` naming a declared function (E-EVAL-002 unknown,
/// E-EVAL-001 non-function, E-EVAL-003 ambiguous).
fn select_entrypoint<'p>(
    args: &EvalArgs,
    package: &'p emath_ir::SemanticPackage,
) -> Result<&'p emath_ir::Declaration, (&'static str, String)> {
    let functions: Vec<&emath_ir::Declaration> = package
        .declarations
        .iter()
        .filter(|declaration| declaration.kind_label == "function")
        .collect();
    match &args.function {
        Some(name) => match package.declarations.iter().find(|declaration| declaration.name.leaf() == name)
        {
            Some(declaration) if declaration.kind_label == "function" => Ok(declaration),
            Some(declaration) => Err((
                "E-EVAL-001",
                format!(
                    "entrypoint `{name}` is a `{}` declaration, not an evaluable `emath function`",
                    declaration.kind_label
                ),
            )),
            None => Err((
                "E-EVAL-002",
                format!(
                    "no declaration named `{name}` in the admitted package; `--function` must name a declared `emath function`"
                ),
            )),
        },
        None => match functions.len() {
            0 => Err((
                "E-EVAL-001",
                "no `emath function` entrypoint in this file; `emath eval` executes only stateless function declarations"
                    .to_string(),
            )),
            1 => Ok(functions[0]),
            _ => Err((
                "E-EVAL-003",
                format!(
                    "{} function declarations share this file; select the entrypoint with `--function <name>` ({})",
                    functions.len(),
                    functions
                        .iter()
                        .map(|declaration| declaration.name.leaf())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
        },
    }
}

/// Parse one `--set name=value` payload: a finite decimal scalar or a
/// `[a, b, c]` vector of finite decimals. Nothing else binds.
fn parse_set_value(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = trimmed[1..trimmed.len() - 1].trim();
        if inner.is_empty() {
            return None;
        }
        let mut elements = Vec::new();
        for part in inner.split(',') {
            let value: f64 = part.trim().parse().ok()?;
            if !value.is_finite() {
                return None;
            }
            elements.push(value);
        }
        Some(Value::Vector(elements))
    } else {
        let value: f64 = trimmed.parse().ok()?;
        if value.is_finite() {
            Some(Value::F64(value))
        } else {
            None
        }
    }
}

/// Deterministic `emath.eval-function` receipt. Inputs are rendered in
/// sorted name order (`BTreeMap`), outputs in declared order, so the
/// byte stream is stable across runs and `--set` arrangement.
fn emit_function_receipt(
    function: &str,
    entrypoint: &str,
    inputs_from: &str,
    inputs: &[(String, String)],
    outputs: &[(String, String)],
    meaning_id: &str,
    json: bool,
) {
    if json {
        println!(
            "{}",
            render_function_receipt_json(
                function,
                entrypoint,
                inputs_from,
                inputs,
                outputs,
                meaning_id
            )
        );
    } else {
        println!("function {function}");
        println!("entrypoint {entrypoint}");
        println!("inputs_from {inputs_from}");
        println!("meaning_id {meaning_id}");
        for (name, value) in inputs {
            println!("input {name} = {value}");
        }
        for (name, value) in outputs {
            println!("output {name} = {value}");
        }
    }
}

fn render_function_receipt_json(
    function: &str,
    entrypoint: &str,
    inputs_from: &str,
    inputs: &[(String, String)],
    outputs: &[(String, String)],
    meaning_id: &str,
) -> String {
    let mut object = JsonWriter::object();
    object.string("schema", "emath.eval-function");
    object.int("schema_version", 1);
    object.string("function", function);
    object.string("entrypoint", entrypoint);
    object.string("inputs_from", inputs_from);
    object.string("meaning_id", meaning_id);
    object.object_field("inputs", &value_map_json(inputs));
    object.object_field("outputs", &value_map_json(outputs));
    object.finish()
}

/// One JSON object body from an ordered `(name, rendered value)` list.
fn value_map_json(entries: &[(String, String)]) -> String {
    let mut object = JsonWriter::object();
    for (name, value) in entries {
        object.string(name, value);
    }
    object.finish().trim_end().to_string()
}

/// The genesis lane (unchanged surface): evaluate a genesis-format
/// reference file on the semantic VM. `--world` selects one admitted
/// world; a lock commits the locked fingerprint; plain evals use the
/// default world.
fn eval_genesis(path: &Path, world_name: Option<&str>, json: bool) -> CliExit {
    let analysis = match genesis_cmd::analyze(path) {
        Ok(analysis) => analysis,
        Err(error) => return refuse_eval(&error, json),
    };
    let all_worlds = genesis_cmd::builtin_worlds(&analysis.inference.signature);
    let selection = match crate::meaning_cmd::resolve_locked_worlds(path, &analysis, all_worlds) {
        Ok(selection) => selection,
        Err(error) => return refuse_eval(&error, json),
    };
    if let Some(lock) = &selection.lock {
        if let Some(wanted) = world_name {
            match evaluate_named(&analysis, wanted) {
                Ok(receipt) if receipt.world_id == lock.fingerprint => {
                    emit_receipt(&receipt.with_lock(lock.lock_id), json);
                    return EXIT_OK;
                }
                Ok(_) => {
                    return refuse_eval(
                        &format!(
                            "E-LOCK-004: --world `{wanted}` disagrees with locked fingerprint {:016x}; re-open the portfolio with `emath meaning unset`",
                            lock.fingerprint
                        ),
                        json,
                    );
                }
                Err(error) => return refuse_eval(&error, json),
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
            Err(error) => refuse_eval(&error, json),
        }
    }
}

fn repl_cmd(path: &Path) -> CliExit {
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
