//! emath CLI: `check`, `plan`, `build`, `artifact`, `architecture`, `web`, `serve`,
//! Semantic Genesis (`parse`, `expand`, `signature`, `genesis`, `eval`,
//! `repl`, `compile --parametric`, `world show`, `portfolio show`, `meaning`),
//! and meaning-budget (`solve`, `exactness`, `freeze`, `why`, `assumptions`).
//! Host entry is [`run`] -> [`CliExit`] (not a raw `u8`). Exit codes: 0 ok, 1 refused, 2 usage/io.

#![forbid(unsafe_code)]

mod agent_cmd;
pub mod catalog;
pub mod coverage_cmd;
pub mod coverage_seed;
pub mod diagnostics;
mod eval_cmd;
pub mod genesis_cmd;
pub mod meaning_cmd;
mod library_cmd;
mod provenance_cmd;
pub mod serve_cmd;
pub mod simulate_cmd;
mod fit_cmd;
mod tooling_cmd;

use emath_build::{build_file, BuildOptions};
use emath_core::Diagnostics;
use emath_plan::{
    emit_provider_trait, lift_missing, plan as run_planner, PlanInspection, PlannerConfig,
    PlanningOutcome,
};
use emath_provider_api::{ProviderRegistry, RegistryConfig};
use emath_sema::session::CompilerSession;
use emath_syntax::ExactnessStatus;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Closed 3-way host exit. `repr(u8)` is the process mapping (0/1/2), not a
/// public `u8` return; [`run`] returns `CliExit` and `main` matches exhaustively.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CliExit {
    Ok = 0,
    Refused = 1,
    Usage = 2,
}

pub const EXIT_OK: CliExit = CliExit::Ok;
pub const EXIT_REFUSED: CliExit = CliExit::Refused;
pub const EXIT_USAGE: CliExit = CliExit::Usage;

fn exit_from_diagnostics(has_errors: bool) -> CliExit {
    if has_errors {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
}

pub use provenance_cmd::provenance_explanation;

pub fn print_diagnostics(diagnostics: &Diagnostics) {
    for item in diagnostics.items() {
        eprintln!(
            "{} {} ({}:{})",
            item.code, item.message, item.primary.file.0, item.primary.start
        );
        if let Some(help) = &item.help {
            for line in help.lines() {
                eprintln!("  {line}");
            }
        }
    }
}

/// Split `error: E-FOO-001: rest` (or the same without the `error:` prefix)
/// into a stable code and message.
pub(crate) fn split_error_code(error: &str) -> Option<(&str, &str)> {
    let error = error.strip_prefix("error: ").unwrap_or(error).trim();
    let (code, rest) = error.split_once(':')?;
    let code = code.trim();
    if code.starts_with("E-") || code.starts_with("N-") {
        Some((code, rest.trim()))
    } else {
        None
    }
}

/// One `{code,severity,message}` diagnostic object for `--json` envelopes.
pub fn json_diagnostic_entry(code: &str, severity: &str, message: &str) -> String {
    let mut entry = emath_artifact::JsonWriter::object();
    entry.string("code", code);
    entry.string("severity", severity);
    entry.string("message", message);
    entry.finish().trim_end().to_string()
}

fn json_put_opt(entry: &mut emath_artifact::JsonObject, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        entry.string(key, value);
    }
}

pub(crate) fn json_diagnostics_entries(diagnostics: &Diagnostics) -> Vec<String> {
    diagnostics
        .items()
        .iter()
        .map(|item| {
            let mut entry = emath_artifact::JsonWriter::object();
            entry.string("code", item.code);
            entry.string(
                "severity",
                match item.severity {
                    emath_core::Severity::Error => "error",
                    emath_core::Severity::Warning => "warning",
                    emath_core::Severity::Note => "note",
                },
            );
            entry.string("message", &item.message);
            json_put_opt(&mut entry, "help", item.help.as_deref());
            if let Some(pedagogy) = &item.pedagogy {
                entry.string("understood", &pedagogy.understood);
                entry.string("unknown", &pedagogy.unknown);
                entry.string("why", &pedagogy.why);
                entry.string("smallest_repair", &pedagogy.smallest_repair);
                if !pedagogy.alternatives.is_empty() {
                    entry.strings("alternatives", &pedagogy.alternatives);
                }
                json_put_opt(&mut entry, "example", pedagogy.example.as_deref());
                json_put_opt(
                    &mut entry,
                    "deeper_concept",
                    pedagogy.deeper_concept.as_deref(),
                );
                json_put_opt(
                    &mut entry,
                    "authority_consequence",
                    pedagogy.authority_consequence.as_deref(),
                );
                json_put_opt(&mut entry, "library_link", pedagogy.library_link.as_deref());
            }
            entry.finish().trim_end().to_string()
        })
        .collect()
}

/// Stdout envelope for `--json` command refusals (`check`/`eval` pattern).
pub fn diagnostics_json_document(command: &str, admitted: bool, entries: &[String]) -> String {
    let mut out = emath_artifact::JsonWriter::object();
    out.string("command", command);
    out.bool("admitted", admitted);
    out.objects("diagnostics", entries);
    out.finish()
}

pub(crate) fn print_json_diagnostics(command: &str, admitted: bool, entries: &[String]) {
    println!("{}", diagnostics_json_document(command, admitted, entries));
}

fn refuse_coded(command: &str, json: bool, exit: CliExit, code: &str, message: &str) -> CliExit {
    eprintln!("error: {code}: {message}");
    if json {
        print_json_diagnostics(
            command,
            false,
            &[json_diagnostic_entry(code, "error", message)],
        );
    }
    exit
}

/// Stdout envelope for `emath check --json`.
pub fn check_json_document(
    admitted: bool,
    package_id: &str,
    diagnostics: &Diagnostics,
    meaning_id: Option<&str>,
    units_profiles: &[(String, String)],
) -> String {
    let mut out = emath_artifact::JsonWriter::object();
    out.string("command", "check");
    out.bool("admitted", admitted);
    out.objects("diagnostics", &json_diagnostics_entries(diagnostics));
    out.string("package", package_id);
    json_put_opt(&mut out, "meaning_id", meaning_id);
    let rows: Vec<String> = units_profiles
        .iter()
        .map(|(declaration, profile)| {
            let mut row = emath_artifact::JsonWriter::object();
            row.string("declaration", declaration);
            row.string("profile", profile);
            row.finish().trim_end().to_string()
        })
        .collect();
    out.objects("units_profiles", &rows);
    out.finish()
}

fn goal_json_rows(goals: &[emath_ir::Goal]) -> Vec<String> {
    goals
        .iter()
        .map(|goal| {
            let mut row = emath_artifact::JsonWriter::object();
            row.string("kind", goal.kind.as_str());
            row.string("target", &goal.target);
            row.finish().trim_end().to_string()
        })
        .collect()
}

/// `check <file> [--verify-data] [--json]`: parse + admit, no codegen.
/// `--verify-data` (04 §5.2, emath-r3-observations-9ffu) re-hashes every
/// `sha256` declared in InstrumentRun provenance against the file on
/// disk, relative to the source file; drift refuses `E-OBS-HASH`.
pub fn check(path: &Path, json: bool, verify_data: bool) -> CliExit {
    if let Some(code) = meaning_cmd::refuse_malformed_project_lock(path) {
        return code;
    }
    let path = path.to_path_buf();
    let (mut diagnostics, package_id, units_profiles) = run_check(&path);
    if verify_data && !diagnostics.has_errors() {
        verify_declared_data(&path, &mut diagnostics);
    }
    let meaning_id = if diagnostics.has_errors() {
        None
    } else {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|source| admitted_meaning_id(&path, &source))
    };
    print_diagnostics(&diagnostics);
    if !json && !units_profiles.is_empty() {
        // §6.5 pack-table: the effective honesty declaration, printed
        // deterministically in source order (admission order).
        for (declaration, profile) in &units_profiles {
            println!("honesty: units_profile {declaration}={profile}");
        }
    }
    if json {
        // The diagnostics array carries codes and messages, not counts:
        // a checker lane must be able to assert the exact E-* code the
        // CLI refused with.
        println!(
            "{}",
            check_json_document(
                !diagnostics.has_errors(),
                &package_id,
                &diagnostics,
                meaning_id.as_ref().map(|id| id.as_str()),
                &units_profiles,
            )
        );
    }
    exit_from_diagnostics(diagnostics.has_errors())
}

/// Declared raw-data digests (04 §5.2): InstrumentRun provenance rows
/// carrying a `sha256`, as (binding, file, declared digest).
fn declared_data_digests(path: &Path) -> Vec<(String, String, String)> {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        return Vec::new();
    };
    let result = session.check(package.file);
    result
        .package
        .binding_provenance
        .iter()
        .filter_map(|(site, provenance)| match provenance {
            emath_ir::Provenance::InstrumentRun {
                file,
                sha256: Some(sha256),
                ..
            } => Some((site.binding.clone(), file.clone(), sha256.clone())),
            _ => None,
        })
        .collect()
}

/// Re-hash every declared data digest; append `E-OBS-HASH` on drift or
/// unreadable data. Changed data under an unchanged model is a different
/// artifact identity.
fn verify_declared_data(path: &Path, diagnostics: &mut Diagnostics) {
    let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
    for (binding, file, declared) in declared_data_digests(path) {
        let data_path = base.join(&file);
        let Some(bytes) = std::fs::read(&data_path).ok() else {
            diagnostics.error(
                "E-OBS-HASH",
                format!(
                    "cannot read data file for observation `{binding}` ({})",
                    data_path.display()
                ),
                emath_core::Span::default(),
            );
            continue;
        };
        let digest: String = emath_core::sha256_digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if !declared.eq_ignore_ascii_case(&digest) {
            diagnostics.error(
                "E-OBS-HASH",
                format!(
                    "data drift for observation `{binding}`: declared sha256 {declared} but {} hashes to {digest} — changed data under an unchanged model is a different artifact identity",
                    data_path.display()
                ),
                emath_core::Span::default(),
            );
        }
    }
}

fn admitted_meaning_id(path: &Path, source: &str) -> Option<emath_core::MeaningId> {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned(&path.display().to_string(), source);
    if result.diagnostics.has_errors() {
        return None;
    }
    result.package.meaning_id(&[]).ok()
}

fn source_has_content(source: &str) -> bool {
    source
        .lines()
        .map(str::trim)
        .any(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("//"))
}

fn read_emath_source(command: &str, path: &Path, json: bool) -> Result<String, CliExit> {
    match std::fs::read_to_string(path) {
        Ok(source) => {
            if source_has_content(&source) {
                Ok(source)
            } else {
                Err(refuse_coded(
                    command,
                    json,
                    EXIT_REFUSED,
                    "E-PKG-081",
                    &format!("source has no declarations ({})", path.display()),
                ))
            }
        }
        Err(_) => Err(refuse_coded(
            command,
            json,
            EXIT_USAGE,
            "E-PKG-080",
            &format!("cannot read source file ({})", path.display()),
        )),
    }
}

fn print_missing_newline(s: &str) {
    if !s.ends_with('\n') {
        println!();
    }
}

/// Stdout envelope for `emath expand --json`.
pub fn expand_json_document(
    source: &str,
    expansion: &emath_syntax::ScratchExpansion,
    meaning_id: Option<&str>,
) -> String {
    let mut notes = Vec::new();
    for note in &expansion.notes {
        let mut entry = emath_artifact::JsonWriter::object();
        entry.string("inferred", &note.inferred);
        entry.string("rationale", &note.rationale);
        entry.string("replacement", &note.replacement);
        entry.string("stability", note.stability.as_str());
        notes.push(entry.finish().trim_end().to_string());
    }
    let mut out = emath_artifact::JsonWriter::object();
    out.string("command", "expand");
    out.bool("rewritten", expansion.rewritten());
    out.string("level", expansion.level().as_str());
    out.bool("ok", !expansion.diagnostics.has_errors());
    out.string("source", source);
    out.string("expanded", &expansion.expanded);
    json_put_opt(&mut out, "meaning_id", meaning_id);
    out.objects("notes", &notes);
    let mut holes = Vec::new();
    for hole in &expansion.holes {
        holes.push(hole_json(hole));
    }
    out.objects("holes", &holes);
    let mut solve_candidates = Vec::new();
    for world in expansion.solve.menu() {
        solve_candidates.push(solve_candidate_json(
            *world,
            expansion.solve.selected(*world),
        ));
    }
    out.objects("solve_candidates", &solve_candidates);
    out.objects(
        "diagnostics",
        &json_diagnostics_entries(&expansion.diagnostics),
    );
    out.finish()
}

/// Stdout envelope for `emath exactness --json`.
pub fn exactness_json_document(
    ledger: &emath_syntax::ExactnessLedger,
    meaning_id: Option<&str>,
) -> String {
    let mut rows = Vec::new();
    for entry in &ledger.entries {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("id", &entry.inference_id);
        object.string("dimension", entry.dimension.as_str());
        object.string("status", entry.status.as_str());
        object.string("name", &entry.name);
        object.string("rationale", &entry.rationale);
        rows.push(object.finish().trim_end().to_string());
    }
    let mut out = emath_artifact::JsonWriter::object();
    out.string("command", "exactness");
    out.int(
        "declared",
        ledger.count(emath_syntax::ExactnessStatus::Declared) as u64,
    );
    out.int(
        "inferred",
        ledger.count(emath_syntax::ExactnessStatus::Inferred) as u64,
    );
    out.int(
        "constructed",
        ledger.count(emath_syntax::ExactnessStatus::Constructed) as u64,
    );
    out.int(
        "open",
        ledger.count(emath_syntax::ExactnessStatus::Open) as u64,
    );
    json_put_opt(&mut out, "meaning_id", meaning_id);
    out.objects("entries", &rows);
    out.finish()
}

/// Stdout envelope for `emath plan --json`.
/// `goals` is `[{kind, target}]` with `kind` = `GoalKind::as_str()`.
pub fn plan_json_document(admitted: bool, goals: &[emath_ir::Goal], plans: u64) -> String {
    let mut object = emath_artifact::JsonWriter::object();
    object.string("command", "plan");
    object.bool("admitted", admitted);
    object.int("plans", plans);
    object.objects("goals", &goal_json_rows(goals));
    object.finish()
}

/// Stdout envelope for `emath agent plan`. Same `goals[{kind,target}]` as
/// [`plan_json_document`]; unique keys so first-win parse sees the array.
pub fn agent_plan_json_document(admitted: bool, goals: &[emath_ir::Goal], plans: u64) -> String {
    let mut object = emath_artifact::JsonWriter::object();
    object.string("schema", "emath.agent");
    object.string("command", "plan");
    object.bool("admitted", admitted);
    object.int("plans", plans);
    object.objects("goals", &goal_json_rows(goals));
    object.finish()
}

/// Stdout envelope for `emath agent triage`. `goals` is `[{kind,target}]`
/// with `kind` = `GoalKind::as_str()` (not a count); `diagnostics` is
/// `[{code,severity,message}]`.
pub fn agent_triage_json_document(
    file: &str,
    doctor_ok: bool,
    doctor: &[String],
    admitted: bool,
    package_id: &str,
    diagnostics: &Diagnostics,
    plan_ok: bool,
    plan_error: Option<&str>,
    goals: &[emath_ir::Goal],
    plans: u64,
) -> String {
    let mut object = emath_artifact::JsonWriter::object();
    object.string("schema", "emath.agent");
    object.string("command", "triage");
    object.string("file", file);
    object.bool("doctor_ok", doctor_ok);
    object.objects("doctor", doctor);
    object.bool("admitted", admitted);
    object.string("package", package_id);
    object.objects("diagnostics", &json_diagnostics_entries(diagnostics));
    object.bool("plan_ok", plan_ok);
    if let Some(message) = plan_error {
        object.string("plan_error", message);
    }
    object.objects("goals", &goal_json_rows(goals));
    object.int("plans", plans);
    object.finish()
}

/// Stdout envelope for `emath agent check`. `diagnostics` is
/// `[{code,severity,message}]`, not a count plus concatenated `diagnostics_text`.
pub fn agent_check_json_document(
    admitted: bool,
    package_id: &str,
    diagnostics: &Diagnostics,
) -> String {
    let mut object = emath_artifact::JsonWriter::object();
    object.string("schema", "emath.agent");
    object.string("command", "check");
    object.bool("admitted", admitted);
    object.string("package", package_id);
    object.objects("diagnostics", &json_diagnostics_entries(diagnostics));
    object.finish()
}

/// Stdout envelope for `emath solve --check --json`.
pub fn solve_check_json_document(expansion: &emath_syntax::ScratchExpansion) -> String {
    let mut candidates = Vec::new();
    for world in expansion.solve.menu() {
        candidates.push(solve_candidate_json(
            *world,
            expansion.solve.selected(*world),
        ));
    }
    let mut out = emath_artifact::JsonWriter::object();
    out.string("command", "solve");
    out.bool("ok", !expansion.diagnostics.has_errors());
    out.objects("solve_candidates", &candidates);
    out.finish()
}

/// `expand <file> [--json]`: print the contracted form of L0/L1/L2 shorthand.
pub fn expand_cmd(path: &Path, json: bool) -> CliExit {
    let source = match read_emath_source("expand", path, json) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let expansion = emath_syntax::expand_scratch(&source);
    print_diagnostics(&expansion.diagnostics);
    let meaning_id = admitted_meaning_id(path, &expansion.expanded);
    if json {
        println!(
            "{}",
            expand_json_document(
                &source,
                &expansion,
                meaning_id.as_ref().map(|id| id.as_str()),
            )
        );
    } else {
        println!(
            "# emath expand: level={} rewritten={}",
            expansion.level().as_str(),
            expansion.rewritten()
        );
        if let Some(id) = meaning_id {
            println!("# meaning_id: {id}");
        }
        for note in &expansion.notes {
            println!(
                "# inferred: {} ({}) — {}",
                note.inferred,
                note.stability.as_str(),
                note.rationale
            );
            println!("# write instead: {}", note.replacement.replace('\n', " / "));
        }
        for world in expansion.solve.menu() {
            println!(
                "# solve candidate: {} type={} domain={} exactness={} method={} evidence={} default={} selected={} holes={}",
                world.as_str(),
                world.result_type(),
                world.domain(),
                world.exactness(),
                world.method(),
                world.evidence_class(),
                world.beginner_default(),
                expansion.solve.selected(*world),
                world.holes().join(",")
            );
        }
        for hole in &expansion.holes {
            println!("# {}", hole.summary());
            for candidate in &hole.candidates {
                println!(
                    "# candidate: {} ({}) labeled",
                    candidate.label,
                    candidate.kind.as_str()
                );
            }
            for rejection in &hole.rejections {
                println!("# rejected: {} — {}", rejection.attempt, rejection.reason);
            }
        }
        print!("{}", expansion.expanded);
        print_missing_newline(&expansion.expanded);
    }
    exit_from_diagnostics(expansion.diagnostics.has_errors())
}

enum ExactnessRequest {
    Ready {
        path: PathBuf,
        json: bool,
        raise: Option<emath_syntax::ExactnessDimension>,
    },
}

fn parse_exactness_request(args: &[String]) -> Option<ExactnessRequest> {
    let mut path = None;
    let mut json = false;
    let mut raise = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--raise" => {
                let token = take_nonflag_value(args, &mut index)?;
                assign_once(
                    &mut raise,
                    emath_syntax::ExactnessDimension::from_raise_token(token)?,
                )?;
            }
            other if other.starts_with('-') => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
        index += 1;
    }
    let path = path?;
    Some(ExactnessRequest::Ready { path, json, raise })
}

fn exactness_cmd(request: ExactnessRequest) -> CliExit {
    let ExactnessRequest::Ready { path, json, raise } = request;
    let source = match read_emath_source("exactness", &path, json) {
        Ok(source) => source,
        Err(code) => return code,
    };
    // A freeze lock pins the meaning that was frozen; a raise would move a
    // dimension of that frozen meaning after the fact (zql4b negative
    // control). Propose-only display without `--raise` stays allowed: the
    // budget itself is a view, not an authority change.
    if let Some(dimension) = &raise {
        let lock_path = sidecar_lock_path(&path);
        if lock_path.is_file() {
            eprintln!(
                "E-SYN-155 raise refused: {} carries a freeze lock ({}); `{}` cannot raise a \
                 frozen meaning — edit the source and refreeze instead",
                path.display(),
                lock_path.display(),
                dimension.as_str(),
            );
            return EXIT_REFUSED;
        }
    }
    let raised = match &raise {
        Some(dimension) => std::slice::from_ref(dimension),
        None => &[],
    };
    let ledger = emath_syntax::exactness_ledger_raised(&source, raised);
    let expanded = emath_syntax::expand_scratch(&source);
    let meaning_id = admitted_meaning_id(&path, &expanded.expanded);
    if json {
        println!(
            "{}",
            exactness_json_document(&ledger, meaning_id.as_ref().map(|id| id.as_str()))
        );
    } else {
        println!(
            "exactness declared={} inferred={} constructed={} open={}",
            ledger.count(emath_syntax::ExactnessStatus::Declared),
            ledger.count(emath_syntax::ExactnessStatus::Inferred),
            ledger.count(emath_syntax::ExactnessStatus::Constructed),
            ledger.count(emath_syntax::ExactnessStatus::Open)
        );
        if let Some(meaning_id) = &meaning_id {
            println!("meaning_id {meaning_id}");
        }
        for entry in &ledger.entries {
            println!(
                "{} {} {} {} — {}",
                entry.inference_id,
                entry.dimension.as_str(),
                entry.status.as_str(),
                entry.name,
                entry.rationale
            );
        }
    }
    EXIT_OK
}

fn hole_json(hole: &emath_syntax::HoleRecord) -> String {
    let mut object = emath_artifact::JsonWriter::object();
    object.string("name", &hole.name);
    object.strings("constraints", &hole.constraints);
    object.string("continuation", hole.continuation.as_str());
    if let emath_syntax::HoleContinuation::Search { goal } = &hole.continuation {
        object.string("search_goal", goal);
    }
    let candidates: Vec<String> = hole
        .candidates
        .iter()
        .map(|candidate| {
            let mut row = emath_artifact::JsonWriter::object();
            row.string("status", "labeled");
            row.string("kind", candidate.kind.as_str());
            row.string("label", &candidate.label);
            row.finish().trim_end().to_string()
        })
        .collect();
    object.objects("candidates", &candidates);
    let rejections: Vec<String> = hole
        .rejections
        .iter()
        .map(|rejection| {
            let mut row = emath_artifact::JsonWriter::object();
            row.string("attempt", &rejection.attempt);
            row.string("reason", &rejection.reason);
            row.finish().trim_end().to_string()
        })
        .collect();
    object.objects("rejections", &rejections);
    object.finish().trim_end().to_string()
}

fn solve_candidate_json(world: emath_syntax::SolveWorld, selected: bool) -> String {
    let mut object = emath_artifact::JsonWriter::object();
    object.string("label", world.as_str());
    object.string("result_type", world.result_type());
    object.string("domain", world.domain());
    object.string("exactness", world.exactness());
    object.string("method", world.method());
    object.string("evidence_class", world.evidence_class());
    let holes: Vec<String> = world
        .holes()
        .iter()
        .map(|hole| (*hole).to_string())
        .collect();
    object.strings("holes", &holes);
    object.bool("beginner_default", world.beginner_default());
    object.bool("selected", selected);
    object.finish().trim_end().to_string()
}

enum SolveRequest {
    Apply {
        path: PathBuf,
        world: emath_syntax::SolveWorld,
        json: bool,
    },
    Check {
        path: PathBuf,
        json: bool,
    },
}

enum ParsedSolve {
    Request(SolveRequest),
    Usage,
    UnknownLabel(String),
}

fn parse_solve_request(args: &[String]) -> ParsedSolve {
    let json = catalog::wants_json(args);
    let check = args.iter().any(|arg| arg == "--check");
    let mut apply = None;
    let mut path = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--apply" => match take_nonflag_value(args, &mut index) {
                Some(value) => {
                    if assign_once(&mut apply, value.to_string()).is_none() {
                        return ParsedSolve::Usage;
                    }
                    index += 1;
                }
                None => return ParsedSolve::Usage,
            },
            "--check" | "--json" | "--help" | "-h" => index += 1,
            flag if flag.starts_with('-') => return ParsedSolve::Usage,
            other => {
                if assign_once(&mut path, PathBuf::from(other)).is_none() {
                    return ParsedSolve::Usage;
                }
                index += 1;
            }
        }
    }
    let Some(path) = path else {
        return ParsedSolve::Usage;
    };
    if let Some(label) = apply {
        match emath_syntax::SolveWorld::parse_label(&label) {
            Some(world) => ParsedSolve::Request(SolveRequest::Apply { path, world, json }),
            None => ParsedSolve::UnknownLabel(label),
        }
    } else if check {
        ParsedSolve::Request(SolveRequest::Check { path, json })
    } else {
        ParsedSolve::Usage
    }
}

/// `solve --check <file>`: print labeled completions; never a naked float.
fn solve_check_cmd(request: SolveRequest) -> CliExit {
    match request {
        SolveRequest::Apply { path, world, json } => {
            let source = match read_emath_source("solve", &path, json) {
                Ok(source) => source,
                Err(code) => return code,
            };
            match emath_syntax::apply_solve_candidate(&source, world) {
                Ok((rewritten, delta)) => {
                    if json {
                        let mut out = emath_artifact::JsonWriter::object();
                        out.string("command", "solve");
                        out.string("apply", world.as_str());
                        out.string("source", &rewritten);
                        out.string("meaning_delta", &delta);
                        println!("{}", out.finish());
                    } else {
                        println!("# {delta}");
                        print!("{rewritten}");
                    }
                    EXIT_OK
                }
                Err(error) => {
                    eprintln!("error: {error}");
                    EXIT_REFUSED
                }
            }
        }
        SolveRequest::Check { path, json } => {
            let source = match read_emath_source("solve", &path, json) {
                Ok(source) => source,
                Err(code) => return code,
            };
            let expansion = emath_syntax::expand_scratch(&source);
            print_diagnostics(&expansion.diagnostics);
            if matches!(expansion.solve, emath_syntax::SolveIntent::Absent) {
                let message = format!("no `solve` intent in {}", path.display());
                eprintln!("error: {message}");
                if json {
                    print_json_diagnostics(
                        "solve",
                        false,
                        &[json_diagnostic_entry("error", "error", &message)],
                    );
                }
                return EXIT_REFUSED;
            }
            if json {
                println!("{}", solve_check_json_document(&expansion));
            } else {
                println!("solve candidates (none is a naked numeric root):");
                for world in expansion.solve.menu() {
                    let mark = if expansion.solve.selected(*world) {
                        "*"
                    } else if world.beginner_default() {
                        "default"
                    } else {
                        ""
                    };
                    println!(
                        "  {} type={} domain={} exactness={} method={} evidence={} holes=[{}] {mark}",
                        world.as_str(),
                        world.result_type(),
                        world.domain(),
                        world.exactness(),
                        world.method(),
                        world.evidence_class(),
                        world.holes().join(",")
                    );
                }
            }
            exit_from_diagnostics(expansion.diagnostics.has_errors())
        }
    }
}

fn freeze_lock_json(
    source: &str,
    frozen: &str,
    ledger: &emath_syntax::ExactnessLedger,
    meaning_id: &emath_core::MeaningId,
) -> String {
    let mut object = emath_artifact::JsonWriter::object();
    object.string("schema", "emath.freeze.lock.v1");
    object.string(
        "source_content_id",
        &emath_core::content_id_of_str(source).0,
    );
    object.string(
        "frozen_content_id",
        &emath_core::content_id_of_str(frozen).0,
    );
    object.string("meaning_id", meaning_id.as_str());
    object.bool("authority_raised", false);
    object.string("prelude", "scratch-v1");
    let none: Vec<String> = Vec::new();
    object.strings("packages", &none);
    object.strings("methods", &none);
    object.string("numeric_policy", "strict-f64");
    object.strings("providers", &["native.rust".to_string()]);
    let open: Vec<String> = ledger
        .open_holes()
        .into_iter()
        .map(|entry| format!("{}:{}", entry.dimension.as_str(), entry.name))
        .collect();
    object.strings("open", &open);
    let rows: Vec<String> = ledger
        .entries
        .iter()
        .map(|entry| {
            let mut row = emath_artifact::JsonWriter::object();
            row.string("id", &entry.inference_id);
            row.string("dimension", entry.dimension.as_str());
            row.string("status", entry.status.as_str());
            row.string("name", &entry.name);
            row.finish().trim_end().to_string()
        })
        .collect();
    object.objects("ledger", &rows);
    object.finish()
}

fn write_via_rename(path: &Path, bytes: &str) -> bool {
    let mut tmp = path.to_path_buf();
    tmp.as_mut_os_string().push(".tmp");
    let ok = std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, path).is_ok();
    if !ok {
        let _ = std::fs::remove_file(&tmp);
    }
    ok
}

fn sidecar_lock_path(out: &Path) -> PathBuf {
    let mut lock_path = out.to_path_buf();
    match lock_path.extension().and_then(|ext| ext.to_str()) {
        Some("emath") | Some("lock") => {
            lock_path.set_extension("freeze.lock.json");
        }
        Some(ext) => {
            lock_path.set_extension(format!("{ext}.freeze.lock.json"));
        }
        None => {
            lock_path.set_extension("freeze.lock.json");
        }
    }
    lock_path
}

enum FreezeRequest {
    Ready {
        path: PathBuf,
        out: Option<PathBuf>,
        json: bool,
    },
}

fn parse_freeze_request(args: &[String]) -> Option<FreezeRequest> {
    let mut path = None;
    let mut out = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--out" | "-o" => {
                assign_once(
                    &mut out,
                    PathBuf::from(take_nonflag_value(args, &mut index)?),
                )?;
            }
            other if other.starts_with('-') => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
        index += 1;
    }
    let path = path?;
    Some(FreezeRequest::Ready { path, out, json })
}

fn freeze_cmd(request: FreezeRequest) -> CliExit {
    let FreezeRequest::Ready { path, out, json } = request;
    let source = match read_emath_source("freeze", &path, json) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let expansion = emath_syntax::expand_scratch(&source);
    if expansion
        .diagnostics
        .items()
        .iter()
        .any(|item| item.code == "E-SYN-147")
    {
        eprintln!(
            "E-SYN-147 claiming exactness while holes remain open is refused; freeze does not upgrade authority"
        );
        return EXIT_REFUSED;
    }
    if expansion.diagnostics.has_errors() {
        print_diagnostics(&expansion.diagnostics);
        return EXIT_REFUSED;
    }
    let ledger = emath_syntax::exactness_ledger(&source);
    let Some(meaning_id) = admitted_meaning_id(&path, &expansion.expanded) else {
        eprintln!(
            "error: freeze requires admitted meaning; fix semantic diagnostics before freezing"
        );
        return EXIT_REFUSED;
    };
    let mut frozen = String::new();
    frozen.push_str("# emath freeze: does not raise evidence authority\n");
    for entry in ledger.open_holes() {
        frozen.push_str(&format!(
            "# emath freeze: open {} ({})\n",
            entry.dimension.as_str(),
            entry.name
        ));
    }
    frozen.push_str(&expansion.expanded);
    let lock = freeze_lock_json(&source, &frozen, &ledger, &meaning_id);
    if let Some(ref out) = out {
        if !write_via_rename(out, &frozen) {
            eprintln!("error: cannot write {}", out.display());
            return EXIT_USAGE;
        }
        let lock_path = sidecar_lock_path(out);
        if !write_via_rename(&lock_path, &lock) {
            eprintln!("error: cannot write {}", lock_path.display());
            let _ = std::fs::remove_file(out);
            return EXIT_USAGE;
        }
    }
    if json {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("command", "freeze");
        object.bool("ok", !expansion.diagnostics.has_errors());
        object.bool("authority_raised", false);
        object.int(
            "open_holes",
            ledger.count(emath_syntax::ExactnessStatus::Open) as u64,
        );
        object.string("source", &source);
        object.string("frozen", &frozen);
        object.string("lock", &lock);
        println!("{}", object.finish());
    } else if out.is_none() {
        print!("{frozen}");
        print_missing_newline(&frozen);
        println!("--- emath.freeze.lock.v1 ---");
        print!("{lock}");
        print_missing_newline(&lock);
    }
    exit_from_diagnostics(expansion.diagnostics.has_errors())
}

enum WhyRequest {
    Ready {
        path: PathBuf,
        needle: String,
        json: bool,
    },
}

fn parse_why_request(args: &[String]) -> Option<WhyRequest> {
    let mut path = None;
    let mut json = false;
    let mut needle = None;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            other if other.starts_with("inference:") => {
                assign_once(&mut needle, other.to_string())?
            }
            other if other.starts_with('-') => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
    }
    Some(WhyRequest::Ready {
        path: path?,
        needle: needle?,
        json,
    })
}

fn why_cmd(request: WhyRequest) -> CliExit {
    let WhyRequest::Ready { path, needle, json } = request;
    let source = match read_emath_source("why", &path, json) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let notes = emath_syntax::explanation_notes(&source);
    let Some(note) = notes.iter().find(|note| {
        note.inferred.starts_with(&needle) || note.inferred.contains(&format!(" {needle} "))
    }) else {
        let index = needle
            .strip_prefix("inference:")
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1));
        if let Some(note) = index.and_then(|i| notes.get(i)) {
            print_why(note, json);
            return EXIT_OK;
        }
        eprintln!("error: no such inference `{needle}`");
        return EXIT_REFUSED;
    };
    print_why(note, json);
    EXIT_OK
}

fn print_why(note: &emath_syntax::ScratchNote, json: bool) {
    if json {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("command", "why");
        object.string("inferred", &note.inferred);
        object.string("rationale", &note.rationale);
        object.string("replacement", &note.replacement);
        object.string("stability", note.stability.as_str());
        println!("{}", object.finish());
    } else {
        println!("{} ({})", note.inferred, note.stability.as_str());
        println!("{}", note.rationale);
        println!("write: {}", note.replacement.replace('\n', " / "));
    }
}

fn assumptions_cmd(path: &Path, json: bool) -> CliExit {
    let source = match read_emath_source("assumptions", path, json) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let notes: Vec<_> = emath_syntax::explanation_notes(&source)
        .into_iter()
        .filter(|note| note.stability == ExactnessStatus::Inferred)
        .collect();
    if json {
        let mut rows = Vec::new();
        for note in &notes {
            let mut object = emath_artifact::JsonWriter::object();
            object.string("inferred", &note.inferred);
            object.string("rationale", &note.rationale);
            object.string("stability", note.stability.as_str());
            rows.push(object.finish().trim_end().to_string());
        }
        let mut out = emath_artifact::JsonWriter::object();
        out.string("command", "assumptions");
        out.objects("notes", &rows);
        println!("{}", out.finish());
    } else {
        for note in &notes {
            println!("{} — {}", note.inferred, note.rationale);
        }
    }
    EXIT_OK
}

pub fn run_check(path: &Path) -> (Diagnostics, String, Vec<(String, String)>) {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        let mut diagnostics = Diagnostics::new();
        diagnostics.error(
            "E-PKG-080",
            format!("cannot read source file ({})", path.display()),
            emath_core::Span::default(),
        );
        return (diagnostics, String::new(), Vec::new());
    };
    let result = session.check(package.file);
    let package_id = result.package.content_id().0;
    (result.diagnostics, package_id, result.units_profiles)
}

/// `plan <file> [--json]`: check + goals + plans, no artifact.
pub fn plan(path: &Path, json: bool) -> CliExit {
    if let Some(code) = meaning_cmd::refuse_malformed_project_lock(path) {
        return code;
    }
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        return refuse_coded(
            "plan",
            json,
            EXIT_USAGE,
            "E-PKG-080",
            &format!("cannot read source file ({})", path.display()),
        );
    };
    let result = session.plan(package.file);
    if !result.diagnostics.is_empty() {
        print_diagnostics(&result.diagnostics);
    }
    if json {
        println!(
            "{}",
            plan_json_document(
                !result.diagnostics.has_errors(),
                &result.package.goals,
                result.plans.len() as u64,
            )
        );
    } else {
        for plan in &result.plans {
            println!(
                "plan {} goal={} policy={} class={}",
                plan.plan_id.0,
                plan.goal.index(),
                plan.policy,
                plan.artifact_class
            );
        }
    }
    exit_from_diagnostics(result.diagnostics.has_errors())
}

pub enum BuildRequest {
    Ready {
        spec: PathBuf,
        out: PathBuf,
        verify: bool,
        json: bool,
    },
}

fn parse_build_request(args: &[String]) -> Option<BuildRequest> {
    let mut path = None;
    let mut out = None;
    let mut verify = false;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" => {
                assign_once(
                    &mut out,
                    PathBuf::from(take_nonflag_value(args, &mut index)?),
                )?;
            }
            "--verify" => verify = true,
            "--json" => json = true,
            other if other.starts_with('-') => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
        index += 1;
    }
    let spec = path?;
    // One-command quick run: `emath build <file>` publishes under
    // target/emath relative to the working directory.
    let out = out.unwrap_or_else(|| PathBuf::from("target/emath"));
    Some(BuildRequest::Ready {
        spec,
        out,
        verify,
        json,
    })
}

/// `build <file> [--out <dir>] [--verify] [--json]` (default out:
/// `target/emath` under the working directory).
pub fn build(request: BuildRequest) -> CliExit {
    let BuildRequest::Ready {
        spec,
        out,
        verify,
        json,
    } = request;
    if let Some(code) = meaning_cmd::refuse_malformed_project_lock(&spec) {
        return code;
    }
    let options = BuildOptions {
        verify_generated_crate: verify,
    };
    match build_file(&spec, &out, options) {
        Ok(report) => {
            println!(
                "artifact {} (crate {}) → {}",
                report.artifact_id.0,
                report.crate_name,
                report.artifact_dir.display()
            );
            for assumption in &report.assumptions {
                println!("  assumption: {assumption}");
            }
            if json {
                let mut object = emath_artifact::JsonWriter::object();
                object.string("command", "build");
                object.string("artifact_id", &report.artifact_id.0);
                object.string("package_id", &report.package_id.0);
                object.string("crate", &report.crate_name);
                object.string("artifact_dir", &report.artifact_dir.display().to_string());
                object.strings("plan_ids", &report.plan_ids);
                object.strings("exports", &report.exports);
                println!("{}", object.finish());
            }
            EXIT_OK
        }
        Err(error) => {
            let text = error.to_string();
            eprintln!("error: {text}");
            if json {
                let (code, message) = if text.starts_with("cannot read spec:") {
                    ("E-PKG-080", text.as_str())
                } else {
                    split_error_code(&text).unwrap_or(("error", text.as_str()))
                };
                print_json_diagnostics(
                    "build",
                    false,
                    &[json_diagnostic_entry(code, "error", message)],
                );
            }
            if text.contains("admission refused") {
                EXIT_REFUSED
            } else {
                EXIT_USAGE
            }
        }
    }
}

/// Plan inspections for `emath explain <file>` / `--json`.
///
/// Each object is `PlanInspection::to_json` (`emath.plan-explanation v1`).
/// Admission failures are `Err(EXIT_REFUSED)` / `Err(EXIT_USAGE)`.
pub fn explain_inspections(path: &Path) -> Result<Vec<PlanInspection>, CliExit> {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        eprintln!("error: cannot read {}", path.display());
        return Err(EXIT_USAGE);
    };
    let result = session.plan(package.file);
    crate::print_diagnostics(&result.diagnostics);
    if result.diagnostics.has_errors() {
        return Err(EXIT_REFUSED);
    }
    Ok(inspections_from_plan_result(&result))
}

fn inspections_from_plan_result(result: &emath_sema::session::PlanResult) -> Vec<PlanInspection> {
    if result.package.goals.is_empty() {
        return vec![PlanInspection {
            policy: PlannerConfig::default().policy,
            candidates: Vec::new(),
            exclusions: Vec::new(),
            selected_plan_id: None,
            combination: None,
            checks: Vec::new(),
            budget: None,
            artifact_class: "none".into(),
        }];
    }
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    register_native_rust(&mut registry);
    let config = PlannerConfig::default();
    result
        .package
        .goals
        .iter()
        .map(|goal| run_planner(goal, &registry, &config).inspection().clone())
        .collect()
}

fn print_inspection_json(json: bool, inspection: &PlanInspection) {
    if json {
        println!("{}", inspection.to_json());
    }
}

pub enum PlannerRequest {
    Ready {
        path: PathBuf,
        json: bool,
        parametric: bool,
    },
}

fn parse_planner_request(args: &[String]) -> Option<PlannerRequest> {
    let mut path = None;
    let mut json = false;
    let mut parametric = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--parametric" => parametric = true,
            other if other.starts_with('-') => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
    }
    Some(PlannerRequest::Ready {
        path: path?,
        json,
        parametric,
    })
}

/// `planner <file.emath> [--json] [--parametric]`: run the deterministic
/// planner over the provider registry and print the machine inspection
/// (candidates, exclusions, selected plan, checks, disposition). With
/// `--parametric`, missing providers lift to a compilable Rust trait.
pub fn planner_cmd(request: PlannerRequest) -> CliExit {
    let PlannerRequest::Ready {
        path,
        json,
        parametric,
    } = request;
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(&path) else {
        return refuse_coded(
            "planner",
            json,
            EXIT_USAGE,
            "E-PKG-080",
            &format!("cannot read source file ({})", path.display()),
        );
    };
    let result = session.plan(package.file);
    if result.diagnostics.has_errors() {
        print_diagnostics(&result.diagnostics);
        return EXIT_REFUSED;
    }
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    // The in-tree static native lane (provider list: `native.rust`
    // implemented) is the Phase 1 `evaluate.rust.library` producer. An
    // empty registry would make every goal unplanned and the command a
    // dead refusal; register the real capability so supported goals plan.
    register_native_rust(&mut registry);
    let mut config = PlannerConfig::default();
    if parametric {
        config.policy = "deterministic-planner:parametric".to_string();
    }
    let mut any_unplanned = false;
    for goal in &result.package.goals {
        let mut goal = goal.clone();
        if parametric {
            goal.requirements.fallback = emath_ir::FallbackPolicy::Parametric;
        }
        let outcome = run_planner(&goal, &registry, &config);
        match &outcome {
            PlanningOutcome::Selected { plan, inspection } => {
                print_inspection_json(json, inspection);
                if !json {
                    println!(
                        "plan goal={} disposition={} candidates={} root={} checks={}",
                        goal.target,
                        plan.artifact_class,
                        inspection.candidate_count(),
                        plan.root.index(),
                        inspection.checks.join(",")
                    );
                }
            }
            PlanningOutcome::NoEligible {
                reasons,
                disposition,
                inspection,
            } => {
                any_unplanned = true;
                print_inspection_json(json, inspection);
                if !json {
                    for reason in reasons {
                        println!("excluded: {reason}");
                    }
                    println!(
                        "disposition goal={} class={}",
                        goal.target,
                        disposition.name()
                    );
                    if *disposition == emath_plan::ArtifactDisposition::Parametric {
                        let spec = lift_missing(&goal.target, &["unknown-operator".to_string()]);
                        println!("{}", emit_provider_trait(&spec));
                    }
                }
            }
            PlanningOutcome::Exhausted {
                continuation,
                disposition,
                inspection,
            } => {
                any_unplanned = true;
                print_inspection_json(json, inspection);
                if !json {
                    println!(
                        "exhausted goal={} class={} continuation={}",
                        goal.target,
                        disposition.name(),
                        continuation
                    );
                }
            }
        }
    }
    if any_unplanned {
        // A goal that could not be planned must not exit 0 (silent success).
        return EXIT_REFUSED;
    }
    EXIT_OK
}

/// Registers the Phase 1 in-tree static `native.rust` capability
/// (`evaluate.rust.library` → f64, exact, deterministic, E2 ceiling) so
/// the generic planner serves the same goals the native pipeline plans.
/// This mirrors the `provider list` status table, never a new capability.
fn register_native_rust(registry: &mut ProviderRegistry) {
    use emath_ir::EvidenceLevel;
    use emath_provider_api::{
        CapabilitySpec, CapabilityTable, ProviderIsolation, ProviderLock, RepresentationSpec,
    };
    let table = CapabilityTable {
        capabilities: vec![
            CapabilitySpec {
                // Exact produce match (`evaluate.rust.library`), not a prefix:
                // a bare `evaluate` capability would serve every evaluate goal
                // and hide unplanned produce targets (CONF-0028).
                name: "evaluate.rust.library".into(),
                semantic_subset: "rust-library".into(),
                representations: vec![RepresentationSpec {
                    name: "f64".into(),
                    exact_relation: "bit-identical".into(),
                    encode_cost: 0,
                }],
                exactness: vec!["exact".into()],
                failure_modes: vec![],
                checker_bindings: vec!["sir-checker".into()],
            },
            CapabilitySpec {
                name: "simplify".into(),
                semantic_subset: "symbolic".into(),
                representations: vec![RepresentationSpec {
                    name: "exact-integer-expression".into(),
                    exact_relation: "structural-checked".into(),
                    encode_cost: 0,
                }],
                exactness: vec!["exact".into()],
                failure_modes: vec!["E-SYM-002".into(), "E-SYM-003".into()],
                checker_bindings: vec!["native-symbolic-v1".into()],
            },
        ],
        isolation: ProviderIsolation::Static,
        lock: ProviderLock::Unlocked,
        maximum_evidence: EvidenceLevel::E2,
        deterministic: true,
    };
    if let Err(error) = registry.register("native.rust", ProviderIsolation::Static, table) {
        // Static bootstrap table is well-formed; a failure is an internal
        // registry defect, not user input. Keep planning usable by logging.
        eprintln!("warning: native.rust capability registration failed: {error:?}");
    }
}

/// `import modelica <file.mo> [--json]`: retain a Modelica subset source as
/// foreign-model declarations with adapter identity. No source rewrite.
pub fn import_modelica_cmd(path: &Path, json: bool) -> CliExit {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", path.display());
            return EXIT_USAGE;
        }
    };
    match emath_adapter_rumoca::import_modelica(&source) {
        Ok(declarations) => {
            if json {
                let mut object = emath_artifact::JsonWriter::object();
                object.string("command", "import modelica");
                object.int("declarations", declarations.len() as u64);
                let names: Vec<String> = declarations
                    .iter()
                    .map(|declaration| declaration.name.clone())
                    .collect();
                object.strings("models", &names);
                println!("{}", object.finish());
            } else {
                for declaration in &declarations {
                    println!(
                        "foreign {} adapter={} parameters={} equations={} identity={:016x}",
                        declaration.name,
                        declaration.adapter,
                        declaration.parameters.join(","),
                        declaration.equations,
                        declaration.content_identity()
                    );
                }
            }
            EXIT_OK
        }
        Err(error) => {
            eprintln!("error: {} {}", error.code, error.message);
            EXIT_REFUSED
        }
    }
}

fn list_published_artifact_ids(dir: &Path) -> Result<Vec<String>, CliExit> {
    let artifact_root = dir.join("emath");
    if !artifact_root.is_dir() {
        eprintln!(
            "error: E-EVID-105: no `emath/` state directory under {}",
            dir.display()
        );
        return Err(EXIT_USAGE);
    }
    let Ok(entries) = std::fs::read_dir(&artifact_root) else {
        eprintln!(
            "error: E-TLT-005: cannot list artifact state directory {}",
            artifact_root.display()
        );
        return Err(EXIT_USAGE);
    };
    let mut artifact_ids = Vec::new();
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            artifact_ids.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    artifact_ids.sort_unstable();
    if artifact_ids.is_empty() {
        eprintln!(
            "error: E-EVID-105: no published artifacts under {}",
            artifact_root.display()
        );
        return Err(EXIT_REFUSED);
    }
    Ok(artifact_ids)
}

/// `artifact check <dir>`: independent verification of every published
/// artifact under `<dir>/emath/<artifact-id>` via `emath-evidence`'s `checker` module
/// (one identity, one checker; empty state dirs are refused).
pub fn artifact_check(dir: &Path) -> CliExit {
    let artifact_ids = match list_published_artifact_ids(dir) {
        Ok(ids) => ids,
        Err(code) => return code,
    };
    let artifact_root = dir.join("emath");
    let mut ok = true;
    for id in artifact_ids {
        let root = artifact_root.join(&id);
        match emath_evidence::checker::check_artifact_dir(&root) {
            Ok(report) if report.valid() => {
                println!("artifact {id}: verified ({} files)", report.files_verified);
            }
            Ok(report) => {
                for issue in &report.issues {
                    eprintln!("artifact {id}: {}: {}", issue.code, issue.message);
                }
                ok = false;
            }
            Err(error) => {
                eprintln!("artifact {id}: FAILED: {error}");
                ok = false;
            }
        }
    }
    if ok {
        EXIT_OK
    } else {
        EXIT_REFUSED
    }
}

/// `artifact battery <dir>`: run the seeded negative-control battery over
/// every published artifact. Each seed must be refused with the code the
/// checker assigns; an escape is an admitted dishonest artifact and
/// refuses the command (CI-visible lane over real staged output).
pub fn artifact_battery(dir: &Path) -> CliExit {
    let artifact_ids = match list_published_artifact_ids(dir) {
        Ok(ids) => ids,
        Err(code) => return code,
    };
    let artifact_root = dir.join("emath");
    let mut ok = true;
    for id in artifact_ids {
        let root = artifact_root.join(&id);
        match emath_evidence::checker::artifact_input_from_dir(&root) {
            Ok(input) => {
                let run = emath_evidence::checker::run_standard_battery(&input);
                for control in &run.refused {
                    println!("artifact {id}: control refused ({control})");
                }
                for (control, detail) in &run.escaped {
                    eprintln!("artifact {id}: control ESCAPED ({control}): {detail}");
                }
                if run.all_refused() {
                    println!(
                        "artifact {id}: battery clean ({} controls, {})",
                        run.refused.len() + run.escaped.len(),
                        run.refused.join(", ")
                    );
                } else {
                    ok = false;
                }
            }
            Err(error) => {
                eprintln!("artifact {id}: battery FAILED: {error}");
                ok = false;
            }
        }
    }
    if ok {
        EXIT_OK
    } else {
        EXIT_REFUSED
    }
}

/// Stdout document for `emath architecture --json`.
pub fn architecture_json() -> String {
    let pipeline = ".emath -> SIR -> GIR -> resolution plan -> EMIR -> Rust artifact -> protected host promotion";
    let paths: Vec<String> = emath_artifact::required_artifact_paths()
        .iter()
        .map(ToString::to_string)
        .collect();
    let mut object = emath_artifact::JsonWriter::object();
    object.string("schema", "emath.architecture");
    object.string("pipeline", pipeline);
    object.strings("required_paths", &paths);
    object.finish()
}

/// `architecture [--json]`: provider-neutral pipeline description.
pub fn architecture(json: bool) -> CliExit {
    if json {
        print!("{}", architecture_json());
    } else {
        let pipeline = ".emath -> SIR -> GIR -> resolution plan -> EMIR -> Rust artifact -> protected host promotion";
        let paths: Vec<String> = emath_artifact::required_artifact_paths()
            .iter()
            .map(ToString::to_string)
            .collect();
        println!("{pipeline}");
        println!("provider-neutral required paths: {paths:?}");
    }
    EXIT_OK
}

/// `help` output. Generated from the command catalog so usage and summary
/// cannot drift from `emath help <command>` / `emath capabilities --json`.
pub fn help_text() -> String {
    let mut out = String::from("emath compiler (Phase 1 + Semantic Genesis G0-G3)\n\nusage:\n");
    for command in catalog::COMMANDS {
        let Some(usage) = catalog::command_usage(command) else {
            continue;
        };
        let Some(summary) = catalog::command_summary(command) else {
            continue;
        };
        out.push_str("  emath ");
        out.push_str(usage);
        out.push('\n');
        out.push_str("      ");
        out.push_str(summary);
        out.push('\n');
    }
    out.push_str("\nexit codes: 0 ok, 1 refused/admission diagnostics, 2 usage or io error\n");
    out
}

/// Entry used by main; keeps the CLI testable.
pub fn run(args: &[String]) -> CliExit {
    match parse_cli(args) {
        ParsedCli::Empty => {
            print!("{}", help_text());
            EXIT_OK
        }
        ParsedCli::MetaHelp { rest } => help_cmd(rest),
        ParsedCli::MetaVersion { rest } => catalog_read_cmd("version", rest, || {
            println!("{}", catalog::version_text());
            EXIT_OK
        }),
        ParsedCli::CommandHelp { name } => print_command_help(name),
        ParsedCli::UnknownFlag { code } => code,
        ParsedCli::Usage(message) => usage(message),
        ParsedCli::Unknown(name) => unknown_command(name),
        ParsedCli::Known(command) => run_command(command),
    }
}

enum ParsedCli<'a> {
    Empty,
    MetaHelp { rest: &'a [String] },
    MetaVersion { rest: &'a [String] },
    CommandHelp { name: &'a str },
    UnknownFlag { code: CliExit },
    Usage(&'static str),
    Known(Command),
    Unknown(&'a str),
}

enum Command {
    Check(FileJsonRequest),
    Plan(FileJsonRequest),
    Planner(PlannerRequest),
    Build(BuildRequest),
    Expand(FileJsonRequest),
    Assumptions(FileJsonRequest),
    Solve(ParsedSolve),
    Exactness(ExactnessRequest),
    Freeze(FreezeRequest),
    Why(WhyRequest),
    Parse(ParseRequest),
    Signature(SignatureRequest),
    Genesis(GenesisRequest),
    Compile(CompileRequest),
    RobotDocs,
    Provider(ProviderRequest),
    Fork(ForkRequest),
    Capabilities,
    Eval(eval_cmd::EvalArgs),
    Simulate(simulate_cmd::SimulateArgs),
    Fit(fit_cmd::FitArgs),
    Repl { path: PathBuf },
    WorldShow { id: String, dir: PathBuf },
    PortfolioShow { id: String, dir: PathBuf },
    Meaning(meaning_cmd::MeaningRequest),
    LibraryMount { name: String },
    ImportModelica { path: PathBuf, json: bool },
    ArtifactCheck(PathBuf),
    ArtifactBattery(PathBuf),
    Architecture { json: bool },
    Web(serve_cmd::ServeArgs),
    Serve(serve_cmd::ServeArgs),
    New { name: String, out: PathBuf },
    Fmt {
        path: Option<PathBuf>,
        value: Option<String>,
        sf: Option<u32>,
        from: Option<String>,
        format: Option<String>,
    },
    Migrate {
        path: PathBuf,
        fix: bool,
        check_only: bool,
        receipt: Option<PathBuf>,
        list_rules: bool,
    },
    Explain(ExplainRequest),
    Run { path: PathBuf, out: PathBuf },
    Test { path: PathBuf, out: PathBuf },
    Bench { path: PathBuf },
    Verify { dir: PathBuf },
    Inspect { dir: PathBuf, json: bool },
    Diff { a: PathBuf, b: PathBuf, json: bool },
    Doctor { json: bool },
    Vendor { out: PathBuf },
    Agent(AgentRequest),
    Coverage(Vec<String>),
}

pub(crate) enum ExplainRequest {
    File {
        path: PathBuf,
        symbol: Option<String>,
        provenance: bool,
        json: bool,
        show_defaults: bool,
    },
    Law {
        json: bool,
    },
}

pub(crate) enum AgentRequest {
    Check { path: PathBuf },
    Plan { path: PathBuf },
    Build { path: PathBuf, out: PathBuf },
    Triage { path: PathBuf },
    Propose { path: PathBuf },
}

pub(crate) enum ProviderRequest {
    List { json: bool },
    Inspect { id: String },
    Test { id: String, json: bool },
}

pub(crate) enum ForkRequest {
    Status { json: bool },
    Sync { dry_run: bool, json: bool },
}

enum ParseKnownError {
    Usage(&'static str),
    Unknown,
}

fn parse_cli(args: &[String]) -> ParsedCli<'_> {
    let Some(first) = args.first() else {
        return ParsedCli::Empty;
    };
    let rest = &args[1..];
    match first.as_str() {
        "help" | "--help" | "-h" => return ParsedCli::MetaHelp { rest },
        "version" | "--version" | "-V" => return ParsedCli::MetaVersion { rest },
        _ => {}
    }
    if catalog::wants_help(rest) {
        return ParsedCli::CommandHelp { name: first };
    }
    if let Some(code) = catalog::reject_unknown_flags(first, rest) {
        return ParsedCli::UnknownFlag { code };
    }
    match parse_known(first.as_str(), rest) {
        Ok(command) => ParsedCli::Known(command),
        Err(ParseKnownError::Usage(message)) => ParsedCli::Usage(message),
        Err(ParseKnownError::Unknown) => ParsedCli::Unknown(first),
    }
}

fn parse_known(name: &str, rest: &[String]) -> Result<Command, ParseKnownError> {
    match name {
        "check" => parse_check_request(rest)
            .map(Command::Check)
            .ok_or(ParseKnownError::Usage(
                "check <file.emath> [--verify-data] [--json]",
            )),
        "plan" => parse_file_json_request(rest)
            .map(Command::Plan)
            .ok_or(ParseKnownError::Usage("plan <file.emath> [--json]")),
        "planner" => {
            parse_planner_request(rest)
                .map(Command::Planner)
                .ok_or(ParseKnownError::Usage(
                    "planner <file.emath> [--json] [--parametric]",
                ))
        }
        "build" => parse_build_request(rest)
            .map(Command::Build)
            .ok_or(ParseKnownError::Usage(
                "build <file.emath> [--out <dir>] [--verify] [--json]",
            )),
        "parse" => parse_parse_request(rest)
            .map(Command::Parse)
            .ok_or(ParseKnownError::Usage(
                "parse --forest <file.emath> [--out <dir>]",
            )),
        "expand" => parse_file_json_request(rest)
            .map(Command::Expand)
            .ok_or(ParseKnownError::Usage("expand <file.emath> [--json]")),
        "solve" => Ok(Command::Solve(parse_solve_request(rest))),
        "exactness" => {
            parse_exactness_request(rest)
                .map(Command::Exactness)
                .ok_or(ParseKnownError::Usage(
                    "exactness <file.emath> [--json] [--raise units]",
                ))
        }
        "freeze" => parse_freeze_request(rest)
            .map(Command::Freeze)
            .ok_or(ParseKnownError::Usage(
                "freeze <file.emath> [--out <file>] [--json]",
            )),
        "why" => parse_why_request(rest)
            .map(Command::Why)
            .ok_or(ParseKnownError::Usage(
                "why <file.emath> inference:N [--json]",
            )),
        "assumptions" => parse_file_json_request(rest)
            .map(Command::Assumptions)
            .ok_or(ParseKnownError::Usage("assumptions <file.emath> [--json]")),
        "signature" => {
            parse_signature_request(rest)
                .map(Command::Signature)
                .ok_or(ParseKnownError::Usage(
                    "signature <file.emath> [--out <dir>]",
                ))
        }
        "genesis" => parse_genesis_request(rest)
            .map(Command::Genesis)
            .ok_or(ParseKnownError::Usage("genesis <file.emath> --out <dir>")),
        "eval" => eval_cmd::parse_eval_args(rest)
            .map(Command::Eval)
            .ok_or(ParseKnownError::Usage(
                "eval <file.emath> [--world <name>] [--json]",
            )),
        "simulate" => match simulate_cmd::parse_simulate_args(rest) {
            Ok(parsed) => Ok(Command::Simulate(parsed)),
            Err(message) => {
                eprintln!("error: {message}");
                Err(ParseKnownError::Usage(
                    "simulate <file.emath> [--model NAME] [--dt N] [--t0 N] [--t1 N] [--method euler|rk4|rk45|backward-euler|velocity-verlet] [--atol N] [--rtol N] [--dt-max N] [--event name=value] [--set name=value] [--json]",
                ))
            }
        },
        "fit" => match fit_cmd::parse_fit_args(rest) {
            Ok(parsed) => Ok(Command::Fit(parsed)),
            Err(message) => {
                eprintln!("error: {message}");
                Err(ParseKnownError::Usage("fit <file.emath> [--json]"))
            }
        },
        "repl" => eval_cmd::parse_repl_path(rest)
            .map(|path| Command::Repl { path })
            .ok_or(ParseKnownError::Usage("repl <file.emath>")),
        "compile" => {
            parse_compile_request(rest)
                .map(Command::Compile)
                .ok_or(ParseKnownError::Usage(
                    "compile --parametric <file.emath> --out <dir> [--world LABEL]",
                ))
        }
        "world" => parse_show_named(rest)
            .map(|(id, dir)| Command::WorldShow { id, dir })
            .ok_or(ParseKnownError::Usage("world show WORLD_ID --dir <dir>")),
        "portfolio" => parse_show_named(rest)
            .map(|(id, dir)| Command::PortfolioShow { id, dir })
            .ok_or(ParseKnownError::Usage(
                "portfolio show PORTFOLIO_ID --dir <dir>",
            )),
        "meaning" => meaning_cmd::parse_meaning_request(rest)
            .map(Command::Meaning)
            .map_err(ParseKnownError::Usage),
        "library" => match rest {
            [sub, name] if sub == "mount" => Ok(Command::LibraryMount {
                name: name.clone(),
            }),
            _ => Err(ParseKnownError::Usage("library mount <name>")),
        },
        "import" => parse_import_modelica(rest)
            .map(|(path, json)| Command::ImportModelica { path, json })
            .ok_or(ParseKnownError::Usage("import modelica <file.mo> [--json]")),
        "artifact" => match rest {
            [sub, dir] if sub == "check" => Ok(Command::ArtifactCheck(PathBuf::from(dir))),
            [sub, dir] if sub == "battery" => Ok(Command::ArtifactBattery(PathBuf::from(dir))),
            _ => Err(ParseKnownError::Usage("artifact check|battery <dir>")),
        },
        "architecture" => {
            if no_extra_positionals(rest) {
                Ok(Command::Architecture {
                    json: catalog::wants_json(rest),
                })
            } else {
                Err(ParseKnownError::Usage("architecture [--json]"))
            }
        }
        "web" => match serve_cmd::parse_serve_args(rest) {
            Ok(parsed) => Ok(Command::Web(parsed)),
            Err(message) => {
                eprintln!("error: {message}");
                Err(ParseKnownError::Usage(
                    "web [--port N] [--no-open] [--dist PATH]",
                ))
            }
        },
        "serve" => match serve_cmd::parse_serve_args(rest) {
            Ok(parsed) => Ok(Command::Serve(parsed)),
            Err(message) => {
                eprintln!("error: {message}");
                Err(ParseKnownError::Usage(
                    "serve [--port N] [--no-open] [--dist PATH]",
                ))
            }
        },
        "capabilities" => {
            if no_extra_positionals(rest) {
                Ok(Command::Capabilities)
            } else {
                Err(ParseKnownError::Usage("capabilities [--json]"))
            }
        }
        "coverage" => Ok(Command::Coverage(rest.to_vec())),
        "robot-docs" => match rest {
            [] => Ok(Command::RobotDocs),
            [guide] if guide == "guide" || guide == "--guide" => Ok(Command::RobotDocs),
            _ => Err(ParseKnownError::Usage("robot-docs [guide]")),
        },
        "new" => parse_new_request(rest)
            .map(|(name, out)| Command::New { name, out })
            .ok_or(ParseKnownError::Usage("new <name> [--out <dir>]")),
        "fmt" => parse_fmt_request(rest),
        "migrate" => parse_migrate_request(rest),
        "explain" => {
            parse_explain_request(rest)
                .map(Command::Explain)
                .ok_or(ParseKnownError::Usage(
                    "explain <file.emath> [<symbol>] [--provenance] [--show-defaults] | explain \
                     E-LAW-001 [--json]",
                ))
        }
        "run" => parse_path_out_request(rest)
            .map(|(path, out)| Command::Run { path, out })
            .ok_or(ParseKnownError::Usage("run <file.emath> [--out <dir>]")),
        "test" => parse_path_out_request(rest)
            .map(|(path, out)| Command::Test { path, out })
            .ok_or(ParseKnownError::Usage("test <file.emath> [--out <dir>]")),
        "bench" => parse_required_path(rest)
            .map(|path| Command::Bench { path })
            .ok_or(ParseKnownError::Usage("bench <file.emath>")),
        "verify" => parse_required_path(rest)
            .map(|dir| Command::Verify { dir })
            .ok_or(ParseKnownError::Usage("verify <artifact-dir>")),
        "inspect" => parse_inspect_request(rest)
            .map(|(dir, json)| Command::Inspect { dir, json })
            .ok_or(ParseKnownError::Usage("inspect <artifact-dir> [--json]")),
        "diff" => parse_diff_request(rest)
            .map(|(a, b, json)| Command::Diff { a, b, json })
            .ok_or(ParseKnownError::Usage("diff <a.emath> <b.emath> [--json]")),
        "doctor" => {
            if no_extra_positionals(rest) {
                Ok(Command::Doctor {
                    json: catalog::wants_json(rest),
                })
            } else {
                Err(ParseKnownError::Usage("doctor [--json]"))
            }
        }
        "vendor" => parse_vendor_request(rest)
            .map(|out| Command::Vendor { out })
            .ok_or(ParseKnownError::Usage("vendor --out <dir>")),
        "provider" => {
            parse_provider_request(rest)
                .map(Command::Provider)
                .ok_or(ParseKnownError::Usage(
                    "provider list|inspect <id>|test <id> [--json]",
                ))
        }
        "fork" => parse_fork_request(rest)
            .map(Command::Fork)
            .ok_or(ParseKnownError::Usage(
                "fork status|sync [--dry-run] [--json]",
            )),
        "agent" => parse_agent_request(rest)
            .map(Command::Agent)
            .ok_or(ParseKnownError::Usage(
                "agent check|plan|build|triage|propose <file> [--out <dir>]",
            )),
        _ => Err(ParseKnownError::Unknown),
    }
}

/// `fmt [<file.emath>]` or value mode:
/// `fmt --value <literal> [--sf N] [--from UNIT] [--format "0.1 %"|preferred_unit UNIT]`
fn parse_fmt_request(rest: &[String]) -> Result<Command, ParseKnownError> {
    const USAGE: &str = "fmt [<file.emath>] | fmt --value <literal> \
                         [--sf N] [--from UNIT] [--format \"0.1 %\"|preferred_unit UNIT]";
    let mut path: Option<PathBuf> = None;
    let mut value: Option<String> = None;
    let mut sf: Option<u32> = None;
    let mut from: Option<String> = None;
    let mut format: Option<String> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--value" => {
                i += 1;
                value = rest.get(i).map(|s| s.to_string());
                if value.is_none() {
                    return Err(ParseKnownError::Usage(USAGE));
                }
            }
            "--sf" => {
                i += 1;
                match rest.get(i).and_then(|s| s.parse::<u32>().ok()) {
                    Some(n) => sf = Some(n),
                    None => return Err(ParseKnownError::Usage(USAGE)),
                }
            }
            "--from" => {
                i += 1;
                from = rest.get(i).map(|s| s.to_string());
                if from.is_none() {
                    return Err(ParseKnownError::Usage(USAGE));
                }
            }
            "--format" => {
                i += 1;
                if rest.get(i).is_none() {
                    return Err(ParseKnownError::Usage(USAGE));
                }
                format = Some(rest[i..].join(" "));
                break;
            }
            other if !other.starts_with('-') && path.is_none() && value.is_none() => {
                path = Some(PathBuf::from(other));
            }
            _ => return Err(ParseKnownError::Usage(USAGE)),
        }
        i += 1;
    }
    // Exactly one of file mode or value mode.
    if value.is_some() == path.is_some() {
        return Err(ParseKnownError::Usage(USAGE));
    }
    Ok(Command::Fmt {
        path,
        value,
        sf,
        from,
        format,
    })
}

/// `migrate <file.emath> [--fix] [--check] [--receipt <path>] | migrate --list-rules`
/// (05 §5, emath-r3-migrate-contract-b75y / emath-7ijoe). Lossless
/// rewrites only; the receipt is the canonical stable-JSON artifact.
fn parse_migrate_request(rest: &[String]) -> Result<Command, ParseKnownError> {
    const USAGE: &str = "migrate <file.emath> [--fix] [--check] [--receipt <path>] | \
                         migrate --list-rules";
    if matches!(rest, [flag] if flag == "--list-rules") {
        return Ok(Command::Migrate {
            path: PathBuf::new(),
            fix: false,
            check_only: false,
            receipt: None,
            list_rules: true,
        });
    }
    let mut path: Option<PathBuf> = None;
    let mut fix = false;
    let mut check_only = false;
    let mut receipt: Option<PathBuf> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--fix" => fix = true,
            "--check" => check_only = true,
            "--receipt" => {
                i += 1;
                receipt = rest.get(i).map(PathBuf::from);
                if receipt.is_none() {
                    return Err(ParseKnownError::Usage(USAGE));
                }
            }
            other if !other.starts_with('-') && path.is_none() => {
                path = Some(PathBuf::from(other));
            }
            _ => return Err(ParseKnownError::Usage(USAGE)),
        }
        i += 1;
    }
    let Some(path) = path else {
        return Err(ParseKnownError::Usage(USAGE));
    };
    if check_only && fix {
        return Err(ParseKnownError::Usage(
            "migrate: --check and --fix are mutually exclusive",
        ));
    }
    Ok(Command::Migrate {
        path,
        fix,
        check_only,
        receipt,
        list_rules: false,
    })
}

fn run_command(command: Command) -> CliExit {
    match command {
        Command::Check(FileJsonRequest::Ready {
            path,
            json,
            verify_data,
        }) => check(&path, json, verify_data),
        Command::Plan(FileJsonRequest::Ready { path, json, .. }) => plan(&path, json),
        Command::Planner(request) => planner_cmd(request),
        Command::Build(request) => build(request),
        Command::Parse(ParseRequest::Ready {
            path,
            out,
            forest_only,
        }) => genesis_cmd::parse_cmd(&path, out.as_ref(), forest_only),
        Command::Expand(FileJsonRequest::Ready { path, json, .. }) => expand_cmd(&path, json),
        Command::Solve(ParsedSolve::Request(request)) => solve_check_cmd(request),
        Command::Solve(ParsedSolve::Usage) => {
            usage("solve --check <file.emath> [--json] [--apply <label>]")
        }
        Command::Solve(ParsedSolve::UnknownLabel(label)) => {
            eprintln!("error: unknown solve candidate `{label}`");
            EXIT_REFUSED
        }
        Command::Exactness(request) => exactness_cmd(request),
        Command::Freeze(request) => freeze_cmd(request),
        Command::Why(request) => why_cmd(request),
        Command::Assumptions(FileJsonRequest::Ready { path, json, .. }) => {
            assumptions_cmd(&path, json)
        }
        Command::Signature(SignatureRequest::Ready { path, out }) => {
            genesis_cmd::signature_cmd(&path, out.as_ref())
        }
        Command::Genesis(GenesisRequest::Ready { path, out }) => {
            genesis_cmd::genesis_cmd(&path, &out)
        }
        Command::Eval(args) => eval_cmd::dispatch_eval(args),
        Command::Simulate(args) => simulate_cmd::dispatch_simulate(&args),
        Command::Fit(args) => fit_cmd::dispatch_fit(&args),
        Command::Repl { path } => eval_cmd::dispatch_repl(&path),
        Command::Compile(request) => genesis_cmd::compile_cmd(request),
        Command::WorldShow { id, dir } => genesis_cmd::world_show_cmd(&id, &dir),
        Command::PortfolioShow { id, dir } => genesis_cmd::portfolio_show_cmd(&id, &dir),
        Command::Meaning(request) => meaning_cmd::dispatch(request),
        Command::LibraryMount { name } => library_cmd::mount_cmd(&name),
        Command::ImportModelica { path, json } => import_modelica_cmd(&path, json),
        Command::ArtifactCheck(dir) => artifact_check(&dir),
        Command::ArtifactBattery(dir) => artifact_battery(&dir),
        Command::Architecture { json } => architecture(json),
        Command::Web(args) | Command::Serve(args) => serve_cmd::web_cmd(args),
        Command::RobotDocs => robot_docs_cmd(),
        Command::Provider(request) => tooling_cmd::provider_cmd(request),
        Command::Fork(request) => tooling_cmd::fork_cmd(request),
        Command::Capabilities => {
            print!("{}", catalog::capabilities_json());
            EXIT_OK
        }
        Command::New { name, out } => tooling_cmd::new_cmd(&name, &out),
        Command::Fmt {
            path,
            value,
            sf,
            from,
            format,
        } => match value {
            Some(raw) => tooling_cmd::fmt_value_cmd(&raw, sf, from.as_deref(), format.as_deref()),
            None => tooling_cmd::fmt_cmd(path.as_deref().expect("path or --value at parse")),
        },
        Command::Migrate {
            path,
            fix,
            check_only,
            receipt,
            list_rules,
        } => tooling_cmd::migrate_cmd(&path, fix, check_only, receipt.as_deref(), list_rules),
        Command::Explain(request) => tooling_cmd::explain_cmd(request),
        Command::Run { path, out } => tooling_cmd::run_cmd(&path, &out),
        Command::Test { path, out } => tooling_cmd::test_cmd(&path, &out),
        Command::Bench { path } => tooling_cmd::bench_cmd(&path),
        Command::Verify { dir } => tooling_cmd::verify_cmd(&dir),
        Command::Inspect { dir, json } => tooling_cmd::inspect_cmd(&dir, json),
        Command::Diff { a, b, json } => tooling_cmd::diff_cmd(&a, &b, json),
        Command::Doctor { json } => tooling_cmd::doctor_cmd(json),
        Command::Vendor { out } => tooling_cmd::vendor_cmd(&out),
        Command::Agent(request) => agent_cmd::agent_cmd(request),
        Command::Coverage(rest) => coverage_cmd::coverage_cmd(&rest),
    }
}

fn parse_import_modelica(rest: &[String]) -> Option<(PathBuf, bool)> {
    let [sub, tail @ ..] = rest else {
        return None;
    };
    if sub != "modelica" {
        return None;
    }
    let mut path = None;
    let mut json = false;
    for arg in tail {
        match arg.as_str() {
            "--json" => json = true,
            other if other.starts_with('-') && other != "-" => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
    }
    Some((path?, json))
}

fn parse_show_named(rest: &[String]) -> Option<(String, PathBuf)> {
    if rest.first().map(String::as_str) != Some("show") {
        return None;
    }
    let tail = &rest[1..];
    let id = tail.iter().find(|arg| !arg.starts_with('-'))?.clone();
    let (_, dir, _) = parse_genesis_args(tail)?;
    Some((id, dir?))
}

fn parse_provider_request(rest: &[String]) -> Option<ProviderRequest> {
    let json = catalog::wants_json(rest);
    let mut sub = None;
    let mut id = None;
    for arg in rest {
        match arg.as_str() {
            "--json" => {}
            other if other.starts_with('-') && other != "-" => return None,
            other if sub.is_none() => sub = Some(other),
            other if id.is_none() => id = Some(other.to_string()),
            _ => return None,
        }
    }
    match sub {
        Some("list") if id.is_none() => Some(ProviderRequest::List { json }),
        Some("inspect") => Some(ProviderRequest::Inspect { id: id? }),
        Some("test") => Some(ProviderRequest::Test { id: id?, json }),
        _ => None,
    }
}

fn parse_fork_request(rest: &[String]) -> Option<ForkRequest> {
    let json = catalog::wants_json(rest);
    let dry_run = rest.iter().any(|arg| arg == "--dry-run");
    let mut sub = None;
    for arg in rest {
        match arg.as_str() {
            "--json" | "--dry-run" => {}
            other if other.starts_with('-') && other != "-" => return None,
            other if sub.is_none() => sub = Some(other),
            _ => return None,
        }
    }
    match sub {
        Some("status") => Some(ForkRequest::Status { json }),
        Some("sync") => Some(ForkRequest::Sync { dry_run, json }),
        _ => None,
    }
}

fn parse_new_request(args: &[String]) -> Option<(String, PathBuf)> {
    let mut name = None;
    let mut out = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" => {
                assign_once(
                    &mut out,
                    PathBuf::from(take_nonflag_value(args, &mut index)?),
                )?;
            }
            other if other.starts_with('-') && other != "-" => return None,
            other => assign_once(&mut name, other.to_string())?,
        }
        index += 1;
    }
    let name = name?;
    let out = out.unwrap_or_else(|| PathBuf::from(&name));
    Some((name, out))
}

fn parse_path_out_request(args: &[String]) -> Option<(PathBuf, PathBuf)> {
    let mut path = None;
    let mut out = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" => {
                assign_once(
                    &mut out,
                    PathBuf::from(take_nonflag_value(args, &mut index)?),
                )?;
            }
            other if other.starts_with('-') && other != "-" => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
        index += 1;
    }
    let path = path?;
    let out = out.unwrap_or_else(|| PathBuf::from("target/emath"));
    Some((path, out))
}

fn parse_required_path(args: &[String]) -> Option<PathBuf> {
    let mut path = None;
    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        assign_once(&mut path, PathBuf::from(arg))?;
    }
    path
}

fn parse_inspect_request(args: &[String]) -> Option<(PathBuf, bool)> {
    Some((parse_required_path(args)?, catalog::wants_json(args)))
}

fn parse_diff_request(args: &[String]) -> Option<(PathBuf, PathBuf, bool)> {
    let mut positionals = args.iter().filter(|arg| !arg.starts_with('-'));
    let a = PathBuf::from(positionals.next()?);
    let b = PathBuf::from(positionals.next()?);
    if positionals.next().is_some() {
        return None;
    }
    Some((a, b, catalog::wants_json(args)))
}

fn parse_vendor_request(args: &[String]) -> Option<PathBuf> {
    let mut out = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" => {
                assign_once(
                    &mut out,
                    PathBuf::from(take_nonflag_value(args, &mut index)?),
                )?;
            }
            other if other.starts_with('-') && other != "-" => return None,
            _ => return None,
        }
        index += 1;
    }
    out
}

fn parse_explain_request(args: &[String]) -> Option<ExplainRequest> {
    let mut path = None;
    let mut symbol = None;
    let mut json = false;
    let mut provenance = false;
    let mut show_defaults = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--provenance" => provenance = true,
            "--show-defaults" => show_defaults = true,
            other if other.starts_with('-') && other != "-" => return None,
            other if path.is_none() => path = Some(other.to_string()),
            other if symbol.is_none() => symbol = Some(other.to_string()),
            _ => return None,
        }
    }
    let path = path?;
    if path.starts_with("E-LAW-") || path == crate::diagnostics::E_LAW_001 {
        Some(ExplainRequest::Law { json })
    } else {
        Some(ExplainRequest::File {
            path: PathBuf::from(path),
            symbol,
            provenance,
            json,
            show_defaults,
        })
    }
}

fn parse_agent_request(args: &[String]) -> Option<AgentRequest> {
    let sub = args.first()?;
    let mut path = None;
    let mut out = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" => {
                if sub.as_str() != "build" {
                    return None;
                }
                assign_once(
                    &mut out,
                    PathBuf::from(take_nonflag_value(args, &mut index)?),
                )?;
            }
            other if other.starts_with('-') && other != "-" => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
        index += 1;
    }
    let path = path?;
    match sub.as_str() {
        "check" => Some(AgentRequest::Check { path }),
        "plan" => Some(AgentRequest::Plan { path }),
        "build" => {
            let out = out.unwrap_or_else(|| PathBuf::from("target/emath"));
            Some(AgentRequest::Build { path, out })
        }
        "triage" => Some(AgentRequest::Triage { path }),
        "propose" => Some(AgentRequest::Propose { path }),
        _ => None,
    }
}

fn catalog_read_cmd(command: &str, args: &[String], emit: impl FnOnce() -> CliExit) -> CliExit {
    if catalog::wants_help(args) {
        return print_command_help(command);
    }
    if let Some(code) = catalog::reject_unknown_flags(command, args) {
        return code;
    }
    if !no_extra_positionals(args) {
        return usage(command);
    }
    emit()
}

fn robot_docs_cmd() -> CliExit {
    print!("{}", catalog::robot_docs_guide());
    EXIT_OK
}

fn help_cmd(args: &[String]) -> CliExit {
    match args {
        [] => {
            print!("{}", help_text());
            EXIT_OK
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print!("{}", help_text());
            EXIT_OK
        }
        [command] => print_command_help(command),
        _ => usage("help [<command>]"),
    }
}

fn print_command_help(command: &str) -> CliExit {
    match catalog::command_help_text(command) {
        Some(text) => {
            print!("{text}");
            EXIT_OK
        }
        None => unknown_command(command),
    }
}

fn unknown_command(other: &str) -> CliExit {
    eprintln!("error: unknown command `{other}`");
    if let Some(hint) = catalog::suggest_command(other) {
        eprintln!("did you mean `emath {hint}`?");
        eprintln!("try: emath help {hint}");
    } else {
        eprintln!("try: emath help");
    }
    EXIT_USAGE
}

fn next_arg<'a>(args: &'a [String], index: &mut usize) -> Option<&'a str> {
    *index += 1;
    args.get(*index).map(String::as_str)
}

fn take_nonflag_value<'a>(args: &'a [String], index: &mut usize) -> Option<&'a str> {
    let value = next_arg(args, index)?;
    if value.starts_with("--") || matches!(value, "-o" | "-h" | "-V") {
        None
    } else {
        Some(value)
    }
}

fn assign_once<T>(slot: &mut Option<T>, value: T) -> Option<()> {
    if slot.is_some() {
        None
    } else {
        *slot = Some(value);
        Some(())
    }
}

fn no_extra_positionals(args: &[String]) -> bool {
    args.iter().all(|arg| arg.starts_with('-') && arg != "-")
}

pub enum CompileRequest {
    Ready {
        path: PathBuf,
        out: PathBuf,
        worlds: Vec<String>,
    },
}

fn parse_compile_request(args: &[String]) -> Option<CompileRequest> {
    let parametric = args.iter().any(|arg| arg == "--parametric");
    let (path, out, worlds) = parse_genesis_args(args)?;
    match (path, out, parametric) {
        (Some(path), Some(out), true) => Some(CompileRequest::Ready { path, out, worlds }),
        _ => None,
    }
}

enum FileJsonRequest {
    Ready {
        path: PathBuf,
        json: bool,
        /// `check --verify-data` (04 §5.2): re-hash declared sha256
        /// provenance files and refuse drift as `E-OBS-HASH`.
        verify_data: bool,
    },
}

fn parse_file_json_request(args: &[String]) -> Option<FileJsonRequest> {
    parse_file_request_inner(args, false)
}

/// `check` additionally admits `--verify-data` (04 §5.2); plan/expand/
/// assumptions reject it through the catalog flag whitelist.
fn parse_check_request(args: &[String]) -> Option<FileJsonRequest> {
    parse_file_request_inner(args, true)
}

fn parse_file_request_inner(args: &[String], wants_verify_data: bool) -> Option<FileJsonRequest> {
    let mut path = None;
    let mut json = false;
    let mut verify_data = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--verify-data" if wants_verify_data => verify_data = true,
            other if other.starts_with('-') && other != "-" => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
    }
    Some(FileJsonRequest::Ready {
        path: path?,
        json,
        verify_data,
    })
}

enum ParseRequest {
    Ready {
        path: PathBuf,
        out: Option<PathBuf>,
        forest_only: bool,
    },
}

fn parse_parse_request(args: &[String]) -> Option<ParseRequest> {
    let (path, out, _) = parse_genesis_args(args)?;
    let forest_only = args.iter().any(|arg| arg == "--forest");
    Some(ParseRequest::Ready {
        path: path?,
        out,
        forest_only,
    })
}

enum SignatureRequest {
    Ready { path: PathBuf, out: Option<PathBuf> },
}

fn parse_signature_request(args: &[String]) -> Option<SignatureRequest> {
    let (path, out, _) = parse_genesis_args(args)?;
    Some(SignatureRequest::Ready { path: path?, out })
}

enum GenesisRequest {
    Ready { path: PathBuf, out: PathBuf },
}

fn parse_genesis_request(args: &[String]) -> Option<GenesisRequest> {
    let (path, out, _) = parse_genesis_args(args)?;
    Some(GenesisRequest::Ready {
        path: path?,
        out: out?,
    })
}

/// Shared arg scan for genesis commands: positional file, `--out`, `--world`.
fn parse_genesis_args(args: &[String]) -> Option<(Option<PathBuf>, Option<PathBuf>, Vec<String>)> {
    let mut path = None;
    let mut out = None;
    let mut worlds = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" | "--dir" => {
                assign_once(
                    &mut out,
                    PathBuf::from(take_nonflag_value(args, &mut index)?),
                )?;
            }
            "--world" => {
                worlds.push(take_nonflag_value(args, &mut index)?.to_owned());
            }
            "--parametric" | "--forest" | "--json" => {}
            other if other.starts_with('-') => return None,
            other => assign_once(&mut path, PathBuf::from(other))?,
        }
        index += 1;
    }
    Some((path, out, worlds))
}

pub(crate) fn usage(message: &str) -> CliExit {
    eprintln!("error: missing or invalid arguments for this command");
    eprintln!("usage: emath {message}");
    let command = message.split_whitespace().next().unwrap_or("help");
    eprintln!("try: emath help {command}");
    EXIT_USAGE
}

/// JSON pretty-helper used by tests.
#[allow(dead_code)]
pub(crate) fn write_json(out: &mut impl Write, fields: &[(&str, String)]) -> std::io::Result<()> {
    let mut object = emath_artifact::JsonWriter::object();
    for (name, value) in fields {
        object.string(name, value);
    }
    write!(out, "{}", object.finish())
}

pub mod lsp;

pub mod layout;

pub mod agent_protocol;

pub mod portfolio;
