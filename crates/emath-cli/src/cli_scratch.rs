//! `emath expand` and the exactness/solve scratchpad commands.

use super::*;

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

pub(super) enum ExactnessRequest {
    Ready {
        path: PathBuf,
        json: bool,
        raise: Option<emath_syntax::ExactnessDimension>,
    },
}

pub(super) fn parse_exactness_request(args: &[String]) -> Option<ExactnessRequest> {
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

pub(super) fn exactness_cmd(request: ExactnessRequest) -> CliExit {
    let ExactnessRequest::Ready { path, json, raise } = request;
    let source = match read_emath_source("exactness", &path, json) {
        Ok(source) => source,
        Err(code) => return code,
    };
    // A freeze lock pins the meaning that was frozen; a raise would move a
    // dimension of that frozen meaning after the fact. Propose-only
    // display without `--raise` stays allowed: the budget itself is a
    // view, not an authority change.
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

pub(super) fn hole_json(hole: &emath_syntax::HoleRecord) -> String {
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

pub(super) fn solve_candidate_json(world: emath_syntax::SolveWorld, selected: bool) -> String {
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

pub(super) enum SolveRequest {
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

pub(super) enum ParsedSolve {
    Request(SolveRequest),
    Usage,
    UnknownLabel(String),
}

pub(super) fn parse_solve_request(args: &[String]) -> ParsedSolve {
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
pub(super) fn solve_check_cmd(request: SolveRequest) -> CliExit {
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
