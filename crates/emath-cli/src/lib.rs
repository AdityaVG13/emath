//! emath CLI: `check`, `plan`, `build`, `artifact`, `architecture`, `web`, `serve`,
//! and the Semantic Genesis commands (`parse`, `expand`, `signature`, `genesis`, `eval`,
//! `repl`, `compile --parametric`, `world show`, `portfolio show`, `meaning`).
//! Exit codes: 0 success, 1 refusal/diagnostic, 2 usage or io error.

#![forbid(unsafe_code)]

mod agent_cmd;
pub mod catalog;
mod eval_cmd;
pub mod genesis_cmd;
mod meaning_cmd;
mod provenance_cmd;
mod serve_cmd;
mod simulate_cmd;
mod tooling_cmd;

use emath_build::{BuildOptions, build_file};
use emath_core::Diagnostics;
use emath_plan::{
    PlanInspection, PlannerConfig, PlanningOutcome, emit_provider_trait, lift_missing,
    plan as run_planner,
};
use emath_provider_api::{ProviderRegistry, RegistryConfig};
use emath_sema::session::CompilerSession;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const EXIT_OK: u8 = 0;
pub const EXIT_REFUSED: u8 = 1;
pub const EXIT_USAGE: u8 = 2;

pub use provenance_cmd::provenance_explanation;

pub fn print_diagnostics(diagnostics: &Diagnostics) {
    for item in diagnostics.items() {
        println!(
            "{} {} ({}:{})",
            item.code, item.message, item.primary.file.0, item.primary.start
        );
        if let Some(help) = &item.help {
            for line in help.lines() {
                println!("  {line}");
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

pub(crate) fn json_diagnostic_entry(code: &str, severity: &str, message: &str) -> String {
    let mut entry = emath_artifact::JsonWriter::object();
    entry.string("code", code);
    entry.string("severity", severity);
    entry.string("message", message);
    entry.finish().trim_end().to_string()
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
            if let Some(help) = &item.help {
                entry.string("help", help);
            }
            if let Some(pedagogy) = &item.pedagogy {
                entry.string("understood", &pedagogy.understood);
                entry.string("unknown", &pedagogy.unknown);
                entry.string("why", &pedagogy.why);
                entry.string("smallest_repair", &pedagogy.smallest_repair);
                if !pedagogy.alternatives.is_empty() {
                    entry.strings("alternatives", &pedagogy.alternatives);
                }
                if let Some(example) = &pedagogy.example {
                    entry.string("example", example);
                }
                if let Some(deeper) = &pedagogy.deeper_concept {
                    entry.string("deeper_concept", deeper);
                }
                if let Some(authority) = &pedagogy.authority_consequence {
                    entry.string("authority_consequence", authority);
                }
                if let Some(link) = &pedagogy.library_link {
                    entry.string("library_link", link);
                }
            }
            entry.finish().trim_end().to_string()
        })
        .collect()
}

pub(crate) fn print_json_diagnostics(command: &str, admitted: bool, entries: &[String]) {
    let mut out = emath_artifact::JsonWriter::object();
    out.string("command", command);
    out.bool("admitted", admitted);
    out.objects("diagnostics", entries);
    println!("{}", out.finish());
}

/// `check <file> [--json]`: parse + admit, no codegen.
pub fn check(path: &Path, json: bool) -> u8 {
    if let Some(code) = meaning_cmd::refuse_malformed_project_lock(path) {
        return code;
    }
    let path = path.to_path_buf();
    let (diagnostics, package_id) = run_check(&path);
    let meaning_id = if diagnostics.has_errors() {
        None
    } else {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|source| admitted_meaning_id(&path, &source))
    };
    print_diagnostics(&diagnostics);
    if json {
        // The diagnostics array carries codes and messages, not counts:
        // a checker lane must be able to assert the exact E-* code the
        // CLI refused with.
        let body = json_diagnostics_entries(&diagnostics);
        let mut out = emath_artifact::JsonWriter::object();
        out.string("command", "check");
        out.bool("admitted", !diagnostics.has_errors());
        out.objects("diagnostics", &body);
        out.string("package", &package_id);
        if let Some(meaning_id) = &meaning_id {
            out.string("meaning_id", meaning_id.as_str());
        }
        println!("{}", out.finish());
    }
    if diagnostics.has_errors() {
        EXIT_REFUSED
    } else {
        EXIT_OK
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

/// `expand <file> [--json]`: print the contracted form of L0/L1/L2 shorthand.
pub fn expand_cmd(path: &Path, json: bool) -> u8 {
    let Ok(source) = std::fs::read_to_string(path) else {
        eprintln!("error: cannot read {}", path.display());
        return EXIT_USAGE;
    };
    let expansion = emath_syntax::expand_scratch(&source);
    print_diagnostics(&expansion.diagnostics);
    let meaning_id = admitted_meaning_id(path, &expansion.expanded);
    if json {
        let mut notes = Vec::new();
        for note in &expansion.notes {
            let mut entry = emath_artifact::JsonWriter::object();
            entry.string("inferred", &note.inferred);
            entry.string("rationale", &note.rationale);
            entry.string("replacement", &note.replacement);
            entry.string("stability", note.stability);
            notes.push(entry.finish().trim_end().to_string());
        }
        let mut out = emath_artifact::JsonWriter::object();
        out.string("command", "expand");
        out.bool("rewritten", expansion.rewritten);
        out.string("level", expansion.level.as_str());
        out.bool("ok", !expansion.diagnostics.has_errors());
        out.string("source", &expansion.expanded);
        if let Some(meaning_id) = &meaning_id {
            out.string("meaning_id", meaning_id.as_str());
        }
        out.objects("notes", &notes);
        let mut holes = Vec::new();
        for hole in &expansion.holes {
            holes.push(hole_json(hole));
        }
        out.objects("holes", &holes);
        let mut solve_candidates = Vec::new();
        for candidate in &expansion.solve_candidates {
            solve_candidates.push(solve_candidate_json(candidate));
        }
        out.objects("solve_candidates", &solve_candidates);
        out.objects(
            "diagnostics",
            &json_diagnostics_entries(&expansion.diagnostics),
        );
        println!("{}", out.finish());
    } else {
        println!(
            "# emath expand: level={} rewritten={}",
            expansion.level.as_str(),
            expansion.rewritten
        );
        if let Some(meaning_id) = &meaning_id {
            println!("# meaning_id: {meaning_id}");
        }
        for note in &expansion.notes {
            println!(
                "# inferred: {} ({}) — {}",
                note.inferred, note.stability, note.rationale
            );
            println!("# write instead: {}", note.replacement.replace('\n', " / "));
        }
        for candidate in &expansion.solve_candidates {
            println!(
                "# solve candidate: {} type={} domain={} exactness={} method={} evidence={} default={} selected={} holes={}",
                candidate.label,
                candidate.result_type,
                candidate.domain,
                candidate.exactness,
                candidate.method,
                candidate.evidence_class,
                candidate.beginner_default,
                candidate.selected,
                candidate.holes.join(",")
            );
        }
        for hole in &expansion.holes {
            println!("# {}", hole.summary());
            for candidate in &hole.candidates {
                println!(
                    "# candidate: {} ({}) {}",
                    candidate.label, candidate.kind, candidate.status
                );
            }
            for rejection in &hole.rejections {
                println!("# rejected: {} — {}", rejection.attempt, rejection.reason);
            }
        }
        print!("{}", expansion.expanded);
        if !expansion.expanded.ends_with('\n') {
            println!();
        }
    }
    if expansion.diagnostics.has_errors() {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
}

fn exactness_cmd(args: &[String]) -> u8 {
    let mut path = None;
    let mut json = false;
    let mut raise = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--raise" => {
                index += 1;
                if index < args.len() {
                    raise.push(args[index].as_str());
                }
            }
            other if other.starts_with('-') => {}
            other => path = Some(PathBuf::from(other)),
        }
        index += 1;
    }
    let Some(path) = path else {
        return usage("exactness <file.emath> [--json] [--raise units]");
    };
    let Ok(source) = std::fs::read_to_string(&path) else {
        eprintln!("error: cannot read {}", path.display());
        return EXIT_USAGE;
    };
    let raise_refs: Vec<&str> = raise.clone();
    let ledger = emath_syntax::exactness_ledger_raised(&source, &raise_refs);
    let expanded = emath_syntax::expand_scratch(&source);
    let meaning_id = admitted_meaning_id(&path, &expanded.expanded);
    if json {
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
        if let Some(meaning_id) = &meaning_id {
            out.string("meaning_id", meaning_id.as_str());
        }
        out.objects("entries", &rows);
        println!("{}", out.finish());
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
            format!(
                "{}:{}:{}",
                candidate.status, candidate.kind, candidate.label
            )
        })
        .collect();
    object.strings("candidates", &candidates);
    let rejections: Vec<String> = hole
        .rejections
        .iter()
        .map(|rejection| format!("{} — {}", rejection.attempt, rejection.reason))
        .collect();
    object.strings("rejections", &rejections);
    object.finish().trim_end().to_string()
}

fn solve_candidate_json(candidate: &emath_syntax::SolveCandidate) -> String {
    let mut object = emath_artifact::JsonWriter::object();
    object.string("label", &candidate.label);
    object.string("result_type", &candidate.result_type);
    object.string("domain", &candidate.domain);
    object.string("exactness", &candidate.exactness);
    object.string("method", &candidate.method);
    object.string("evidence_class", &candidate.evidence_class);
    object.strings("holes", &candidate.holes);
    object.bool("beginner_default", candidate.beginner_default);
    object.bool("selected", candidate.selected);
    object.finish().trim_end().to_string()
}

/// `solve --check <file>`: print labeled completions; never a naked float.
fn solve_check_cmd(args: &[String]) -> u8 {
    let json = catalog::wants_json(args);
    let check = args.iter().any(|arg| arg == "--check");
    let apply = tooling_cmd::flag_value("--apply", args);
    let mut path = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--apply" => index += 2,
            flag if flag.starts_with('-') => index += 1,
            other => {
                path = Some(other.to_string());
                index += 1;
            }
        }
    }
    let Some(path) = path else {
        return usage("solve --check <file.emath> [--json] [--apply <label>]");
    };
    let Ok(source) = std::fs::read_to_string(&path) else {
        eprintln!("error: cannot read {path}");
        return EXIT_USAGE;
    };
    if let Some(label) = apply {
        return match emath_syntax::apply_solve_candidate(&source, &label) {
            Ok((rewritten, delta)) => {
                if json {
                    let mut out = emath_artifact::JsonWriter::object();
                    out.string("command", "solve");
                    out.string("apply", &label);
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
        };
    }
    if !check {
        return usage("solve --check <file.emath> [--json] [--apply <label>]");
    }
    let expansion = emath_syntax::expand_scratch(&source);
    print_diagnostics(&expansion.diagnostics);
    if expansion.solve_candidates.is_empty() {
        eprintln!("error: no `solve` intent in {path}");
        return EXIT_REFUSED;
    }
    if json {
        let mut candidates = Vec::new();
        for candidate in &expansion.solve_candidates {
            candidates.push(solve_candidate_json(candidate));
        }
        let mut out = emath_artifact::JsonWriter::object();
        out.string("command", "solve");
        out.bool("ok", !expansion.diagnostics.has_errors());
        out.objects("solve_candidates", &candidates);
        println!("{}", out.finish());
    } else {
        println!("solve candidates (none is a naked numeric root):");
        for candidate in &expansion.solve_candidates {
            let mark = if candidate.selected {
                "*"
            } else if candidate.beginner_default {
                "default"
            } else {
                ""
            };
            println!(
                "  {} {} type={} domain={} exactness={} method={} evidence={} holes=[{}] {mark}",
                candidate.label,
                candidate.result_type,
                candidate.result_type,
                candidate.domain,
                candidate.exactness,
                candidate.method,
                candidate.evidence_class,
                candidate.holes.join(",")
            );
        }
    }
    if expansion.diagnostics.has_errors() {
        EXIT_REFUSED
    } else {
        EXIT_OK
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
            format!(
                "{} {} {} {}",
                entry.inference_id,
                entry.dimension.as_str(),
                entry.status.as_str(),
                entry.name
            )
        })
        .collect();
    object.strings("ledger", &rows);
    object.finish()
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

fn freeze_cmd(args: &[String]) -> u8 {
    let mut path = None;
    let mut out = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--out" | "-o" => {
                index += 1;
                if index < args.len() {
                    out = Some(PathBuf::from(&args[index]));
                }
            }
            other if other.starts_with('-') => {}
            other => path = Some(PathBuf::from(other)),
        }
        index += 1;
    }
    let Some(path) = path else {
        return usage("freeze <file.emath> [--out <file>] [--json]");
    };
    let Ok(source) = std::fs::read_to_string(&path) else {
        eprintln!("error: cannot read {}", path.display());
        return EXIT_USAGE;
    };
    if emath_syntax::exactness::claims_exactness_with_open_holes(&source) {
        eprintln!(
            "E-SYN-147 claiming exactness while holes remain open is refused; freeze does not upgrade authority"
        );
        return EXIT_REFUSED;
    }
    let expansion = emath_syntax::expand_scratch(&source);
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
        if std::fs::write(out, &frozen).is_err() {
            eprintln!("error: cannot write {}", out.display());
            return EXIT_USAGE;
        }
        let lock_path = sidecar_lock_path(out);
        if std::fs::write(&lock_path, &lock).is_err() {
            eprintln!("error: cannot write {}", lock_path.display());
            return EXIT_USAGE;
        }
    }
    if json {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("command", "freeze");
        object.string("schema", "emath.freeze.lock.v1");
        object.bool("ok", !expansion.diagnostics.has_errors());
        object.bool("authority_raised", false);
        object.int(
            "open_holes",
            ledger.count(emath_syntax::ExactnessStatus::Open) as u64,
        );
        object.string("source", &frozen);
        object.string("lock", &lock);
        println!("{}", object.finish());
    } else if out.is_none() {
        print!("{frozen}");
        if !frozen.ends_with('\n') {
            println!();
        }
        println!("--- emath.freeze.lock.v1 ---");
        print!("{lock}");
        if !lock.ends_with('\n') {
            println!();
        }
    }
    if expansion.diagnostics.has_errors() {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
}

fn why_cmd(args: &[String]) -> u8 {
    let mut path = None;
    let mut json = false;
    let mut needle = None;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            other if other.starts_with("inference:") => needle = Some(other.to_string()),
            other if other.starts_with('-') => {}
            other => path = Some(PathBuf::from(other)),
        }
    }
    let Some(path) = path else {
        return usage("why <file.emath> inference:N [--json]");
    };
    let Some(needle) = needle else {
        return usage("why <file.emath> inference:N [--json]");
    };
    let Ok(source) = std::fs::read_to_string(&path) else {
        eprintln!("error: cannot read {}", path.display());
        return EXIT_USAGE;
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
        object.string("stability", note.stability);
        println!("{}", object.finish());
    } else {
        println!("{} ({})", note.inferred, note.stability);
        println!("{}", note.rationale);
        println!("write: {}", note.replacement.replace('\n', " / "));
    }
}

fn assumptions_cmd(path: &Path, json: bool) -> u8 {
    let Ok(source) = std::fs::read_to_string(path) else {
        eprintln!("error: cannot read {}", path.display());
        return EXIT_USAGE;
    };
    let notes: Vec<_> = emath_syntax::explanation_notes(&source)
        .into_iter()
        .filter(|note| note.stability == "inferred")
        .collect();
    if json {
        let mut rows = Vec::new();
        for note in &notes {
            let mut object = emath_artifact::JsonWriter::object();
            object.string("inferred", &note.inferred);
            object.string("rationale", &note.rationale);
            object.string("stability", note.stability);
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

pub(crate) fn run_check(path: &Path) -> (Diagnostics, String) {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        let mut diagnostics = Diagnostics::new();
        diagnostics.error(
            "E-PKG-080",
            "cannot read source file",
            emath_core::Span::default(),
        );
        return (diagnostics, String::new());
    };
    let result = session.check(package.file);
    let package_id = result.package.content_id().0;
    (result.diagnostics, package_id)
}

/// `plan <file> [--json]`: check + goals + plans, no artifact.
pub fn plan(path: &PathBuf, json: bool) -> u8 {
    if let Some(code) = meaning_cmd::refuse_malformed_project_lock(path) {
        return code;
    }
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        eprintln!("error: cannot read {}", path.display());
        return EXIT_USAGE;
    };
    let result = session.plan(package.file);
    if !result.diagnostics.is_empty() {
        print_diagnostics(&result.diagnostics);
    }
    if json {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("command", "plan");
        object.bool("admitted", !result.diagnostics.has_errors());
        object.int("goals", result.package.goals.len() as u64);
        object.int("plans", result.plans.len() as u64);
        let mut goals = String::new();
        for goal in &result.package.goals {
            let entry = format!("{} {} ", goal.kind.as_str(), goal.target);
            goals.push_str(&entry);
        }
        object.string("goals", &goals);
        println!("{}", object.finish());
    }
    for plan in &result.plans {
        println!(
            "plan {} goal={} policy={} class={}",
            plan.plan_id.0,
            plan.goal.index(),
            plan.policy,
            plan.artifact_class
        );
    }
    if result.diagnostics.has_errors() {
        EXIT_REFUSED
    } else {
        EXIT_OK
    }
}

/// `build <file> [--out <dir>] [--verify] [--json]` (default out:
/// `target/emath` under the working directory).
pub fn build(spec: &PathBuf, out: &PathBuf, verify: bool, json: bool) -> u8 {
    if let Some(code) = meaning_cmd::refuse_malformed_project_lock(spec) {
        return code;
    }
    let options = BuildOptions {
        verify_generated_crate: verify,
    };
    match build_file(spec, out, options) {
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
            eprintln!("error: {error}");
            if error.to_string().contains("admission refused") {
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
pub fn explain_inspections(path: &Path) -> Result<Vec<PlanInspection>, u8> {
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

/// `planner <file.emath> [--json] [--parametric]`: run the deterministic
/// planner over the provider registry and print the machine inspection
/// (candidates, exclusions, selected plan, checks, disposition). With
/// `--parametric`, missing providers lift to a compilable Rust trait.
pub fn planner_cmd(path: &PathBuf, json: bool, parametric: bool) -> u8 {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let Ok(package) = session.load_package(path) else {
        eprintln!("error: cannot read {}", path.display());
        return EXIT_USAGE;
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
                if json {
                    println!("{}", inspection.to_json());
                }
                println!(
                    "plan goal={} disposition={} candidates={} root={} checks={}",
                    goal.target,
                    plan.artifact_class,
                    inspection.candidate_count(),
                    plan.root.index(),
                    inspection.checks.join(",")
                );
            }
            PlanningOutcome::NoEligible {
                reasons,
                disposition,
                inspection,
            } => {
                any_unplanned = true;
                if json {
                    println!("{}", inspection.to_json());
                }
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
            PlanningOutcome::Exhausted {
                continuation,
                disposition,
                inspection,
            } => {
                any_unplanned = true;
                if json {
                    println!("{}", inspection.to_json());
                }
                println!(
                    "exhausted goal={} class={} continuation={}",
                    goal.target,
                    disposition.name(),
                    continuation
                );
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
pub fn import_modelica_cmd(path: &Path, json: bool) -> u8 {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", path.display());
            return EXIT_USAGE;
        }
    };
    match emath_adapter_rumoca::import::import_modelica(&source) {
        Ok(declarations) => {
            if json {
                let mut object = emath_artifact::JsonWriter::object();
                object.string("command", "import modelica");
                object.int("declarations", declarations.len() as u64);
                let mut names = String::new();
                for declaration in &declarations {
                    let entry = format!("{} ", declaration.name);
                    names.push_str(&entry);
                }
                object.string("models", &names);
                println!("{}", object.finish());
            }
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
            EXIT_OK
        }
        Err(error) => {
            eprintln!("error: {} {}", error.code, error.message);
            EXIT_REFUSED
        }
    }
}

/// `artifact check <dir>`: independent verification of every published
/// artifact under `<dir>/emath/<artifact-id>` via `emath-checker`
/// (one identity, one checker; empty state dirs are refused).
pub fn artifact_check(dir: &Path) -> u8 {
    let artifact_root = dir.join("emath");
    if !artifact_root.is_dir() {
        eprintln!(
            "error: E-EVID-105: no `emath/` state directory under {}",
            dir.display()
        );
        return EXIT_USAGE;
    }
    let Ok(entries) = std::fs::read_dir(&artifact_root) else {
        eprintln!(
            "error: E-TLT-005: cannot list artifact state directory {}",
            artifact_root.display()
        );
        return EXIT_USAGE;
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
        return EXIT_REFUSED;
    }
    let mut ok = true;
    for id in artifact_ids {
        let root = artifact_root.join(&id);
        match emath_checker::check_artifact_dir(&root) {
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
    if ok { EXIT_OK } else { EXIT_REFUSED }
}

/// `artifact battery <dir>`: run the seeded negative-control battery over
/// every published artifact. Each seed must be refused with the code the
/// checker assigns; an escape is an admitted dishonest artifact and
/// refuses the command (CI-visible lane over real staged output).
pub fn artifact_battery(dir: &Path) -> u8 {
    let artifact_root = dir.join("emath");
    if !artifact_root.is_dir() {
        eprintln!(
            "error: E-EVID-105: no `emath/` state directory under {}",
            dir.display()
        );
        return EXIT_USAGE;
    }
    let Ok(entries) = std::fs::read_dir(&artifact_root) else {
        eprintln!(
            "error: E-TLT-005: cannot list artifact state directory {}",
            artifact_root.display()
        );
        return EXIT_USAGE;
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
        return EXIT_REFUSED;
    }
    let mut ok = true;
    for id in artifact_ids {
        let root = artifact_root.join(&id);
        match emath_checker::artifact_input_from_dir(&root) {
            Ok(input) => {
                let run = emath_checker::run_standard_battery(&input);
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
    if ok { EXIT_OK } else { EXIT_REFUSED }
}

/// `architecture [--json]`: provider-neutral pipeline description.
pub fn architecture(json: bool) -> u8 {
    let pipeline = ".emath -> SIR -> GIR -> resolution plan -> EMIR -> Rust artifact -> protected host promotion";
    let paths: Vec<String> = emath_artifact::required_artifact_paths()
        .iter()
        .map(ToString::to_string)
        .collect();
    if json {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.architecture");
        object.string("pipeline", pipeline);
        object.strings("required_paths", &paths);
        println!("{}", object.finish());
    } else {
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
pub fn run(args: &[String]) -> u8 {
    let Some(command) = args.first() else {
        print!("{}", help_text());
        return EXIT_OK;
    };
    match command.as_str() {
        "help" | "--help" | "-h" => return help_cmd(&args[1..]),
        "version" | "--version" | "-V" => {
            return catalog_read_cmd("version", &args[1..], || {
                println!("{}", catalog::version_text());
                EXIT_OK
            });
        }
        "capabilities" => {
            return catalog_read_cmd("capabilities", &args[1..], || {
                print!("{}", catalog::capabilities_json());
                EXIT_OK
            });
        }
        "robot-docs" => {
            return catalog_read_cmd("robot-docs", &args[1..], || robot_docs_cmd(&args[1..]));
        }
        _ => {}
    }
    if catalog::wants_help(&args[1..]) {
        return print_command_help(command);
    }
    if let Some(code) = catalog::reject_unknown_flags(command, &args[1..]) {
        return code;
    }
    match command.as_str() {
        "check" => {
            let (path, json) = parse_file_args(&args[1..]);
            match path {
                Some(path) => check(&path, json),
                None => usage("check <file.emath> [--json]"),
            }
        }
        "plan" => {
            let (path, json) = parse_file_args(&args[1..]);
            match path {
                Some(path) => plan(&path, json),
                None => usage("plan <file.emath> [--json]"),
            }
        }
        "planner" => {
            let mut path = None;
            let mut json = false;
            let mut parametric = false;
            for arg in &args[1..] {
                match arg.as_str() {
                    "--json" => json = true,
                    "--parametric" => parametric = true,
                    other if other.starts_with('-') => {}
                    other => path = Some(PathBuf::from(other)),
                }
            }
            match path {
                Some(path) => planner_cmd(&path, json, parametric),
                None => usage("planner <file.emath> [--json] [--parametric]"),
            }
        }
        "build" => {
            let mut path = None;
            let mut out = None;
            let mut verify = false;
            let mut json = false;
            let mut index = 1;
            while index < args.len() {
                match args[index].as_str() {
                    "--out" | "-o" => {
                        index += 1;
                        if index < args.len() {
                            out = Some(PathBuf::from(&args[index]));
                        }
                    }
                    "--verify" => verify = true,
                    "--json" => json = true,
                    other if other.starts_with('-') => {
                        return usage("build <file.emath> [--out <dir>] [--verify] [--json]");
                    }
                    other => path = Some(PathBuf::from(other)),
                }
                index += 1;
            }
            match path {
                Some(spec) => {
                    // One-command quick run: `emath build <file>` publishes
                    // under target/emath relative to the working directory.
                    let out = out.unwrap_or_else(|| PathBuf::from("target/emath"));
                    build(&spec, &out, verify, json)
                }
                None => usage("build <file.emath> [--out <dir>] [--verify] [--json]"),
            }
        }
        "parse" => {
            let (path, out, _) = parse_genesis_args(&args[1..]);
            let forest_only = args[1..].iter().any(|arg| arg == "--forest");
            match path {
                Some(path) => genesis_cmd::parse_cmd(&path, out.as_ref(), forest_only),
                None => usage("parse --forest <file.emath> [--out <dir>]"),
            }
        }
        "expand" => {
            let (path, json) = parse_file_args(&args[1..]);
            match path {
                Some(path) => expand_cmd(&path, json),
                None => usage("expand <file.emath> [--json]"),
            }
        }
        "solve" => solve_check_cmd(&args[1..]),
        "exactness" => exactness_cmd(&args[1..]),
        "freeze" => freeze_cmd(&args[1..]),
        "why" => why_cmd(&args[1..]),
        "assumptions" => {
            let (path, json) = parse_file_args(&args[1..]);
            match path {
                Some(path) => assumptions_cmd(&path, json),
                None => usage("assumptions <file.emath> [--json]"),
            }
        }
        "signature" => {
            let (path, out, _) = parse_genesis_args(&args[1..]);
            match path {
                Some(path) => genesis_cmd::signature_cmd(&path, out.as_ref()),
                None => usage("signature <file.emath> [--out <dir>]"),
            }
        }
        "genesis" => {
            let (path, out, _) = parse_genesis_args(&args[1..]);
            match (path, out) {
                (Some(path), Some(out)) => genesis_cmd::genesis_cmd(&path, &out),
                _ => usage("genesis <file.emath> --out <dir>"),
            }
        }
        "eval" => eval_cmd::dispatch_eval(&args[1..]),
        "simulate" => simulate_cmd::dispatch_simulate(&args[1..]),
        "repl" => eval_cmd::dispatch_repl(&args[1..]),
        "compile" => {
            let (path, out, worlds) = parse_genesis_args(&args[1..]);
            let parametric = args[1..].iter().any(|arg| arg == "--parametric");
            match (path, out, parametric) {
                (Some(path), Some(out), true) => genesis_cmd::compile_cmd(&path, &out, &worlds),
                _ => usage("compile --parametric <file.emath> --out <dir> [--world LABEL]"),
            }
        }
        "world" => {
            if args.get(1).is_some_and(|sub| sub == "show") && args.len() >= 3 {
                let (_, dir, _) = parse_genesis_args(&args[3..]);
                let id = &args[2];
                match dir {
                    Some(dir) => genesis_cmd::world_show_cmd(id, &dir),
                    None => usage("world show WORLD_ID --dir <dir>"),
                }
            } else {
                usage("world show WORLD_ID --dir <dir>")
            }
        }
        "portfolio" => {
            if args.get(1).is_some_and(|sub| sub == "show") && args.len() >= 3 {
                let (_, dir, _) = parse_genesis_args(&args[3..]);
                let id = &args[2];
                match dir {
                    Some(dir) => genesis_cmd::portfolio_show_cmd(id, &dir),
                    None => usage("portfolio show PORTFOLIO_ID --dir <dir>"),
                }
            } else {
                usage("portfolio show PORTFOLIO_ID --dir <dir>")
            }
        }
        "meaning" => meaning_cmd::dispatch(&args[1..]),
        "import" => {
            if args.get(1).is_some_and(|sub| sub == "modelica") && args.len() >= 3 {
                let json = args[2..].iter().any(|arg| arg == "--json");
                import_modelica_cmd(&PathBuf::from(&args[2]), json)
            } else {
                usage("import modelica <file.mo> [--json]")
            }
        }
        "artifact" => {
            if args.get(1).is_some_and(|c| c == "check") && args.len() >= 3 {
                artifact_check(&PathBuf::from(&args[2]))
            } else if args.get(1).is_some_and(|c| c == "battery") && args.len() >= 3 {
                artifact_battery(&PathBuf::from(&args[2]))
            } else {
                usage("artifact check|battery <dir>")
            }
        }
        "architecture" => architecture(catalog::wants_json(&args[1..])),
        "web" | "serve" => serve_cmd::web_cmd(&args[1..]),
        "new" | "fmt" | "explain" | "run" | "test" | "bench" | "verify" | "inspect" | "diff"
        | "doctor" | "vendor" | "provider" | "fork" | "agent" => {
            tooling_cmd::tooling_dispatch(command, &args[1..])
        }
        other => unknown_command(other),
    }
}

fn catalog_read_cmd(command: &str, args: &[String], emit: impl FnOnce() -> u8) -> u8 {
    if catalog::wants_help(args) {
        return print_command_help(command);
    }
    if let Some(code) = catalog::reject_unknown_flags(command, args) {
        return code;
    }
    emit()
}

fn robot_docs_cmd(args: &[String]) -> u8 {
    match args.first().map(String::as_str) {
        None | Some("guide" | "--guide") => {
            print!("{}", catalog::robot_docs_guide());
            EXIT_OK
        }
        Some(other) => {
            eprintln!("error: unknown robot-docs topic `{other}`");
            eprintln!("did you mean `emath robot-docs guide`?");
            eprintln!("try: emath help robot-docs");
            EXIT_USAGE
        }
    }
}

fn help_cmd(args: &[String]) -> u8 {
    match args.first().map(String::as_str) {
        None | Some("--help" | "-h") => {
            print!("{}", help_text());
            EXIT_OK
        }
        Some(command) => print_command_help(command),
    }
}

fn print_command_help(command: &str) -> u8 {
    match catalog::command_help_text(command) {
        Some(text) => {
            print!("{text}");
            EXIT_OK
        }
        None => unknown_command(command),
    }
}

fn unknown_command(other: &str) -> u8 {
    eprintln!("error: unknown command `{other}`");
    if let Some(hint) = catalog::suggest_command(other) {
        eprintln!("did you mean `emath {hint}`?");
        eprintln!("try: emath help {hint}");
    } else {
        eprintln!("try: emath help");
    }
    EXIT_USAGE
}

/// Shared arg scan for genesis commands: positional file, `--out`, `--world`.
fn parse_genesis_args(args: &[String]) -> (Option<PathBuf>, Option<PathBuf>, Vec<String>) {
    let mut path = None;
    let mut out = None;
    let mut worlds = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" | "--dir" => {
                index += 1;
                if index < args.len() {
                    out = Some(PathBuf::from(&args[index]));
                }
            }
            "--world" => {
                index += 1;
                if index < args.len() {
                    worlds.push(args[index].clone());
                }
            }
            "--parametric" | "--forest" | "--json" => {}
            other if other.starts_with('-') => {}
            other => path = Some(PathBuf::from(other)),
        }
        index += 1;
    }
    (path, out, worlds)
}

fn parse_file_args(args: &[String]) -> (Option<PathBuf>, bool) {
    let mut path = None;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            other if other.starts_with('-') && other != "-" => {}
            other => path = Some(PathBuf::from(other)),
        }
    }
    (path, json)
}

pub(crate) fn usage(message: &str) -> u8 {
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
