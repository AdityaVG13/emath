//! `emath verify`: recheck every kept claim in a library file against
//! the checkers the library itself names. For each own function whose
//! declared output is the claim type, verify evaluates the entry (the
//! keep shape: one `unused: Int` input), builds a registry entry for
//! each distinct named checker that resolves in the library's own
//! scope, and runs the frozen `check_claim` on the stored certificate
//! — the search that produced the claim is never rerun. A claim is
//! SETTLED only when the reference VM and the emitted Rust agree on
//! the checker's verdict: verify emits the library's artifact crate
//! once per run through the existing emit path, drives each accepted
//! claim's checker natively on the certificate it built into the
//! record, and compares. One row per claim; a verdict the checker
//! rejected, a checker the library neither defines nor uses, or a
//! content-id mismatch refuses with the claim named; a disagreement
//! between the two paths refuses as a compiler defect; a checker the
//! emitter cannot lower is reported vm-only (not settled, not a
//! crash), with the emit refusal named; `--json` emits the same rows.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use emath_core::json_escape;
use emath_core::tree::{Expr, ExprKind, Item};
use emath_exec_ir::constructor_layer::{
    parse_constructor_scalar, CValue, DEFAULT_WORK, PreparedModule,
};
use emath_rust_backend::constructor_crate::emit_constructor_crate;
use emath_rust_backend::rust_ir::ast::escape_ident;
use emath_syntax::formatter::{format_expr, Prec};

use crate::{CliExit, EXIT_ADMISSION, EXIT_FAULT, EXIT_IO, EXIT_OK, EXIT_REFUSED};

pub(crate) const USAGE: &str = "verify <library.emath> [--json]";

#[derive(Clone, Debug)]
pub(crate) struct VerifyRequest {
    pub path: PathBuf,
    pub json: bool,
}

impl VerifyRequest {
    pub(crate) fn parse(rest: &[String]) -> Option<Self> {
        let (mut path, mut json) = (None, false);
        let mut i = 0;
        while i < rest.len() {
            match rest[i].as_str() {
                "--json" => json = true,
                other => {
                    if other.starts_with('-') || path.is_some() {
                        return None;
                    }
                    path = Some(PathBuf::from(other));
                }
            }
            i += 1;
        }
        Some(Self { path: path?, json })
    }
}

/// The native (emitted Rust) verdict a claim's checker returned, as
/// the row reports it: agreement (true), disagreement (false), a
/// checker or claim function the emitter cannot lower (unavailable),
/// or a claim the VM already refused, where no second path is needed
/// (skipped).
#[derive(Clone, Copy, PartialEq)]
enum Native {
    True,
    False,
    Unavailable,
    Skipped,
}

impl Native {
    fn as_str(&self) -> &'static str {
        match self {
            Native::True => "true",
            Native::False => "false",
            Native::Unavailable => "unavailable",
            Native::Skipped => "skipped",
        }
    }
}

/// One claim's recheck outcome. `kind`/`standing` are the claim's own
/// closed-set tags carried as data; `checker`/`conditions` are the
/// stored code rendered as text.
struct ClaimRow {
    name: String,
    kind: String,
    standing: String,
    checker: String,
    conditions: String,
    verdict: &'static str,
    native: Native,
    note: Option<String>,
}

fn refuse(code: &str, message: impl std::fmt::Display, exit: CliExit) -> CliExit {
    eprintln!("error {code}: {message}");
    exit
}

/// The code's surface text (a checker name, a condition program).
fn code_text(code: &emath_exec_ir::constructor_layer::Code) -> String {
    let mut text = String::new();
    format_expr(&mut text, &code.expr, Prec::Root);
    text
}

/// Every declaration name in the module's full scope (own plus the
/// retained import graph): a claim's checker resolves when its name
/// appears here.
fn declared_names(module: &PreparedModule) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Ok(tree) = module.merged_tree() {
        for item in &tree.items {
            if let Item::Declaration(decl) = item {
                names.insert(decl.name.clone());
            }
        }
    }
    names
}

fn path_expr(name: &str) -> Expr {
    Expr {
        kind: ExprKind::Path { segments: vec![name.to_string()], generics: None },
        source: emath_core::Span::default(),
    }
}

fn quoted(expr: Expr) -> Expr {
    Expr {
        kind: ExprKind::Quote { body: Box::new(expr) },
        source: emath_core::Span::default(),
    }
}

/// The two-path verdict for one VM-accepted claim (DESIGN §Verification
/// Rule 3): the claim is settled only when the emitted Rust AGREES
/// with the reference VM on the checker's verdict. A checker the
/// emitter cannot lower is vm-only (not settled, not a crash), with
/// the emit refusal carried as the reason; a disagreement is a
/// compiler defect and refuses `E-VERIFY-PATHS`. Pure so the decision
/// is testable directly, without a native build.
#[derive(Clone, Debug, PartialEq)]
pub enum TwoPath {
    /// Both paths accepted the certificate.
    Settled,
    /// The emitted Rust rejected what the VM accepted: not settled.
    CompilerDefect,
    /// The native lane could not reach a verdict; the emit refusal is
    /// the reason.
    VmOnly(String),
}

pub fn two_path(native: Result<bool, String>) -> TwoPath {
    match native {
        Ok(true) => TwoPath::Settled,
        Ok(false) => TwoPath::CompilerDefect,
        Err(reason) => TwoPath::VmOnly(reason),
    }
}

/// Run every queued claim's checker natively through the existing
/// emit path: the library's artifact crate (one emission, one compile
/// per run — the cache is the run's own, never a persistent one),
/// plus a generated std-only driver that builds each claim record,
/// extracts the certificate field (a code value the claim function
/// built), and calls the emitted checker on it. Every queued claim
/// gets an outcome; one the emitter cannot lower carries the emit
/// refusal as its error. The emitter's crate-wide refusals return
/// separately: they are shared across the run, so they print once,
/// not once per claim (each claim's own reason names only the
/// function that failed to lower).
fn native_verdicts(
    module: &PreparedModule,
    source: &str,
    path: &Path,
    queue: &[(String, String)],
) -> (BTreeMap<String, Result<bool, String>>, Vec<String>) {
    let mut outcomes: BTreeMap<String, Result<bool, String>> = BTreeMap::new();
    let emission = match emit_constructor_crate(module, path, source) {
        Ok(emission) => emission,
        Err(refusal) => {
            let reason = match refusal {
                emath_rust_backend::constructor_crate::ConstructorEmitRefusal::NotConstructor(message) => {
                    format!("artifact emission refused: {message}")
                }
                emath_rust_backend::constructor_crate::ConstructorEmitRefusal::ImportsRefused { e_code, detail } => {
                    format!("artifact emission refused ({e_code}): {detail}")
                }
            };
            for (claim, _) in queue {
                outcomes.insert(claim.clone(), Err(reason.clone()));
            }
            return (outcomes, Vec::new());
        }
    };
    // `functions` lists the module's own entries whether or not they
    // lowered: a function outside the emitter's subset appears in the
    // artifact only as a refusal comment. The membership test is the
    // emitted signature itself.
    let lib = emission.lib.as_str();
    // The emitter's crate-wide refusals are shared across the run:
    // deduplicated, they print once, not once per claim.
    let shared: Vec<String> = {
        let mut shared: Vec<String> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for message in &emission.unresolved {
            if seen.insert(message.clone()) {
                shared.push(message.clone());
            }
        }
        shared
    };
    let emitted = |name: &str| lib.contains(&format!("pub fn {}(", escape_ident(name)));
    let mut included: Vec<(String, String)> = Vec::new();
    for (claim, checker) in queue {
        let missing = if !emitted(claim) {
            Some(claim)
        } else if !emitted(checker) {
            Some(checker)
        } else {
            None
        };
        match missing {
            Some(name) => {
                // The claim's own reason names the function that failed
                // to lower. The emitter's crate-wide report is shared:
                // it returns once for the run, not once per claim.
                outcomes.insert(
                    claim.clone(),
                    Err(format!("the emitter cannot lower `{name}`")),
                );
            }
            None => included.push((claim.clone(), checker.clone())),
        }
    }
    if included.is_empty() {
        // Shared emitter refusals still return once per run.
        return (outcomes, shared);
    }

    // The run's own scratch: the artifact crate, the driver host, a
    // private target. Removed when the lane ends; nothing persists.
    let root = std::env::temp_dir().join(format!("emath_verify_native_{}", std::process::id()));
    let artifact = root.join("artifact");
    let host_dir = root.join("host");
    let crate_ident = crate_ident(&emission.package_name);
    let mut failure: Option<String> = None;
    for (relative, contents) in emission.files() {
        let path = artifact.join(relative);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, contents.as_bytes()));
        if let Err(err) = written {
            failure = Some(format!("cannot write {}: {err}", path.display()));
            break;
        }
    }
    if failure.is_none() {
        let manifest = std::fs::create_dir_all(host_dir.join("src"))
            .and_then(|()| std::fs::write(host_dir.join("Cargo.toml"), host_manifest(&crate_ident, &artifact)))
            .and_then(|()| std::fs::write(host_dir.join("src/main.rs"), driver_source(&crate_ident, &included)));
        if let Err(err) = manifest {
            failure = Some(format!("cannot write the native driver: {err}"));
        }
    }
    if failure.is_none() {
        match Command::new("cargo")
            .args(["build", "-q"])
            .current_dir(&host_dir)
            .env("CARGO_TARGET_DIR", root.join("target"))
            .output()
        {
            Err(err) => failure = Some(format!("cannot run cargo: {err}")),
            Ok(out) if !out.status.success() => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let tail: Vec<String> = stderr
                    .lines()
                    .filter(|line| line.contains("error") || line.trim_start().starts_with('|'))
                    .take(3)
                    .map(|line| line.trim().to_string())
                    .collect();
                failure = Some(format!(
                    "the emitted artifact did not compile: {}",
                    if tail.is_empty() { stderr.lines().last().unwrap_or("unknown").trim().to_string() } else { tail.join(" | ") }
                ));
            }
            Ok(_) => {}
        }
    }
    if failure.is_none() {
        match Command::new(root.join("target/debug/verify-native")).output() {
            Err(err) => failure = Some(format!("cannot run the native driver: {err}")),
            Ok(out) if !out.status.success() => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                failure = Some(format!(
                    "the native driver failed: {}",
                    stderr.lines().last().unwrap_or("unknown").trim()
                ));
            }
            Ok(out) => {
                let mut seen: BTreeSet<String> = BTreeSet::new();
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    if let Some((claim, verdict)) = line.split_once(' ') {
                        seen.insert(claim.to_string());
                        outcomes.insert(
                            claim.to_string(),
                            match verdict {
                                "true" => Ok(true),
                                "false" => Ok(false),
                                other => Err(format!("the native checker failed: {other}")),
                            },
                        );
                    }
                }
                for (claim, _) in &included {
                    if !seen.contains(claim.as_str()) {
                        outcomes.insert(claim.clone(), Err("the native driver produced no verdict".to_string()));
                    }
                }
                let _ = std::fs::remove_dir_all(&root);
                return (outcomes, shared);
            }
        }
    }
    let failure = failure.unwrap_or_else(|| "the native lane failed".to_string());
    for (claim, _) in &included {
        outcomes.insert(claim.clone(), Err(failure.clone()));
    }
    let _ = std::fs::remove_dir_all(&root);
    (outcomes, shared)
}

fn crate_ident(package_name: &str) -> String {
    package_name.replace('-', "_")
}

fn host_manifest(crate_ident: &str, artifact: &Path) -> String {
    let artifact = artifact.canonicalize().unwrap_or_else(|_| artifact.to_path_buf());
    format!(
        "# Generated by emath (deterministic; do not edit). The verify\
        \n# native lane's driver: builds each claim record, runs the\
        \n# emitted checker on the certificate it carries, prints one\
        \n# `claim verdict` line.\n\
        [package]\n\
        name = \"verify-native\"\n\
        version = \"0.1.0\"\n\
        edition = \"2024\"\n\
        publish = false\n\n\
        [workspace]\n\n\
        [[bin]]\n\
        name = \"verify-native\"\n\
        path = \"src/main.rs\"\n\n\
        [dependencies]\n\
        {crate_ident} = {{ path = {artifact:?} }}\n"
    )
}

fn driver_source(crate_ident: &str, pairs: &[(String, String)]) -> String {
    let mut text = String::from("fn main() {\n");
    for (claim, checker) in pairs {
        let claim_fn = escape_ident(claim);
        let checker_fn = escape_ident(checker);
        text.push_str(&format!(
            "    println!(\"{claim} {{}}\", match {crate_ident}::{claim_fn}(0i64) {{\n"
        ));
        text.push_str(&format!(
            "        Ok(record) => match {crate_ident}::{checker_fn}(record.certificate) {{\n"
        ));
        text.push_str("            Ok(verdict) => format!(\"{verdict}\"),\n");
        text.push_str("            Err(e) => format!(\"error {e}\"),\n");
        text.push_str("        },\n");
        text.push_str("        Err(e) => format!(\"error {e}\"),\n");
        text.push_str("    });\n");
    }
    text.push_str("}\n");
    text
}

pub(crate) fn run(request: VerifyRequest) -> CliExit {
    let checked = crate::cli_build::checked_file(&request.path);
    if let Some(first) = checked.result.diagnostics.errors().next() {
        if checked.store.files().is_empty() {
            return refuse("E-CLI-IO", &first.message, EXIT_IO);
        }
        eprintln!("{}", crate::cli_json::diagnostics_text(&checked.result.diagnostics, &checked.store));
        return EXIT_ADMISSION;
    }
    let Some(module) = &checked.result.prepared else {
        return refuse("E-VERIFY-SHAPE", "checking produced no executable module", EXIT_ADMISSION);
    };
    let library = request.path.display();
    let claims: Vec<String> = module
        .function_output_types()
        .into_iter()
        .filter(|(_, outputs)| outputs.iter().any(|name| name == "Claim"))
        .map(|(name, _)| name)
        .collect();

    // Registry entries are built per distinct named checker, only for
    // names that resolve in the library's scope: a claim naming a
    // stranger leaves its checker absent, and check_claim's own gate
    // answers the refusal.
    let declared = declared_names(module);
    let mut registry_names: Vec<String> = Vec::new();
    let mut rows: Vec<ClaimRow> = Vec::new();
    // VM-accepted claims, in row order: the native lane checks these.
    let mut native_queue: Vec<(String, String)> = Vec::new();
    let mut accepted_rows: Vec<usize> = Vec::new();
    let mut refusal: Option<(&'static str, String)> = None;
    for name in &claims {
        let inputs: BTreeMap<String, CValue> =
            BTreeMap::from([("unused".to_string(), parse_constructor_scalar("0"))]);
        let value = match module.evaluate_function(name, &inputs, DEFAULT_WORK) {
            Ok(value) => value,
            Err(err) => return refuse(&err.code, format!("{library}: {name}: {}", err.message), EXIT_FAULT),
        };
        let CValue::Record { type_name, fields } = &value else {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name} declares a Claim output but evaluated to {value}"), EXIT_REFUSED);
        };
        if type_name.as_ref() != "Claim" {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name} declares a Claim output but evaluated to a {type_name}"), EXIT_REFUSED);
        }
        let (Some(CValue::Int(kind)), Some(CValue::Int(standing))) = (fields.get("kind"), fields.get("standing")) else {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name} is missing its kind or standing tag"), EXIT_REFUSED);
        };
        let (Some(CValue::Code(checker)), Some(CValue::Code(conditions))) = (fields.get("checker"), fields.get("conditions")) else {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name} is missing its checker or conditions code"), EXIT_REFUSED);
        };
        let checker_name = code_text(checker);
        if !registry_names.contains(&checker_name) && declared.contains(&checker_name) {
            registry_names.push(checker_name.clone());
        }

        let registry = Expr {
            kind: ExprKind::List(
                registry_names
                    .iter()
                    .map(|checker| Expr {
                        kind: ExprKind::Record {
                            type_path: vec!["Checker".to_string()],
                            fields: vec![
                                ("name".to_string(), quoted(path_expr(checker))),
                                ("run".to_string(), path_expr(checker)),
                            ],
                        },
                        source: emath_core::Span::default(),
                    })
                    .collect(),
            ),
            source: emath_core::Span::default(),
        };
        let call = Expr {
            kind: ExprKind::Call {
                function: Box::new(path_expr("check_claim")),
                args: vec![path_expr("claim"), registry],
            },
            source: emath_core::Span::default(),
        };
        let verdict = match module.evaluate_expr(
            call,
            BTreeMap::from([("claim".to_string(), value.clone())]),
        ) {
            Ok(verdict) => verdict,
            Err(err) => {
                return refuse(&err.code, format!("{library}: {name}: {}", err.message), EXIT_FAULT)
            }
        };
        let CValue::Record { fields: verdict_fields, .. } = &verdict else {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name}: check_claim did not return a verdict"), EXIT_REFUSED);
        };
        let Some(CValue::Int(status)) = verdict_fields.get("status") else {
            return refuse("E-VERIFY-SHAPE", format!("{library}: {name}: the verdict carries no status"), EXIT_REFUSED);
        };
        let row = ClaimRow {
            name: name.clone(),
            kind: kind.to_string(),
            standing: standing.to_string(),
            checker: checker_name.clone(),
            conditions: code_text(conditions),
            verdict: "accepted",
            native: Native::Skipped,
            note: None,
        };
        match status.to_i64() {
            Some(1) => {
                accepted_rows.push(rows.len());
                native_queue.push((name.clone(), checker_name.clone()));
                rows.push(row);
            }
            Some(0) => {
                refusal = Some(("E-VERIFY-REJECTED", format!("{library}: {name}: the checker {checker_name} rejected the stored certificate")));
                rows.push(ClaimRow { verdict: "refused", ..row });
                break;
            }
            Some(-1) => {
                refusal = Some(("E-VERIFY-CHECKER", format!("{library}: {name}: the claim names the checker `{checker_name}`, which this library does not define or use")));
                rows.push(ClaimRow { verdict: "refused", ..row });
                break;
            }
            Some(-2) => {
                refusal = Some(("E-VERIFY-SETS", format!("{library}: {name}: the claim's kind {kind} or standing {standing} is outside the claim's closed sets")));
                rows.push(ClaimRow { verdict: "refused", ..row });
                break;
            }
            Some(-3) => {
                refusal = Some(("E-VERIFY-CONTENT-ID", format!("{library}: {name}: the stored content id does not match the claim's content")));
                rows.push(ClaimRow { verdict: "refused", ..row });
                break;
            }
            other => {
                return refuse("E-VERIFY-SHAPE", format!("{library}: {name}: check_claim returned the unknown status {other:?}"), EXIT_REFUSED);
            }
        }
    }

    // The native lane: only when the VM accepted something and no
    // refusal already stands. The two-path decision is the pure
    // `two_path` function: agreement settles the claim, the emitter's
    // limits report vm-only, a disagreement is a compiler defect and
    // refuses.
    let mut notes: Vec<String> = Vec::new();
    let mut shared: Vec<String> = Vec::new();
    if !native_queue.is_empty() && refusal.is_none() {
        let (outcomes, shared_refusals) =
            native_verdicts(module, checked.source(), &request.path, &native_queue);
        shared = shared_refusals;
        for (row_index, (claim, _checker)) in accepted_rows.iter().zip(&native_queue) {
            match two_path(outcomes[claim.as_str()].clone()) {
                TwoPath::Settled => {
                    let row = &mut rows[*row_index];
                    row.verdict = "settled";
                    row.native = Native::True;
                }
                TwoPath::CompilerDefect => {
                    let row = &mut rows[*row_index];
                    row.verdict = "refused";
                    row.native = Native::False;
                    if refusal.is_none() {
                        refusal = Some((
                            "E-VERIFY-PATHS",
                            format!(
                                "{library}: {claim}: the reference VM and the emitted Rust disagree on the checker verdict (VM accepted, native rejected): the claim is not settled; this is a compiler defect"
                            ),
                        ));
                    }
                }
                TwoPath::VmOnly(reason) => {
                    let row = &mut rows[*row_index];
                    row.verdict = "vm-only";
                    row.native = Native::Unavailable;
                    row.note = Some(reason.clone());
                    notes.push(format!("{claim}: {reason}"));
                }
            }
        }
    }

    if request.json {
        let rows: Vec<String> = rows
            .iter()
            .map(|row| {
                let mut object = format!(
                    "{{\"name\":\"{}\",\"kind\":{},\"standing\":{},\"checker\":\"{}\",\"conditions\":\"{}\",\"verdict\":\"{}\",\"native\":{}",
                    json_escape(&row.name),
                    row.kind,
                    row.standing,
                    json_escape(&row.checker),
                    json_escape(&row.conditions),
                    row.verdict,
                    match row.native {
                        Native::True => "true".to_string(),
                        Native::False => "false".to_string(),
                        Native::Unavailable => "\"unavailable\"".to_string(),
                        Native::Skipped => "\"skipped\"".to_string(),
                    },
                );
                if let Some(note) = &row.note {
                    object.push_str(&format!(",\"note\":\"{}\"", json_escape(note)));
                }
                object.push('}');
                object
            })
            .collect();
        println!("[{}]", rows.join(", "));
    } else {
        for row in &rows {
            println!(
                "{} kind={} standing={} checker={} conditions={} verdict={} native={}",
                row.name, row.kind, row.standing, row.checker, row.conditions, row.verdict, row.native.as_str()
            );
        }
    }
    for note in &notes {
        eprintln!("note: {note}");
    }
    // The emitter's crate-wide refusals are shared across every claim:
    // they print once per run, not once per claim.
    for message in &shared {
        eprintln!("note: the emitter reported: {message}");
    }
    match refusal {
        Some((code, message)) => refuse(code, message, EXIT_REFUSED),
        None => EXIT_OK,
    }
}
