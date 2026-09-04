//! `emath plan`/`build`/`planner` pipelines and plan inspections.

use super::*;

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
    (result.diagnostics, package_id, Vec::new())
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
        bin: Option<String>,
        json: bool,
    },
}

pub(super) fn parse_build_request(args: &[String]) -> Option<BuildRequest> {
    let mut path = None;
    let mut out = None;
    let mut verify = false;
    let mut bin = None;
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
            "--bin" => {
                let value = take_nonflag_value(args, &mut index)?;
                if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    return None;
                }
                assign_once(&mut bin, value.to_string())?;
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
        bin,
        json,
    })
}

/// `build <file> [--out <dir>] [--verify] [--bin <entrypoint>] [--json]`
/// (default out: `target/emath` under the working directory).
pub fn build(request: BuildRequest) -> CliExit {
    let BuildRequest::Ready {
        spec,
        out,
        verify,
        bin,
        json,
    } = request;
    if let Some(code) = meaning_cmd::refuse_malformed_project_lock(&spec) {
        return code;
    }
    let options = BuildOptions {
        verify_generated_crate: verify,
        bin_entrypoint: bin,
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
            if let Some(probe) = &report.probe_binary {
                println!("compiled-probe → {}", probe.display());
            }
            if json {
                let mut object = emath_artifact::JsonWriter::object();
                object.string("command", "build");
                object.string("artifact_id", &report.artifact_id.0);
                object.string("package_id", &report.package_id.0);
                object.string("crate", &report.crate_name);
                object.string("artifact_dir", &report.artifact_dir.display().to_string());
                if let Some(probe) = &report.probe_binary {
                    object.string("probe_binary", &probe.display().to_string());
                }
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

pub(super) fn inspections_from_plan_result(result: &emath_sema::PlanResult) -> Vec<PlanInspection> {
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

pub(super) fn print_inspection_json(json: bool, inspection: &PlanInspection) {
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

pub(super) fn parse_planner_request(args: &[String]) -> Option<PlannerRequest> {
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
