//! Mega-command for AI agents and developers (`emath triage` / `emath --robot-triage` / `emath next` / `emath --robot-next`).
//!
//! Provides single-call complete situational awareness:
//! - Workspace and target file orientation
//! - Toolchain presence and doctor health
//! - Semantic admission, diagnostics, and error codes
//! - Constructor admission and diagnostics
//! - Ranked next-action recommendations with exact copy-paste shell commands

use std::path::{Path, PathBuf};

use crate::tooling_cmd::doctor_probes;
use crate::{CliExit, EXIT_OK, EXIT_REFUSED, json_diagnostics_entries, run_check};
use emath_artifact::JsonWriter;

pub struct TriageRecommendation {
    pub priority: usize,
    pub action: &'static str,
    pub command: String,
    pub reason: String,
}

/// Computes ranked next actions given active path and system diagnostic state.
pub fn compute_recommendations(
    active_path: Option<&Path>,
    doctor_ok: bool,
    probes: &[crate::tooling_cmd::DoctorProbe],
    admitted: bool,
    diagnostics: &emath_core::Diagnostics,
) -> Vec<TriageRecommendation> {
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

    if let Some(path) = active_path {
        let path_str = path.display().to_string();
        if admitted {
            recommendations.push(TriageRecommendation {
                priority: prio,
                action: "build",
                command: format!("emath build {path_str} --verify --json"),
                reason: "Compile an admitted constructor program to verified Rust".to_string(),
            });
            prio += 1;
            recommendations.push(TriageRecommendation {
                priority: prio,
                action: "run",
                command: format!("emath run {path_str} --json"),
                reason: "Run an ordinary `emath function` or `emath query`".to_string(),
            });
        } else {
            let first_error = diagnostics.errors().next().map(|d| d.code);
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
                reason: "Resolve admission errors in constructor source".to_string(),
            });
        }
    } else {
        recommendations.push(TriageRecommendation {
            priority: prio,
            action: "scaffold_program",
            command: "emath new my_program".to_string(),
            reason: "No .emath programs found in current directory; create a new project scaffold".to_string(),
        });
        prio += 1;
        recommendations.push(TriageRecommendation {
            priority: prio,
            action: "capabilities",
            command: "emath capabilities --json".to_string(),
            reason: "Inspect the constructor CLI contract".to_string(),
        });
    }

    recommendations
}

pub fn triage_cmd(target: Option<PathBuf>, json: bool) -> CliExit {
    let probes = doctor_probes();
    let doctor_ok = probes.iter().all(|probe| probe.ok);

    let active_path = target.or_else(discover_target_file);

    let (admitted, package_id, diagnostics) = match &active_path {
        Some(path) if path.exists() => {
            let (diags, pkg_id, _) = run_check(path);
            (!diags.has_errors(), Some(pkg_id), diags)
        }
        _ => (false, None, emath_core::Diagnostics::new()),
    };

    let recommendations = compute_recommendations(
        active_path.as_deref(),
        doctor_ok,
        &probes,
        admitted,
        &diagnostics,
    );

    if json {
        print_triage_json(
            active_path.as_deref(),
            doctor_ok,
            &probes,
            admitted,
            package_id.as_deref(),
            &diagnostics,
            &recommendations,
        );
    } else {
        print_triage_human(
            active_path.as_deref(),
            doctor_ok,
            &probes,
            admitted,
            &recommendations,
        );
    }

    if doctor_ok && (active_path.is_none() || admitted) {
        EXIT_OK
    } else {
        EXIT_REFUSED
    }
}

/// Next-action engine command (`emath next` / `emath --robot-next`).
/// Emits the single highest-priority next action for agent loops.
pub fn next_cmd(target: Option<PathBuf>, json: bool) -> CliExit {
    let probes = doctor_probes();
    let doctor_ok = probes.iter().all(|probe| probe.ok);

    let active_path = target.or_else(discover_target_file);

    let (admitted, diagnostics) = match &active_path {
        Some(path) if path.exists() => {
            let (diags, _, _) = run_check(path);
            let adm = !diags.has_errors();
            (adm, diags)
        }
        _ => (false, emath_core::Diagnostics::new()),
    };

    let recommendations = compute_recommendations(
        active_path.as_deref(),
        doctor_ok,
        &probes,
        admitted,
        &diagnostics,
    );

    let top = recommendations.first();

    if json {
        let mut root = JsonWriter::object();
        root.string("schema", "emath.next");
        root.string("tool", "emath");
        root.string("version", env!("CARGO_PKG_VERSION"));
        root.string(
            "status",
            if doctor_ok && (active_path.is_none() || admitted) {
                "ready"
            } else {
                "attention_required"
            },
        );

        if let Some(rec) = top {
            root.string("action", rec.action);
            root.int("priority", rec.priority as u64);
            root.string("command", &rec.command);
            root.string("reason", &rec.reason);
            root.string("claim_command", &rec.command);
        } else {
            root.string("action", "none");
            root.int("priority", 0);
            root.string("command", "");
            root.string("reason", "No pending actions found");
            root.string("claim_command", "");
        }
        println!("{}", root.finish());
    } else {
        println!("{}", crate::terminal::stdout_bold("emath next-action:"));
        if let Some(rec) = top {
            let action_tag = crate::terminal::stdout_cyan(&format!("[{}]", rec.action));
            let cmd_text = crate::terminal::stdout_bold(&rec.command);
            println!("  {action_tag} {cmd_text}");
            println!("  -> {}", rec.reason);
            println!();
            println!(
                "Run command to proceed. Pass {} for machine-readable output.",
                crate::terminal::stdout_dim("--json")
            );
        } else {
            println!("  All actions clear; workspace ready.");
        }
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
            .filter(|p| p.extension().is_some_and(|ext| ext == "emath"))
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
                    .filter(|p| p.extension().is_some_and(|ext| ext == "emath"))
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
    let target_str = target.map_or_else(|| "(none)".to_string(), |p| p.display().to_string());
    quick_ref.string("target", &target_str);
    quick_ref.bool("doctor_ready", doctor_ok);
    quick_ref.bool("admitted", admitted);
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
    recommendations: &[TriageRecommendation],
) {
    let title = crate::terminal::stdout_bold("emath triage: orientation & action report");
    println!("{title}");
    println!("=========================================");
    println!();
    println!(
        "{}: {}",
        crate::terminal::stdout_bold("Target"),
        target.map_or_else(|| "(none)".to_string(), |p| p.display().to_string())
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
