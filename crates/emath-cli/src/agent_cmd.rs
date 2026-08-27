//! Structured `emath agent` envelope over the real check/plan/build paths.

use std::path::{Path, PathBuf};

use emath_agent_protocol::{
    AgentProposal, ChallengeLoop, ChallengeOutcome, CheckerSuite, ProposalKind,
};
use emath_artifact::JsonWriter;
use emath_build::{BuildOptions, build_file};
use emath_portfolio::InterpretationPortfolio;
use emath_sema::session::CompilerSession;
use emath_tuning::{ExecutionDelta, SemanticChange, SemanticVariableKind, WorldDelta};
use emath_world_ir::WorldId;
use emath_world_ir::translation::EvidenceHandle;

use crate::tooling_cmd::{classify_build_error, doctor_probes, flag_value, positional_args};
use crate::{EXIT_OK, EXIT_REFUSED, EXIT_USAGE, run_check, usage};

/// Permissive loop: schema and capability admission decide; empty checkers.
const PROPOSE_LOOP: ChallengeLoop = ChallengeLoop {
    evidence_threshold: 0,
    max_estimated_cost: u64::MAX,
    checker_suite: CheckerSuite { checks: &[] },
    counterexample_generator: None,
};

/// `agent check|plan|build|triage|propose <file>`: same admission/plan/
/// build paths as the interactive commands; agents cannot bypass checks.
pub(crate) fn agent_cmd(args: &[String]) -> u8 {
    let Some(sub) = args.first() else {
        return usage("agent check|plan|build|triage|propose <file> [--out <dir>]");
    };
    let positional = positional_args(&args[1..]);
    let file = positional.first();
    match sub.as_str() {
        "propose" => agent_propose_cmd(file),
        "triage" => agent_triage_cmd(file),
        "check" => {
            let Some(file) = file else {
                return usage("agent check <file.emath>");
            };
            let (diagnostics, package_id) = run_check(Path::new(file));
            let admitted = !diagnostics.has_errors();
            let mut object = JsonWriter::object();
            object.string("schema", "emath.agent");
            object.string("command", "check");
            object.bool("admitted", admitted);
            object.string("package", &package_id);
            object.int("diagnostics", diagnostics.len() as u64);
            let mut lines = String::new();
            for item in diagnostics.items() {
                lines.push_str(item.code);
                lines.push_str(": ");
                lines.push_str(&item.message);
                lines.push_str("; ");
            }
            object.string("diagnostics_text", &lines);
            println!("{}", object.finish());
            if admitted { EXIT_OK } else { EXIT_REFUSED }
        }
        "plan" => {
            let Some(file) = file else {
                return usage("agent plan <file.emath>");
            };
            let mut session = CompilerSession::new(emath_core::limits::Limits::default());
            let Ok(package) = session.load_package(file) else {
                eprintln!("error: cannot read {file}");
                return EXIT_USAGE;
            };
            let result = session.plan(package.file);
            let mut object = JsonWriter::object();
            object.string("schema", "emath.agent");
            object.string("command", "plan");
            object.bool("admitted", !result.diagnostics.has_errors());
            object.int("goals", result.package.goals.len() as u64);
            object.int("plans", result.plans.len() as u64);
            let mut goals = String::new();
            for goal in &result.package.goals {
                goals.push_str(goal.kind.as_str());
                goals.push(' ');
                goals.push_str(goal.target.as_str());
                goals.push(' ');
            }
            object.string("goals", &goals);
            println!("{}", object.finish());
            if result.diagnostics.has_errors() {
                EXIT_REFUSED
            } else {
                EXIT_OK
            }
        }
        "build" => {
            let Some(file) = file else {
                return usage("agent build <file.emath> --out <dir>");
            };
            let out = flag_value("--out", args)
                .or_else(|| flag_value("-o", args))
                .unwrap_or_else(|| "target/emath".to_string());
            match build_file(
                file,
                PathBuf::from(out),
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
                Err(error) => {
                    eprintln!("error: {error}");
                    classify_build_error(&error)
                }
            }
        }
        _ => usage("agent check|plan|build|triage|propose <file> [--out <dir>]"),
    }
}

fn agent_propose_cmd(file: Option<&String>) -> u8 {
    let Some(file) = file else {
        return usage("agent propose <file>");
    };
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("error: cannot read {file}: {error}");
            return EXIT_USAGE;
        }
    };
    let proposal = match parse_proposal_text(&text) {
        Ok(proposal) => proposal,
        Err(error) => {
            eprintln!("error: {error}");
            return EXIT_USAGE;
        }
    };
    print_propose_outcome(
        &proposal,
        PROPOSE_LOOP.run(&proposal, &InterpretationPortfolio::default()),
    )
}

fn print_propose_outcome(proposal: &AgentProposal, outcome: ChallengeOutcome) -> u8 {
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
/// ignored, repeatable keys append. `change` =
/// `kind|symbol|description|provenance`, `obligation` = `id|scope|
/// provenance`, `exec` = `lowering|precision|provider|target|schedule`.
fn parse_proposal_text(text: &str) -> Result<AgentProposal, String> {
    let mut problem_id = String::new();
    let mut kind = ProposalKind::WorldDelta;
    let mut base_worlds = Vec::new();
    let mut holes = Vec::new();
    let mut changes = Vec::new();
    let mut obligations = Vec::new();
    let mut derivation = String::new();
    let mut required_providers = Vec::new();
    let mut estimated_cost = 0_u64;
    let mut evidence_units = 0_u32;
    let mut requested_authority = Vec::new();
    let mut agent_id = "cli".to_string();
    let mut execution_delta = None;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(format!(
                "line {}: expected key: value, got {line}",
                index + 1
            ));
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "problem" => problem_id = value.to_string(),
            "kind" => kind = parse_proposal_kind(value)?,
            "base" => base_worlds.extend(parse_world_ids(value)?),
            "holes" => holes.extend(split_csv(value)),
            "change" => changes.push(parse_change(value)?),
            "obligation" => obligations.push(parse_obligation(value)?),
            "derivation" => derivation = value.to_string(),
            "providers" => required_providers.extend(split_csv(value)),
            "cost" => {
                estimated_cost = value
                    .parse()
                    .map_err(|_| format!("line {}: cost must be a u64, got {value}", index + 1))?;
            }
            "evidence" => {
                evidence_units = value.parse().map_err(|_| {
                    format!("line {}: evidence must be a u32, got {value}", index + 1)
                })?;
            }
            "authority" => requested_authority.extend(split_csv(value)),
            "agent" => agent_id = value.to_string(),
            "exec" => execution_delta = Some(parse_exec(value)?),
            other => {
                return Err(format!("line {}: unknown key `{other}`", index + 1));
            }
        }
    }
    if requested_authority.is_empty() {
        requested_authority.push("propose".to_string());
    }
    let world_id = base_worlds.first().copied().unwrap_or(WorldId(0));
    Ok(AgentProposal::new(
        problem_id,
        kind,
        base_worlds,
        holes,
        WorldDelta::new(world_id, changes),
        execution_delta,
        obligations,
        derivation,
        required_providers,
        estimated_cost,
        evidence_units,
        requested_authority,
        agent_id,
    ))
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

fn agent_triage_cmd(file: Option<&String>) -> u8 {
    let Some(file) = file else {
        return usage("agent triage <file.emath>");
    };
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
    let (diagnostics, package_id) = run_check(Path::new(file));
    let admitted = !diagnostics.has_errors();
    let mut diag_rows = Vec::new();
    for item in diagnostics.items() {
        let mut row = JsonWriter::object();
        row.string("code", item.code);
        row.string("message", &item.message);
        diag_rows.push(row.finish());
    }
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let (goals, plans, plan_ok, plan_error) = match session.load_package(file) {
        Ok(package) => {
            let result = session.plan(package.file);
            (
                result.package.goals.len() as u64,
                result.plans.len() as u64,
                true,
                None,
            )
        }
        Err(error) => (0, 0, false, Some(error.to_string())),
    };
    let mut object = JsonWriter::object();
    object.string("schema", "emath.agent");
    object.string("command", "triage");
    object.string("file", file);
    object.bool("doctor_ok", doctor_ok);
    object.objects("doctor", &doctor_rows);
    object.bool("admitted", admitted);
    object.string("package", &package_id);
    object.objects("diagnostics", &diag_rows);
    object.bool("plan_ok", plan_ok);
    if let Some(message) = &plan_error {
        object.string("plan_error", message);
    }
    object.int("goals", goals);
    object.int("plans", plans);
    println!("{}", object.finish());
    if admitted && doctor_ok && plan_ok {
        EXIT_OK
    } else {
        EXIT_REFUSED
    }
}
