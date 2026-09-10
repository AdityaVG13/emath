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
        Command::Test { path, .. } => Some(("test", false, Some(path))),
        Command::Simulate(_) => Some(("semantic", false, None)),
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

pub(crate) fn load_verified_language(
    anchor: Option<&Path>,
) -> Result<emath_exec_ir::language_image::LanguageDistribution, String> {
    let root = locate_language_root(anchor)?;
    let distribution = emath_exec_ir::language_image::load_language_distribution(&root)
        .map_err(|error| format!("{}: {error:?}", root.display()))?;
    emath_sema::language::install_language_distribution(&distribution)
        .map_err(|error| format!("{}: {error:?}", root.display()))?;
    emath_exec_ir::native_kernel::install_language_distribution(&distribution)
        .map_err(|error| format!("{}: {error:?}", root.display()))?;
    Ok(distribution)
}

fn verify_language_image(
    name: &'static str,
    json: bool,
    anchor: Option<&Path>,
) -> Result<(), (&'static str, bool, String)> {
    load_verified_language(anchor)
        .map(|_| ())
        .map_err(|detail| (name, json, detail))
}

fn verify_language_gate(command: &Command) -> Result<(), (&'static str, bool, String)> {
    let Some((name, json, anchor)) = language_gate(command) else {
        return Ok(());
    };
    verify_language_image(name, json, anchor)
}

/// Shared Language Image gate for production keep-commands and `emath-lab`.
pub fn refuse_unverified_language_image(
    command: &'static str,
    json: bool,
    anchor: Option<&Path>,
) -> Option<CliExit> {
    match verify_language_image(command, json, anchor) {
        Ok(()) => None,
        Err((name, json, detail)) => {
            let message = format!("verified Language Image refused: {detail}");
            eprintln!("error: E-LANG-IMAGE: {message}");
            if json {
                print_json_diagnostics(
                    name,
                    false,
                    &[json_diagnostic_entry("E-LANG-IMAGE", "error", &message)],
                );
            }
            Some(EXIT_REFUSED)
        }
    }
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
        Command::Simulate(args) => simulate_cmd::dispatch_simulate(&args),
        Command::New {
            name,
            out,
            dry_run,
            force,
            json,
        } => tooling_cmd::new_cmd(&name, &out, dry_run, force, json),
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
            dry_run,
            receipt,
            list_rules,
            json,
        } => tooling_cmd::migrate_cmd(
            &path,
            fix,
            check_only,
            dry_run,
            receipt.as_deref(),
            list_rules,
            json,
        ),
        Command::Explain(request) => tooling_cmd::explain_cmd(request),
        Command::Run(request) => execution::run(request),
        Command::Search(request) => compiled_search::run(request),
        Command::Step(request) => execution::step(request),
        Command::Api(request) => language_cmd::api(request),
        Command::Test { path, out } => tooling_cmd::test_cmd(&path, &out),
        Command::Verify { dir, json } => {
            if !dir.is_dir() {
                execution::verify(&dir, json)
            } else if json {
                execution::diagnostic(
                    true,
                    EXIT_USAGE,
                    "E-CLI-USAGE",
                    "--json verification requires a saved run file; artifact-directory verification uses emath verify <dir>",
                )
            } else {
                tooling_cmd::verify_cmd(&dir)
            }
        }
        Command::Inspect { dir, json } => {
            if dir.is_dir() {
                tooling_cmd::inspect_cmd(&dir, json)
            } else {
                execution::inspect(&dir, json)
            }
        }
        Command::Diff { a, b, json } => tooling_cmd::diff_cmd(&a, &b, json),
        Command::Doctor { json } => tooling_cmd::doctor_cmd(json),
    }
}

pub fn parse_show_named(rest: &[String]) -> Option<(String, PathBuf)> {
    if rest.first().map(String::as_str) != Some("show") {
        return None;
    }
    let tail = &rest[1..];
    let id = tail.iter().find(|arg| !arg.starts_with('-'))?.clone();
    let (_, dir, _) = parse_genesis_args(tail)?;
    Some((id, dir?))
}

pub(super) fn parse_new_request(
    args: &[String],
) -> Option<(String, PathBuf, bool, bool, bool)> {
    let mut name = None;
    let mut out = None;
    let mut dry_run = false;
    let mut force = false;
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
            "--force" => force = true,
            "--json" => json = true,
            other if other.starts_with('-') && other != "-" => return None,
            other => assign_once(&mut name, other.to_string())?,
        }
        index += 1;
    }
    let name = name?;
    let out = out.unwrap_or_else(|| PathBuf::from(&name));
    Some((name, out, dry_run, force, json))
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

pub(super) fn catalog_read_cmd(
    command: &str,
    args: &[String],
    emit: impl FnOnce() -> CliExit,
) -> CliExit {
    if catalog::wants_help(args) {
        return print_command_help(command, catalog::wants_json(args));
    }
    if let Some(code) = catalog::reject_unknown_flags(command, args) {
        return code;
    }
    if command == "robot-docs" {
        let positionals: Vec<&str> = args
            .iter()
            .filter(|arg| !arg.starts_with('-'))
            .map(String::as_str)
            .collect();
        if positionals.len() > 1 || (positionals.len() == 1 && positionals[0] != "guide") {
            return usage(command);
        }
    } else if !no_extra_positionals(args) {
        return usage(command);
    }
    emit()
}

pub(super) fn help_cmd(args: &[String]) -> CliExit {
    let json = catalog::wants_json(args);
    let mut command: Option<&str> = None;
    for arg in args {
        if arg == "--json" || arg == "--help" || arg == "-h" || arg == "help" {
            continue;
        }
        if !arg.starts_with('-') && command.is_none() {
            command = Some(arg.as_str());
        } else {
            return usage("help [<command>] [--json]");
        }
    }

    match command {
        None => {
            if json {
                println!("{}", catalog::catalog_help_json());
            } else {
                print!("{}", help_text());
            }
            EXIT_OK
        }
        Some(cmd) => {
            let canonical = catalog::resolve_alias(cmd).unwrap_or(cmd);
            print_command_help(canonical, json)
        }
    }
}

pub(super) fn print_command_help(command: &str, json: bool) -> CliExit {
    let resolved = catalog::resolve_alias(command).unwrap_or(command);
    if json {
        match catalog::command_help_json(resolved) {
            Some(json_text) => {
                println!("{json_text}");
                EXIT_OK
            }
            None => unknown_command(command, true),
        }
    } else {
        match catalog::command_help_text(resolved) {
            Some(text) => {
                print!("{text}");
                EXIT_OK
            }
            None => unknown_command(command, false),
        }
    }
}

pub(super) fn unknown_command(other: &str, json: bool) -> CliExit {
    if catalog::EXTRACTED_COMMANDS.contains(&other) {
        let err = PedagogicError::new(
            "E-CLI-UNKNOWN-COMMAND",
            format!("`{other}` lives in `emath-lab`, not production `emath`"),
            format!("command position 1 (`{other}`)"),
            format!("emath-lab {other}"),
        )
        .with_did_you_mean(format!("emath-lab {other}"))
        .with_help(format!("emath-lab help {other}"));
        return err.emit(json);
    }
    let hint = catalog::suggest_command(other);
    let (remediation, did_you_mean, help) = match hint {
        Some(h) if catalog::EXTRACTED_COMMANDS.contains(&h) => (
            format!("emath-lab {h}"),
            Some(format!("emath-lab {h}")),
            format!("emath-lab help {h}"),
        ),
        Some(h) => (
            format!("emath {h}"),
            Some(format!("emath {h}")),
            format!("emath help {h}"),
        ),
        None => (
            "run `emath help` or `emath capabilities` to list available commands".to_string(),
            None,
            "emath help".to_string(),
        ),
    };
    let mut err = PedagogicError::new(
        "E-CLI-UNKNOWN-COMMAND",
        format!("unknown command `{other}`"),
        format!("command position 1 (`{other}`)"),
        remediation,
    )
    .with_help(help);
    if let Some(dym) = did_you_mean {
        err = err.with_did_you_mean(dym);
    }
    err.emit(json)
}

pub(super) fn next_arg<'a>(args: &'a [String], index: &mut usize) -> Option<&'a str> {
    *index += 1;
    args.get(*index).map(String::as_str)
}

pub fn take_nonflag_value<'a>(args: &'a [String], index: &mut usize) -> Option<&'a str> {
    let value = next_arg(args, index)?;
    if value.starts_with("--") || matches!(value, "-o" | "-h" | "-V") {
        None
    } else {
        Some(value)
    }
}

pub fn assign_once<T>(slot: &mut Option<T>, value: T) -> Option<()> {
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

pub fn parse_compile_request(args: &[String]) -> Option<CompileRequest> {
    let parametric = args.iter().any(|arg| arg == "--parametric");
    let (path, out, worlds) = parse_genesis_args(args)?;
    match (path, out, parametric) {
        (Some(path), Some(out), true) => Some(CompileRequest::Ready { path, out, worlds }),
        _ => None,
    }
}

pub enum FileJsonRequest {
    Ready {
        path: PathBuf,
        json: bool,
        /// `check --verify-data` (04 §5.2): re-hash declared sha256
        /// provenance files and refuse drift as `E-OBS-HASH`.
        verify_data: bool,
    },
}

pub fn parse_file_json_request(args: &[String]) -> Option<FileJsonRequest> {
    parse_file_request_inner(args, false)
}

/// `check` additionally admits `--verify-data` (04 §5.2); plan/expand/
/// assumptions reject it through the catalog flag whitelist.
pub fn parse_check_request(args: &[String]) -> Option<FileJsonRequest> {
    parse_file_request_inner(args, true)
}

pub fn parse_file_request_inner(
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

pub enum ParseRequest {
    Ready {
        path: PathBuf,
        out: Option<PathBuf>,
        forest_only: bool,
    },
}

pub fn parse_parse_request(args: &[String]) -> Option<ParseRequest> {
    let (path, out, _) = parse_genesis_args(args)?;
    let forest_only = args.iter().any(|arg| arg == "--forest");
    Some(ParseRequest::Ready {
        path: path?,
        out,
        forest_only,
    })
}

pub enum SignatureRequest {
    Ready { path: PathBuf, out: Option<PathBuf> },
}

pub fn parse_signature_request(args: &[String]) -> Option<SignatureRequest> {
    let (path, out, _) = parse_genesis_args(args)?;
    Some(SignatureRequest::Ready { path: path?, out })
}

pub enum GenesisRequest {
    Ready { path: PathBuf, out: PathBuf },
}

pub fn parse_genesis_request(args: &[String]) -> Option<GenesisRequest> {
    let (path, out, _) = parse_genesis_args(args)?;
    Some(GenesisRequest::Ready {
        path: path?,
        out: out?,
    })
}

/// Shared arg scan for genesis commands: positional file, `--out`, `--world`.
pub fn parse_genesis_args(
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

pub fn usage(message: &str) -> CliExit {
    let command = message.split_whitespace().next().unwrap_or("help");
    let err = PedagogicError::new(
        "E-CLI-USAGE",
        format!("invalid or missing arguments for `emath {command}`"),
        format!("arguments for `emath {command}`"),
        format!("emath {message}"),
    )
    .with_command(command)
    .with_usage(format!("emath {message}"));
    err.emit(false)
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
