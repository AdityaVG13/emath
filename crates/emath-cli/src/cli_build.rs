//! `emath plan`/`build`/`planner` pipelines and plan inspections.

use super::*;
use emath_core::Diagnostics;

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
    let mut diagnostics = result.diagnostics;
    if !diagnostics.has_errors()
        && let Ok(source) = std::fs::read_to_string(path)
    {
        merge_constructor_admit(&source, Some(path), &mut diagnostics);
    }
    let package_id = result.package.content_id().0;
    (diagnostics, package_id, result.units_profiles)
}

/// Stdin variant of [`run_check`] (`check -`): same shape, source read
/// from a string. Deterministic; the package id derives from the source
/// bytes alone, so piped and on-disk checks of identical text agree.
pub fn run_check_source(name: &str, source: &str) -> (Diagnostics, String, Vec<(String, String)>) {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned(name, source);
    let mut diagnostics = result.diagnostics;
    if !diagnostics.has_errors() {
        merge_constructor_admit(source, None, &mut diagnostics);
    }
    let package_id = result.package.content_id().0;
    (diagnostics, package_id, result.units_profiles)
}

fn merge_constructor_admit(source: &str, path: Option<&Path>, diagnostics: &mut Diagnostics) {
    let (tree, parse) = emath_syntax::parse_str(source);
    if parse.has_errors() {
        diagnostics.extend_from(&parse);
        return;
    }
    let has_constructor = tree.items.iter().any(|item| matches!(
        item,
        emath_core::tree::Item::Declaration(decl)
            if matches!(decl.as_kind.as_str(), "object" | "function" | "query")
    ));
    if !has_constructor {
        diagnostics.error(
            "E-KIND-GONE",
            "`emath check` admits `emath object`, `emath function`, and `emath query`. Other source is not a constructor program.",
            emath_core::Span::default(),
        );
        return;
    }
    let admitted = match path {
        Some(path) => emath_exec_ir::constructor_layer::admit_tree_at(&tree, Some(path)),
        None => emath_exec_ir::constructor_layer::admit_tree(&tree),
    };
    if let Err(error) = admitted {
        diagnostics.error(
            constructor_admit_code(&error.code),
            format!("{}: {}", error.code, error.message),
            emath_core::Span::default(),
        );
    }
}

fn constructor_admit_code(code: &str) -> &'static str {
    match code {
        "E-KIND-GONE" => "E-KIND-GONE",
        "E-PKG-050" => "E-PKG-050",
        "E-USE-ADMISSION" => "E-USE-ADMISSION",
        "E-TYPE-002" | "unbound" => "E-TYPE-002",
        "E-NAME-020" => "E-NAME-020",
        "E-TYPE-003" => "E-TYPE-003",
        "E-TYPE-010" | "type" => "E-TYPE-010",
        "E-SEC-101" => "E-SEC-101",
        "E-KIND-011" => "E-KIND-011",
        _ => "E-TYPE-002",
    }
}

/// `plan <file> [--json]`: check + goals + plans, no artifact.
#[allow(unreachable_code, unused_variables)]
pub fn plan(path: &Path, json: bool) -> CliExit {
    return refuse_coded(
        "plan",
        json,
        EXIT_ADMISSION,
        "E-KIND-GONE",
        "`emath plan` is not a constructor command. There is no `goals:` layer. Write an ordinary `emath function` or `emath query` and `emath run`.",
    );
    if let Some(code) = refuse_malformed_project_lock(path) {
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
        dry_run: bool,
        json: bool,
    },
}

pub(super) fn parse_build_request(args: &[String]) -> Option<BuildRequest> {
    let mut path = None;
    let mut out = None;
    let mut dry_run = false;
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
            "--dry-run" => dry_run = true,
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
        dry_run,
        json,
    })
}

/// `build <file> [--out <dir>] [--dry-run] [--json]`
/// (default out: `target/emath` under the working directory).
pub fn build(request: BuildRequest) -> CliExit {
    let BuildRequest::Ready {
        spec,
        out,
        dry_run,
        json,
    } = request;
    if let Some(code) = refuse_malformed_project_lock(&spec) {
        return code;
    }
    if dry_run {
        let mut session = CompilerSession::new(emath_core::limits::Limits::default());
        let Ok(package) = session.load_package(&spec) else {
            return refuse_coded(
                "build",
                json,
                EXIT_USAGE,
                "E-PKG-080",
                &format!("cannot read spec: {}", spec.display()),
            );
        };
        let check_result = session.check(package.file);
        if check_result.diagnostics.has_errors() {
            crate::print_diagnostics(&check_result.diagnostics);
            if json {
                let items: Vec<String> = check_result
                    .diagnostics
                    .items()
                    .iter()
                    .map(|d| {
                        crate::json_diagnostic_entry(
                            &d.code,
                            match d.severity {
                                emath_core::Severity::Error => "error",
                                emath_core::Severity::Warning => "warning",
                                emath_core::Severity::Note => "note",
                            },
                            &d.message,
                        )
                    })
                    .collect();
                crate::print_json_diagnostics("build", false, &items);
            }
            return EXIT_REFUSED;
        }
        let package_id = check_result.package.content_id();
        let crate_name = check_result
            .package
            .identity
            .as_ref()
            .map_or_else(|| "package".to_string(), |id| id.name.clone());
        if json {
            let mut obj = emath_artifact::JsonWriter::object();
            obj.string("command", "build");
            obj.string("status", "ok");
            obj.string("schema", "emath.constructor-emission.v1");
            obj.bool("dry_run", true);
            obj.string("package_id", &package_id.0);
            obj.string("crate", &crate_name);
            obj.string("target_dir", &out.display().to_string());
            println!("{}", obj.finish());
        } else {
            println!("dry-run: emath build `{}`", spec.display());
            println!("target directory: {}", out.display());
            println!("crate: {crate_name} (package {})", package_id.0);
        }
        return EXIT_OK;
    }
    if let Some(exit) = build_constructor_file(&spec, &out, json) {
        return exit;
    }
    return refuse_coded(
        "build",
        json,
        EXIT_ADMISSION,
        "E-KIND-GONE",
        "`emath build` emits constructor functions. Write `emath object`, `emath function`, or `emath query`.",
    );
}

/// Plan inspections for `emath explain <file>` / `--json`.
///
/// Each object is `PlanInspection::to_json` (`emath.plan-explanation v1`).
/// Admission failures are `Err(EXIT_REFUSED)` / `Err(EXIT_USAGE)`.
#[allow(unreachable_code, unused_variables)]
pub fn explain_inspections(path: &Path) -> Result<Vec<PlanInspection>, CliExit> {
    let _ = path;
    eprintln!(
        "error: E-KIND-GONE: file explanation is not a constructor command. There is no goals planner. Use `emath check` or `emath run`."
    );
    return Err(EXIT_ADMISSION);
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
#[allow(unreachable_code, unused_variables)]
pub fn planner_cmd(request: PlannerRequest) -> CliExit {
    let json = match &request {
        PlannerRequest::Ready { json, .. } => *json,
    };
    return refuse_coded(
        "planner",
        json,
        EXIT_ADMISSION,
        "E-KIND-GONE",
        "`emath planner` is not a constructor command. Write an ordinary `emath function` or `emath query` and `emath run`.",
    );
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

fn build_constructor_file(spec: &Path, out: &Path, json: bool) -> Option<CliExit> {
    let source = match std::fs::read_to_string(spec) {
        Ok(source) => source,
        Err(error) => {
            return Some(refuse_coded(
                "build",
                json,
                EXIT_USAGE,
                "E-PKG-080",
                &error.to_string(),
            ));
        }
    };
    let (tree, diagnostics) = emath_syntax::parse_str(&source);
    if diagnostics.has_errors() {
        crate::print_diagnostics(&diagnostics);
        return Some(EXIT_ADMISSION);
    }
    let constructor_only = tree.items.iter().all(|item| match item {
        emath_core::tree::Item::Use { .. } => true,
        emath_core::tree::Item::Declaration(decl) => {
            matches!(decl.as_kind.as_str(), "object" | "function" | "query")
        }
        _ => true,
    });
    if !constructor_only {
        return Some(refuse_coded(
            "build",
            json,
            EXIT_ADMISSION,
            "E-KIND-GONE",
            "`emath build` emits constructor functions. Write `emath object`, `emath function`, or `emath query`.",
        ));
    }
    let functions: Vec<String> = tree
        .items
        .iter()
        .filter_map(|item| match item {
            emath_core::tree::Item::Declaration(decl) if decl.as_kind == "function" => {
                Some(decl.name.clone())
            }
            _ => None,
        })
        .collect();
    if functions.is_empty() {
        return Some(refuse_coded(
            "build",
            json,
            EXIT_ADMISSION,
            "E-KIND-GONE",
            "`emath build` emits lowered Rust for `emath function` entries. Symbolic query-only files are not marked runnable.",
        ));
    }
    let mut rust = String::from("#![forbid(unsafe_code)]\n\n");
    let mut runnable = true;
    let mut unresolved = Vec::new();
    // One runnable entry per crate (Phase 1): a second runnable
    // function would emit a second `pub fn entry` into the same
    // lib.rs — a duplicate symbol the crate could never compile.
    // Not-runnable siblings emit as comments and never count.
    let mut entries = 0usize;
    for name in &functions {
        match emath_exec_ir::constructor_emir::lower_constructor_function(&tree, name) {
            Ok(lowered) => {
                if !lowered.runnable {
                    runnable = false;
                    unresolved.extend(lowered.unresolved);
                    rust.push_str(&format!(
                        "// `{name}` is not marked runnable: unresolved symbolic code\n"
                    ));
                    continue;
                }
                match emath_rust_backend::emit_constructor_program(&lowered.program, &lowered.inputs)
                {
                    Ok(body) => {
                        entries += 1;
                        if entries > 1 {
                            return Some(refuse_coded(
                                "build",
                                json,
                                EXIT_ADMISSION,
                                "E-CODEGEN-013",
                                &format!(
                                    "`emath build` emits one entry per crate (Phase 1): `{name}` is a second runnable function in this file. Split the file into one `emath function` per file."
                                ),
                            ));
                        }
                        rust.push_str(&format!("// function `{name}`\n"));
                        rust.push_str(&body);
                        rust.push('\n');
                    }
                    Err(err) => {
                        runnable = false;
                        unresolved.push(err.to_string());
                    }
                }
            }
            Err(err) => {
                runnable = false;
                unresolved.push(err);
            }
        }
    }
    if let Err(err) = std::fs::create_dir_all(out.join("src")) {
        return Some(refuse_coded(
            "build",
            json,
            EXIT_FAULT,
            "E-IO",
            &err.to_string(),
        ));
    }
    if let Err(err) = std::fs::write(out.join("src/lib.rs"), rust) {
        return Some(refuse_coded(
            "build",
            json,
            EXIT_FAULT,
            "E-IO",
            &err.to_string(),
        ));
    }
    let status = if runnable { "runnable" } else { "not-runnable" };
    if json {
        let mut object = emath_artifact::JsonWriter::object();
        object.string("command", "build");
        object.string("status", "ok");
        object.string("schema", "emath.constructor-emission.v1");
        object.bool("runnable", runnable);
        object.strings("functions", &functions);
        object.strings("unresolved", &unresolved);
        object.string("artifact_dir", &out.display().to_string());
        println!("{}", object.finish());
    } else {
        println!(
            "constructor emission {status} → {} ({})",
            out.display(),
            if runnable {
                "runnable"
            } else {
                "symbolic code is not marked runnable"
            }
        );
    }
    Some(EXIT_OK)
}
