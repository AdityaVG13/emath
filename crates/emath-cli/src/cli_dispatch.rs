//! Command dispatch and per-subcommand argument parsers.

use super::*;

fn language_gate(command: &Command) -> Option<(&'static str, bool, Option<&Path>)> {
    match command {
        Command::Check(FileJsonRequest::Ready { path, json, .. }) => {
            Some(("check", *json, Some(path)))
        }
        Command::Plan(FileJsonRequest::Ready { path, json, .. }) => {
            Some(("plan", *json, Some(path)))
        }
        Command::Planner(PlannerRequest::Ready { path, json, .. }) => {
            Some(("planner", *json, Some(path)))
        }
        Command::Build(BuildRequest::Ready { spec, json, .. }) => {
            Some(("build", *json, Some(spec)))
        }
        Command::Eval(args) => Some(("eval", args.json, Some(&args.path))),
        Command::Sweep(args) => Some(("sweep", args.json, Some(&args.path))),
        Command::Run { path, .. } => Some(("run", false, Some(path))),
        Command::Test { path, .. } => Some(("test", false, Some(path))),
        Command::Agent(AgentRequest::Check { path }) => Some(("agent check", false, Some(path))),
        Command::Agent(AgentRequest::Plan { path }) => Some(("agent plan", false, Some(path))),
        Command::Agent(AgentRequest::Build { path, .. }) => {
            Some(("agent build", false, Some(path)))
        }
        Command::Expand(_)
        | Command::Assumptions(_)
        | Command::Solve(_)
        | Command::Exactness(_)
        | Command::Freeze(_)
        | Command::Why(_)
        | Command::Parse(_)
        | Command::Signature(_)
        | Command::Genesis(_)
        | Command::Compile(_)
        | Command::Simulate(_)
        | Command::Fit(_)
        | Command::Repl { .. }
        | Command::WorldShow { .. }
        | Command::PortfolioShow { .. }
        | Command::Meaning(_)
        | Command::LibraryMount { .. } => Some(("semantic", false, None)),
        _ => None,
    }
}

fn locate_language_root(anchor: Option<&Path>) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let mut starts = Vec::new();
    if let Some(anchor) = anchor {
        let absolute = if anchor.is_absolute() {
            anchor.to_path_buf()
        } else {
            cwd.join(anchor)
        };
        starts.push(absolute.parent().map(Path::to_path_buf).unwrap_or(absolute));
    }
    starts.push(cwd);
    for start in starts {
        for ancestor in start.ancestors() {
            let language = ancestor.join("language");
            if language.join("spec").is_dir() {
                return Ok(language);
            }
        }
    }
    Err(
        "no project language/spec directory found from source path or working directory"
            .to_string(),
    )
}

fn verify_language_gate(command: &Command) -> Result<(), (&'static str, bool, String)> {
    let Some((name, json, anchor)) = language_gate(command) else {
        return Ok(());
    };
    let root = locate_language_root(anchor).map_err(|detail| (name, json, detail))?;
    let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
        .map_err(|error| (name, json, format!("{}: {error:?}", root.display())))?;
    emath_sema::language::install_language_distribution(&distribution)
        .map_err(|error| (name, json, format!("{}: {error:?}", root.display())))
}

pub(super) fn run_command(command: Command) -> CliExit {
    if let Err((name, json, detail)) = verify_language_gate(&command) {
        let message = format!("verified Language Image refused: {detail}");
        eprintln!("error: E-LANG-IMAGE: {message}");
        if json {
            print_json_diagnostics(
                name,
                false,
                &[json_diagnostic_entry("E-LANG-IMAGE", "error", &message)],
            );
        }
        return EXIT_REFUSED;
    }
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
        Command::Sweep(args) => eval_cmd::dispatch_sweep(args),
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

pub(super) fn parse_import_modelica(rest: &[String]) -> Option<(PathBuf, bool)> {
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

pub(super) fn parse_show_named(rest: &[String]) -> Option<(String, PathBuf)> {
    if rest.first().map(String::as_str) != Some("show") {
        return None;
    }
    let tail = &rest[1..];
    let id = tail.iter().find(|arg| !arg.starts_with('-'))?.clone();
    let (_, dir, _) = parse_genesis_args(tail)?;
    Some((id, dir?))
}

pub(super) fn parse_provider_request(rest: &[String]) -> Option<ProviderRequest> {
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

pub(super) fn parse_fork_request(rest: &[String]) -> Option<ForkRequest> {
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

pub(super) fn parse_new_request(args: &[String]) -> Option<(String, PathBuf)> {
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

pub(super) fn parse_path_out_request(args: &[String]) -> Option<(PathBuf, PathBuf)> {
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

pub(super) fn parse_required_path(args: &[String]) -> Option<PathBuf> {
    let mut path = None;
    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        assign_once(&mut path, PathBuf::from(arg))?;
    }
    path
}

pub(super) fn parse_inspect_request(args: &[String]) -> Option<(PathBuf, bool)> {
    Some((parse_required_path(args)?, catalog::wants_json(args)))
}

pub(super) fn parse_diff_request(args: &[String]) -> Option<(PathBuf, PathBuf, bool)> {
    let mut positionals = args.iter().filter(|arg| !arg.starts_with('-'));
    let a = PathBuf::from(positionals.next()?);
    let b = PathBuf::from(positionals.next()?);
    if positionals.next().is_some() {
        return None;
    }
    Some((a, b, catalog::wants_json(args)))
}

pub(super) fn parse_vendor_request(args: &[String]) -> Option<PathBuf> {
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

pub(super) fn parse_explain_request(args: &[String]) -> Option<ExplainRequest> {
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

pub(super) fn parse_agent_request(args: &[String]) -> Option<AgentRequest> {
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

pub(super) fn catalog_read_cmd(
    command: &str,
    args: &[String],
    emit: impl FnOnce() -> CliExit,
) -> CliExit {
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

pub(super) fn robot_docs_cmd() -> CliExit {
    print!("{}", catalog::robot_docs_guide());
    EXIT_OK
}

pub(super) fn help_cmd(args: &[String]) -> CliExit {
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

pub(super) fn print_command_help(command: &str) -> CliExit {
    match catalog::command_help_text(command) {
        Some(text) => {
            print!("{text}");
            EXIT_OK
        }
        None => unknown_command(command),
    }
}

pub(super) fn unknown_command(other: &str) -> CliExit {
    eprintln!("error: unknown command `{other}`");
    if let Some(hint) = catalog::suggest_command(other) {
        eprintln!("did you mean `emath {hint}`?");
        eprintln!("try: emath help {hint}");
    } else {
        eprintln!("try: emath help");
    }
    EXIT_USAGE
}

pub(super) fn next_arg<'a>(args: &'a [String], index: &mut usize) -> Option<&'a str> {
    *index += 1;
    args.get(*index).map(String::as_str)
}

pub(super) fn take_nonflag_value<'a>(args: &'a [String], index: &mut usize) -> Option<&'a str> {
    let value = next_arg(args, index)?;
    if value.starts_with("--") || matches!(value, "-o" | "-h" | "-V") {
        None
    } else {
        Some(value)
    }
}

pub(super) fn assign_once<T>(slot: &mut Option<T>, value: T) -> Option<()> {
    if slot.is_some() {
        None
    } else {
        *slot = Some(value);
        Some(())
    }
}

pub(super) fn no_extra_positionals(args: &[String]) -> bool {
    args.iter().all(|arg| arg.starts_with('-') && arg != "-")
}

pub enum CompileRequest {
    Ready {
        path: PathBuf,
        out: PathBuf,
        worlds: Vec<String>,
    },
}

pub(super) fn parse_compile_request(args: &[String]) -> Option<CompileRequest> {
    let parametric = args.iter().any(|arg| arg == "--parametric");
    let (path, out, worlds) = parse_genesis_args(args)?;
    match (path, out, parametric) {
        (Some(path), Some(out), true) => Some(CompileRequest::Ready { path, out, worlds }),
        _ => None,
    }
}

pub(super) enum FileJsonRequest {
    Ready {
        path: PathBuf,
        json: bool,
        /// `check --verify-data` (04 §5.2): re-hash declared sha256
        /// provenance files and refuse drift as `E-OBS-HASH`.
        verify_data: bool,
    },
}

pub(super) fn parse_file_json_request(args: &[String]) -> Option<FileJsonRequest> {
    parse_file_request_inner(args, false)
}

/// `check` additionally admits `--verify-data` (04 §5.2); plan/expand/
/// assumptions reject it through the catalog flag whitelist.
pub(super) fn parse_check_request(args: &[String]) -> Option<FileJsonRequest> {
    parse_file_request_inner(args, true)
}

pub(super) fn parse_file_request_inner(
    args: &[String],
    wants_verify_data: bool,
) -> Option<FileJsonRequest> {
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

pub(super) enum ParseRequest {
    Ready {
        path: PathBuf,
        out: Option<PathBuf>,
        forest_only: bool,
    },
}

pub(super) fn parse_parse_request(args: &[String]) -> Option<ParseRequest> {
    let (path, out, _) = parse_genesis_args(args)?;
    let forest_only = args.iter().any(|arg| arg == "--forest");
    Some(ParseRequest::Ready {
        path: path?,
        out,
        forest_only,
    })
}

pub(super) enum SignatureRequest {
    Ready { path: PathBuf, out: Option<PathBuf> },
}

pub(super) fn parse_signature_request(args: &[String]) -> Option<SignatureRequest> {
    let (path, out, _) = parse_genesis_args(args)?;
    Some(SignatureRequest::Ready { path: path?, out })
}

pub(super) enum GenesisRequest {
    Ready { path: PathBuf, out: PathBuf },
}

pub(super) fn parse_genesis_request(args: &[String]) -> Option<GenesisRequest> {
    let (path, out, _) = parse_genesis_args(args)?;
    Some(GenesisRequest::Ready {
        path: path?,
        out: out?,
    })
}

/// Shared arg scan for genesis commands: positional file, `--out`, `--world`.
pub(super) fn parse_genesis_args(
    args: &[String],
) -> Option<(Option<PathBuf>, Option<PathBuf>, Vec<String>)> {
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
