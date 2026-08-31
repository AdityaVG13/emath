//! Structured `emath agent` envelope over the real check/plan/build paths.

use std::path::Path;

use emath_agent_protocol::{
    AgentProposal, ChallengeLoop, ChallengeOutcome, CheckerSuite, ProposalKind,
};
use emath_artifact::JsonWriter;
use emath_build::{build_file, BuildOptions};
use emath_portfolio::InterpretationPortfolio;
use emath_sema::session::CompilerSession;
use emath_tuning::{ExecutionDelta, SemanticChange, SemanticVariableKind, WorldDelta};
use emath_world_ir::{EvidenceHandle, WorldMorphism};
use emath_world_ir::WorldId;

use crate::tooling_cmd::{classify_build_error, doctor_probes};
use crate::{
    json_diagnostic_entry, json_diagnostics_entries, run_check, split_error_code, AgentRequest,
    CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE,
};

fn goal_json_rows(goals: &[emath_ir::Goal]) -> Vec<String> {
    goals
        .iter()
        .map(|goal| {
            let mut row = JsonWriter::object();
            row.string("kind", goal.kind.as_str());
            row.string("target", &goal.target);
            row.finish().trim_end().to_string()
        })
        .collect()
}

/// `emath.agent` plan envelope. Refusal includes `diagnostics[{code,severity,message}]`
/// so a checker can assert the E-* (same class as `agent check`).
fn agent_plan_envelope(
    admitted: bool,
    goals: &[emath_ir::Goal],
    plans: u64,
    diagnostics: &emath_core::Diagnostics,
) -> String {
    let mut object = JsonWriter::object();
    object.string("schema", "emath.agent");
    object.string("command", "plan");
    object.bool("admitted", admitted);
    object.int("plans", plans);
    object.objects("goals", &goal_json_rows(goals));
    object.objects("diagnostics", &json_diagnostics_entries(diagnostics));
    object.finish()
}

fn emit_agent_build_error(path: &Path, error: &dyn std::fmt::Display) -> CliExit {
    eprintln!("error: {error}");
    let (diagnostics, package_id, _) = run_check(path);
    let entries = if diagnostics.has_errors() {
        json_diagnostics_entries(&diagnostics)
    } else {
        let text = error.to_string();
        let (code, message) = split_error_code(&text).unwrap_or(("error", text.as_str()));
        vec![json_diagnostic_entry(code, "error", message)]
    };
    let mut object = JsonWriter::object();
    object.string("schema", "emath.agent");
    object.string("command", "build");
    object.bool("admitted", false);
    object.string("package", &package_id);
    object.objects("diagnostics", &entries);
    println!("{}", object.finish());
    classify_build_error(error)
}

fn emit_agent_propose_error(code: &str, detail: &str) -> CliExit {
    eprintln!("error: {detail}");
    let mut object = JsonWriter::object();
    object.string("schema", "emath.agent");
    object.string("command", "propose");
    object.bool("admitted", false);
    object.string("code", code);
    object.string("detail", detail);
    println!("{}", object.finish());
    EXIT_USAGE
}

/// Permissive loop: schema and capability admission decide; empty checkers.
const PROPOSE_LOOP: ChallengeLoop = ChallengeLoop {
    evidence_threshold: 0,
    max_estimated_cost: u64::MAX,
    checker_suite: CheckerSuite { checks: &[] },
    counterexample_generator: None,
};

/// `agent check|plan|build|triage|propose <file>`: same admission/plan/
/// build paths as the interactive commands; agents cannot bypass checks.
pub(crate) fn agent_cmd(request: AgentRequest) -> CliExit {
    match request {
        AgentRequest::Propose { path } => agent_propose_cmd(&path),
        AgentRequest::Triage { path } => agent_triage_cmd(&path),
        AgentRequest::Check { path } => {
            let (diagnostics, package_id, _) = run_check(&path);
            let admitted = !diagnostics.has_errors();
            println!(
                "{}",
                crate::agent_check_json_document(admitted, &package_id, &diagnostics)
            );
            if admitted {
                EXIT_OK
            } else {
                EXIT_REFUSED
            }
        }
        AgentRequest::Plan { path } => {
            let mut session = CompilerSession::new(emath_core::limits::Limits::default());
            let Ok(package) = session.load_package(&path) else {
                eprintln!("error: cannot read {}", path.display());
                let (diagnostics, _, _) = run_check(&path);
                println!("{}", agent_plan_envelope(false, &[], 0, &diagnostics));
                return EXIT_USAGE;
            };
            let result = session.plan(package.file);
            let admitted = !result.diagnostics.has_errors();
            println!(
                "{}",
                agent_plan_envelope(
                    admitted,
                    &result.package.goals,
                    result.plans.len() as u64,
                    &result.diagnostics,
                )
            );
            if admitted {
                EXIT_OK
            } else {
                EXIT_REFUSED
            }
        }
        AgentRequest::Build { path, out } => {
            match build_file(
                &path,
                out,
                BuildOptions {
                    verify_generated_crate: true,
                },
            ) {
                Ok(report) => {
                    let mut object = JsonWriter::object();
                    object.string("schema", "emath.agent");
                    object.string("command", "build");
                    object.string("artifact_id", &report.artifact_id.0);
                    object.string("package_id", &report.package_id.0);
                    object.string("crate", &report.crate_name);
                    object.string("artifact_dir", &report.artifact_dir.display().to_string());
                    println!("{}", object.finish());
                    EXIT_OK
                }
                Err(error) => emit_agent_build_error(&path, &error),
            }
        }
    }
}

fn agent_propose_cmd(file: &Path) -> CliExit {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) => {
            return emit_agent_propose_error(
                "E-PKG-080",
                &format!("cannot read {}: {error}", file.display()),
            );
        }
    };
    let proposal = match parse_proposal_text(&text) {
        Ok(proposal) => proposal,
        Err(error) => return emit_agent_propose_error("usage", &error),
    };
    print_propose_outcome(
        &proposal,
        PROPOSE_LOOP.run(&proposal, &InterpretationPortfolio::default()),
    )
}

fn print_propose_outcome(proposal: &AgentProposal, outcome: ChallengeOutcome) -> CliExit {
    let mut object = JsonWriter::object();
    object.string("schema", "emath.agent");
    object.string("command", "propose");
    object.string("identity", &format!("{:x}", proposal.identity));
    match outcome {
        ChallengeOutcome::Refused(refusal) => {
            object.bool("admitted", false);
            object.string("code", &refusal.code);
            object.string("detail", &refusal.detail);
            println!("{}", object.finish());
            EXIT_REFUSED
        }
        ChallengeOutcome::RevisionRequested(revision) => {
            object.bool("admitted", false);
            object.string("code", "revision");
            object.string("detail", &revision.feedback.canonical());
            println!("{}", object.finish());
            EXIT_REFUSED
        }
        ChallengeOutcome::WorldCandidate(candidate) => {
            object.bool("admitted", true);
            object.string("world", &candidate.world_id.0.to_string());
            object.int("rank", candidate.rank as u64);
            println!("{}", object.finish());
            EXIT_OK
        }
    }
}

/// Deterministic `key: value` envelope; `#` comments and blanks
/// ignored. Repeatable keys (`base`, `holes`, `change`, `obligation`,
/// `providers`, `authority`) append. Scalar keys (`problem`, `kind`,
/// `derivation`, `cost`, `evidence`, `agent`, `exec`) refuse duplicates.
/// `change` = `kind|symbol|description|provenance`, `obligation` =
/// `id|scope|provenance`, `exec` =
/// `lowering|precision|provider|target|schedule`.
fn parse_proposal_text(text: &str) -> Result<AgentProposal, String> {
    let mut problem_id = None;
    let mut kind = None;
    let mut base_worlds = Vec::new();
    let mut holes = Vec::new();
    let mut changes = Vec::new();
    let mut obligations = Vec::new();
    let mut derivation = None;
    let mut required_providers = Vec::new();
    let mut estimated_cost = None;
    let mut evidence_units = None;
    let mut requested_authority = Vec::new();
    let mut agent_id = None;
    let mut execution_delta = None;
    for (index, raw) in text.lines().enumerate() {
        let line_no = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(format!("line {line_no}: expected key: value, got {line}"));
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "problem" => assign_scalar(&mut problem_id, value.to_string(), key, line_no)?,
            "kind" => assign_scalar(&mut kind, parse_proposal_kind(value)?, key, line_no)?,
            "base" => base_worlds.extend(parse_world_ids(value)?),
            "holes" => holes.extend(split_csv(value)),
            "change" => changes.push(parse_change(value)?),
            "obligation" => obligations.push(parse_obligation(value)?),
            "derivation" => assign_scalar(&mut derivation, value.to_string(), key, line_no)?,
            "providers" => required_providers.extend(split_csv(value)),
            "cost" => {
                let parsed = value
                    .parse()
                    .map_err(|_| format!("line {line_no}: cost must be a u64, got {value}"))?;
                assign_scalar(&mut estimated_cost, parsed, key, line_no)?;
            }
            "evidence" => {
                let parsed = value
                    .parse()
                    .map_err(|_| format!("line {line_no}: evidence must be a u32, got {value}"))?;
                assign_scalar(&mut evidence_units, parsed, key, line_no)?;
            }
            "authority" => requested_authority.extend(split_csv(value)),
            "agent" => assign_scalar(&mut agent_id, value.to_string(), key, line_no)?,
            "exec" => assign_scalar(&mut execution_delta, parse_exec(value)?, key, line_no)?,
            other => {
                return Err(format!("line {line_no}: unknown key `{other}`"));
            }
        }
    }
    if requested_authority.is_empty() {
        requested_authority.push("propose".to_string());
    }
    let world_id = base_worlds.first().copied().unwrap_or(WorldId(0));
    Ok(AgentProposal::new(
        problem_id.unwrap_or_default(),
        kind.unwrap_or(ProposalKind::WorldDelta),
        base_worlds,
        holes,
        WorldDelta::new(world_id, changes),
        execution_delta,
        obligations,
        derivation.unwrap_or_default(),
        required_providers,
        estimated_cost.unwrap_or(0),
        evidence_units.unwrap_or(0),
        requested_authority,
        agent_id.unwrap_or_else(|| "cli".to_string()),
    ))
}

fn assign_scalar<T>(slot: &mut Option<T>, value: T, key: &str, line: usize) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("line {line}: duplicate `{key}`"));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_proposal_kind(name: &str) -> Result<ProposalKind, String> {
    Ok(match name {
        "parse-hypothesis" => ProposalKind::ParseHypothesis,
        "signature" => ProposalKind::Signature,
        "carrier" => ProposalKind::Carrier,
        "operator-meaning" => ProposalKind::OperatorMeaning,
        "law" => ProposalKind::Law,
        "constructor" => ProposalKind::Constructor,
        "world-delta" => ProposalKind::WorldDelta,
        "selection-policy" => ProposalKind::SelectionPolicy,
        "implementation-plan" => ProposalKind::ImplementationPlan,
        other => return Err(format!("unknown proposal kind `{other}`")),
    })
}

fn parse_world_ids(value: &str) -> Result<Vec<WorldId>, String> {
    let mut worlds = Vec::new();
    for item in split_csv(value) {
        let id = item
            .parse::<u64>()
            .map_err(|_| format!("base world id must be a u64, got {item}"))?;
        worlds.push(WorldId(id));
    }
    Ok(worlds)
}

fn parse_change(value: &str) -> Result<SemanticChange, String> {
    let parts = split_bar(value, 4, "change")?;
    let kind = SemanticVariableKind::from_canonical(&parts[0])
        .ok_or_else(|| format!("unknown change kind `{}`", parts[0]))?;
    let symbol = if parts[1].is_empty() {
        None
    } else {
        Some(emath_term::SymbolId(parts[1].clone()))
    };
    Ok(SemanticChange {
        kind,
        symbol,
        description: parts[2].clone(),
        provenance: parts[3].clone(),
    })
}

fn parse_obligation(value: &str) -> Result<EvidenceHandle, String> {
    let parts = split_bar(value, 3, "obligation")?;
    let id = parts[0]
        .parse::<u64>()
        .map_err(|_| format!("obligation id must be a u64, got {}", parts[0]))?;
    Ok(EvidenceHandle {
        id,
        scope: parts[1].clone(),
        provenance: parts[2].clone(),
    })
}

fn parse_exec(value: &str) -> Result<ExecutionDelta, String> {
    let parts = split_bar(value, 5, "exec")?;
    Ok(ExecutionDelta {
        lowering: parts[0].clone(),
        precision: parts[1].clone(),
        provider: parts[2].clone(),
        target: parts[3].clone(),
        schedule: parts[4].clone(),
    })
}

fn split_bar(value: &str, expected: usize, field: &str) -> Result<Vec<String>, String> {
    let parts: Vec<String> = value
        .split('|')
        .map(str::trim)
        .map(str::to_string)
        .collect();
    if parts.len() != expected {
        return Err(format!(
            "{field} expected {expected} `|`-separated fields, got {}",
            parts.len()
        ));
    }
    Ok(parts)
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn agent_triage_cmd(file: &Path) -> CliExit {
    let probes = doctor_probes();
    let doctor_ok = probes.iter().all(|probe| probe.ok);
    let mut doctor_rows = Vec::new();
    for probe in &probes {
        let mut row = JsonWriter::object();
        row.string("name", probe.name);
        row.bool("ok", probe.ok);
        if let Some(version) = &probe.version {
            row.string("version", version);
        }
        doctor_rows.push(row.finish());
    }
    let (diagnostics, package_id, _) = run_check(Path::new(file));
    let admitted = !diagnostics.has_errors();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let (goals, plans, plan_ok, plan_error) = match session.load_package(file) {
        Ok(package) => {
            let result = session.plan(package.file);
            (result.package.goals, result.plans.len() as u64, true, None)
        }
        Err(error) => (Vec::new(), 0, false, Some(error.to_string())),
    };
    println!(
        "{}",
        crate::agent_triage_json_document(
            &file.display().to_string(),
            doctor_ok,
            &doctor_rows,
            admitted,
            &package_id,
            &diagnostics,
            plan_ok,
            plan_error.as_deref(),
            &goals,
            plans,
        )
    );
    if admitted && doctor_ok && plan_ok {
        EXIT_OK
    } else {
        EXIT_REFUSED
    }
}
