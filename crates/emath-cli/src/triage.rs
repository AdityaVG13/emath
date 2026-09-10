//! Mega-command for AI agents and developers (`emath triage` / `emath --robot-triage`).
//!
//! Provides single-call complete situational awareness:
//! - Workspace and target file orientation
//! - Toolchain presence and doctor health
//! - Semantic admission, diagnostics, and error codes
//! - Open mathematical goals and resolution plans
//! - Ranked next-action recommendations with exact copy-paste shell commands

use std::path::{Path, PathBuf};

use crate::tooling_cmd::doctor_probes;
use crate::{CliExit, EXIT_OK, EXIT_REFUSED, json_diagnostics_entries, run_check};
use emath_artifact::JsonWriter;
use emath_sema::CompilerSession;

pub struct TriageRecommendation {
    pub priority: usize,
    pub action: &'static str,
    pub command: String,
    pub reason: String,
}

pub fn triage_cmd(target: Option<PathBuf>, json: bool) -> CliExit {
    let probes = doctor_probes();
    let doctor_ok = probes.iter().all(|probe| probe.ok);

    let active_path = target.or_else(discover_target_file);

    let (admitted, package_id, diagnostics, goals, plans_count, plan_ok) = match &active_path {
        Some(path) if path.exists() => {
            let (diags, pkg_id, _) = run_check(path);
            let adm = !diags.has_errors();
            let mut session = CompilerSession::new(emath_core::limits::Limits::default());
            let (g, p, p_ok) = match session.load_package(path) {
                Ok(pkg) => {
                    let result = session.plan(pkg.file);
                    (result.package.goals, result.plans.len(), true)
                }
                Err(_) => (Vec::new(), 0, false),
            };
            (adm, Some(pkg_id), diags, g, p, p_ok)
        }
        _ => (false, None, emath_core::Diagnostics::new(), Vec::new(), 0, false),
    };

    let mut recommendations = Vec::new();
    let mut prio = 1;

    if !doctor_ok {
        let missing: Vec<&str> = probes.iter().filter(|p| !p.ok).map(|p| p.name).collect();
        recommendations.push(TriageRecommendation {
            priority: prio,
            action: "fix_toolchain",
            command: "emath doctor --json".to_string(),
            reason: format!("Toolchain environment missing: {}", missing.join(", ")),
        });
        prio += 1;
    }

    match &active_path {
        Some(path) => {
            let path_str = path.display().to_string();
            if !admitted {
                let first_error = diagnostics.errors().next().map(|d| &d.code[..]);
                if let Some(code) = first_error {
                    recommendations.push(TriageRecommendation {
                        priority: prio,
                        action: "explain_error",
                        command: format!("emath explain {code} --json"),
                        reason: format!("Investigate refusal diagnostic {code}"),
                    });
                    prio += 1;
                }
                recommendations.push(TriageRecommendation {
                    priority: prio,
                    action: "fix_diagnostics",
                    command: format!("emath check {path_str} --json"),
                    reason: "Resolve admission errors in model source".to_string(),
                });
            } else {
                recommendations.push(TriageRecommendation {
                    priority: prio,
                    action: "plan",
                    command: format!("emath plan {path_str} --json"),
                    reason: "Inspect deterministic mathematical resolution DAG".to_string(),
                });
                prio += 1;
                recommendations.push(TriageRecommendation {
                    priority: prio,
                    action: "build",
                    command: format!("emath build {path_str} --verify --json"),
                    reason: "Compile admitted model to verified Rust Cargo package".to_string(),
                });
                prio += 1;
                recommendations.push(TriageRecommendation {
                    priority: prio,
                    action: "simulate",
                    command: format!("emath simulate {path_str} --method rk45 --json"),
                    reason: "Run numerical solver integration over continuous model states".to_string(),
                });
            }
        }
        None => {
            recommendations.push(TriageRecommendation {
                priority: prio,
                action: "scaffold_model",
                command: "emath new my_model".to_string(),
                reason: "No .emath models found in current directory; create a new project scaffold".to_string(),
            });
            prio += 1;
            recommendations.push(TriageRecommendation {
                priority: prio,
                action: "capabilities",
                command: "emath capabilities --json".to_string(),
                reason: "Inspect all available mathematical features and CLI contracts".to_string(),
            });
        }
    }

    if json {
        print_triage_json(
            active_path.as_deref(),
            doctor_ok,
            &probes,
            admitted,
            package_id.as_deref(),
            &diagnostics,
            goals.len(),
            plans_count,
            plan_ok,
            &recommendations,
        );
    } else {
        print_triage_human(
            active_path.as_deref(),
            doctor_ok,
            &probes,
            admitted,
            goals.len(),
            plans_count,
            &recommendations,
        );
    }

    if doctor_ok && (active_path.is_none() || admitted) {
        EXIT_OK
    } else {
        EXIT_REFUSED
    }
}

fn discover_target_file() -> Option<PathBuf> {
    // Check current directory
    if let Ok(entries) = std::fs::read_dir(".") {
        let mut emath_files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |ext| ext == "emath"))
            .collect();
        emath_files.sort();
        if let Some(first) = emath_files.into_iter().next() {
            return Some(first);
        }
    }
    // Check models/ or src/
    for sub in &["models", "src"] {
        let dir = Path::new(sub);
        if dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                let mut emath_files: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().map_or(false, |ext| ext == "emath"))
                    .collect();
                emath_files.sort();
                if let Some(first) = emath_files.into_iter().next() {
                    return Some(first);
                }
            }
        }
    }
    None
}

fn print_triage_json(
    target: Option<&Path>,
    doctor_ok: bool,
    probes: &[crate::tooling_cmd::DoctorProbe],
    admitted: bool,
    package_id: Option<&str>,
    diagnostics: &emath_core::Diagnostics,
    goals_count: usize,
    plans_count: usize,
    plan_ok: bool,
    recommendations: &[TriageRecommendation],
) {
    let mut root = JsonWriter::object();
    root.string("schema", "emath.triage");
    root.string("tool", "emath");
    root.string("version", env!("CARGO_PKG_VERSION"));

    let mut quick_ref = JsonWriter::object();
    quick_ref.string(
        "status",
        if doctor_ok && (target.is_none() || admitted) {
            "ready"
        } else {
            "attention_required"
        },
    );
    let target_str = target.map(|p| p.display().to_string()).unwrap_or_else(|| "(none)".to_string());
    quick_ref.string("target", &target_str);
    quick_ref.bool("doctor_ready", doctor_ok);
    quick_ref.bool("admitted", admitted);
    quick_ref.int("open_goals", goals_count as u64);
    quick_ref.int("plans_count", plans_count as u64);
    quick_ref.int("recommendations_count", recommendations.len() as u64);
    root.object_field("quick_ref", &quick_ref.finish());

    let mut rec_objects = Vec::new();
    for rec in recommendations {
        let mut obj = JsonWriter::object();
        obj.int("priority", rec.priority as u64);
        obj.string("action", rec.action);
        obj.string("command", &rec.command);
        obj.string("reason", &rec.reason);
        rec_objects.push(obj.finish());
    }
    root.objects("recommendations", &rec_objects);

    let mut health = JsonWriter::object();
    health.bool("doctor_ok", doctor_ok);
    let mut probe_rows = Vec::new();
    for p in probes {
        let mut pr = JsonWriter::object();
        pr.string("name", p.name);
        pr.bool("ok", p.ok);
        if let Some(v) = &p.version {
            pr.string("version", v);
        }
        probe_rows.push(pr.finish());
    }
    health.objects("probes", &probe_rows);
    if let Some(pkg) = package_id {
        health.string("package_id", pkg);
    }
    health.bool("plan_ok", plan_ok);
    health.objects("diagnostics", &json_diagnostics_entries(diagnostics));
    root.object_field("project_health", &health.finish());

    let commands: Vec<String> = vec![
        "emath capabilities --json".to_string(),
        "emath doctor --json".to_string(),
        "emath triage --json".to_string(),
    ];
    root.strings("commands", &commands);

    println!("{}", root.finish());
}

fn print_triage_human(
    target: Option<&Path>,
    _doctor_ok: bool,
    probes: &[crate::tooling_cmd::DoctorProbe],
    admitted: bool,
    goals_count: usize,
    plans_count: usize,
    recommendations: &[TriageRecommendation],
) {
    let title = crate::terminal::stdout_bold("emath triage: orientation & action report");
    println!("{title}");
    println!("=========================================");
    println!();
    println!(
        "{}: {}",
        crate::terminal::stdout_bold("Target"),
        target.map(|p| p.display().to_string()).unwrap_or_else(|| "(none)".to_string())
    );
    let ok_count = probes.iter().filter(|p| p.ok).count();
    let health_str = if ok_count == probes.len() {
        crate::terminal::stdout_green(&format!("{ok_count}/{} probes ok", probes.len()))
    } else {
        crate::terminal::stdout_yellow(&format!("{ok_count}/{} probes ok", probes.len()))
    };
    println!("{}: {}", crate::terminal::stdout_bold("Toolchain Health"), health_str);
    if target.is_some() {
        let adm_str = if admitted {
            crate::terminal::stdout_green("admitted")
        } else {
            crate::terminal::stdout_bold_red("refused (diagnostics pending)")
        };
        println!("{}: {}", crate::terminal::stdout_bold("Admission"), adm_str);
        println!(
            "{}: {}, {}: {}",
            crate::terminal::stdout_bold("Open Goals"),
            goals_count,
            crate::terminal::stdout_bold("Plans Available"),
            plans_count
        );
    }
    println!();
    println!("{}", crate::terminal::stdout_bold("Recommended Next Actions:"));
    for rec in recommendations {
        let action_tag = crate::terminal::stdout_cyan(&format!("[{}]", rec.action));
        let cmd_text = crate::terminal::stdout_bold(&rec.command);
        println!("  {}. {action_tag} {cmd_text}", rec.priority);
        println!("     -> {}", rec.reason);
    }
    println!();
    println!("{}", crate::terminal::stdout_dim("Run with --json for machine-readable output."));
}
