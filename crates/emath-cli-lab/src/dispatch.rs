//! Combined host: constructor tokens go to `emath_cli::run`; extracted
//! tokens refuse `E-KIND-GONE`. Command bodies stay below that gate.

use crate::agent_cmd::{self, AgentRequest};
use crate::catalog;
use crate::cli_freeze::{
    FreezeRequest, WhyRequest, assumptions_cmd, freeze_cmd, parse_freeze_request,
    parse_why_request, why_cmd,
};
use crate::cli_scratch::{
    ExactnessRequest, ParsedSolve, exactness_cmd, expand_cmd, parse_exactness_request,
    parse_solve_request, solve_check_cmd,
};
use crate::eval_cmd;
use crate::fit_cmd;
use crate::host_cmd::{self, architecture, artifact_battery, import_modelica_cmd};
use crate::provider_cmd::{self, ForkRequest, ProviderRequest};
use crate::vendor_cmd;
use crate::{
    CliExit, CompileRequest, EXIT_OK, EXIT_REFUSED, EXIT_USAGE, FileJsonRequest, GenesisRequest,
    ParseRequest, SignatureRequest, parse_compile_request, parse_file_json_request,
    parse_genesis_request, parse_parse_request, parse_show_named, parse_signature_request,
    refuse_coded, usage,
};
use emath_cli::catalog::{self as core_catalog, wants_help, wants_json};
use crate::genesis_cmd;
use crate::meaning_cmd::{self, MeaningRequest};
use std::path::{Path, PathBuf};

enum ParsedCli<'a> {
    Empty,
    MetaHelp { rest: &'a [String] },
    MetaVersion { rest: &'a [String] },
    CommandHelp { name: &'a str },
    UnknownFlag { code: CliExit },
    Usage(&'static str),
    Known(Command),
    Forward(&'a [String]),
    Unknown(&'a str),
}

enum Command {
    Expand(FileJsonRequest),
    Assumptions(FileJsonRequest),
    Solve(ParsedSolve),
    Exactness(ExactnessRequest),
    Freeze(FreezeRequest),
    Why(WhyRequest),
    Parse(ParseRequest),
    Compile(CompileRequest),
    LibraryMount { name: String },
    Signature(SignatureRequest),
    Genesis(GenesisRequest),
    Eval(eval_cmd::EvalArgs),
    Sweep(eval_cmd::SweepArgs),
    Fit(fit_cmd::FitArgs),
    Repl { path: PathBuf },
    WorldShow { id: String, dir: PathBuf },
    PortfolioShow { id: String, dir: PathBuf },
    Meaning(MeaningRequest),
    ImportModelica { path: PathBuf, json: bool },
    ArtifactCheck(PathBuf),
    ArtifactBattery(PathBuf),
    Architecture { json: bool },
    Web(crate::serve_cmd::ServeArgs),
    Serve(crate::serve_cmd::ServeArgs),
    RobotDocs,
    Provider(ProviderRequest),
    Fork(ForkRequest),
    Capabilities,
    Agent(AgentRequest),
    Coverage(Vec<String>),
    Vendor { out: PathBuf },
    Bench { path: PathBuf },
}

/// Combined host used by tests and the `emath-lab` binary.
pub fn run(args: &[String]) -> CliExit {
    match parse_cli(args) {
        ParsedCli::Empty => {
            print!("{}", catalog::help_text());
            EXIT_OK
        }
        ParsedCli::MetaHelp { rest } => help_cmd(rest),
        ParsedCli::MetaVersion { rest } => emath_cli::run(prepend_version(rest).as_slice()),
        ParsedCli::CommandHelp { name } => {
            if catalog::is_extracted_token(name) {
                refuse_extracted(name, false)
            } else {
                catalog::print_command_help(name)
            }
        }
        ParsedCli::UnknownFlag { code } => code,
        ParsedCli::Usage(message) => usage(message),
        ParsedCli::Unknown(name) => unknown_command(name),
        ParsedCli::Forward(forward) => emath_cli::run(forward),
        ParsedCli::Known(command) => run_command(command),
    }
}

fn prepend_version(rest: &[String]) -> Vec<String> {
    let mut out = vec!["version".to_string()];
    out.extend(rest.iter().cloned());
    out
}

fn parse_cli(args: &[String]) -> ParsedCli<'_> {
    let Some(first) = args.first() else {
        return ParsedCli::Empty;
    };
    let rest = &args[1..];
    match first.as_str() {
        "help" | "--help" | "-h" => return ParsedCli::MetaHelp { rest },
        "version" | "--version" | "-V" => return ParsedCli::MetaVersion { rest },
        _ => {}
    }
    if wants_help(rest) {
        return ParsedCli::CommandHelp { name: first };
    }
    if let Some(code) = core_catalog::reject_unknown_flags(first, rest) {
        return ParsedCli::UnknownFlag { code };
    }
    if catalog::is_core_command(first) {
        return ParsedCli::Forward(args);
    }
    match parse_known(first.as_str(), rest) {
        Ok(command) => ParsedCli::Known(command),
        Err(ParseKnownError::Usage(message)) => ParsedCli::Usage(message),
        Err(ParseKnownError::Unknown) => ParsedCli::Unknown(first),
    }
}

enum ParseKnownError {
    Usage(&'static str),
    Unknown,
}

fn parse_known(name: &str, rest: &[String]) -> Result<Command, ParseKnownError> {
    match name {
        "expand" => parse_file_json_request(rest)
            .map(Command::Expand)
            .ok_or(ParseKnownError::Usage("expand <file.emath> [--json]")),
        "solve" => Ok(Command::Solve(parse_solve_request(rest))),
        "exactness" => parse_exactness_request(rest)
            .map(Command::Exactness)
            .ok_or(ParseKnownError::Usage(
                "exactness <file.emath> [--json] [--raise units]",
            )),
        "freeze" => parse_freeze_request(rest)
            .map(Command::Freeze)
            .ok_or(ParseKnownError::Usage(
                "freeze <file.emath> [--out <file>] [--json]",
            )),
        "why" => parse_why_request(rest)
            .map(Command::Why)
            .ok_or(ParseKnownError::Usage(
                "why <file.emath> inference:N [--json]",
            )),
        "assumptions" => parse_file_json_request(rest)
            .map(Command::Assumptions)
            .ok_or(ParseKnownError::Usage("assumptions <file.emath> [--json]")),
        "parse" => parse_parse_request(rest)
            .map(Command::Parse)
            .ok_or(ParseKnownError::Usage(
                "parse --forest <file.emath> [--out <dir>]",
            )),
        "compile" => parse_compile_request(rest)
            .map(Command::Compile)
            .ok_or(ParseKnownError::Usage(
                "compile --parametric <file.emath> --out <dir> [--world LABEL]",
            )),
        "library" => match rest {
            [sub, name] if sub == "mount" => Ok(Command::LibraryMount { name: name.clone() }),
            _ => Err(ParseKnownError::Usage("library mount <name>")),
        },
        "signature" => parse_signature_request(rest)
            .map(Command::Signature)
            .ok_or(ParseKnownError::Usage(
                "signature <file.emath> [--out <dir>]",
            )),
        "genesis" => parse_genesis_request(rest)
            .map(Command::Genesis)
            .ok_or(ParseKnownError::Usage("genesis <file.emath> --out <dir>")),
        "eval" => eval_cmd::parse_eval_args(rest)
            .map(Command::Eval)
            .ok_or(ParseKnownError::Usage(
                "eval <file.emath> [--world <name>] [--function NAME] [--set name=value] [--json]",
            )),
        "sweep" => eval_cmd::parse_sweep_args(rest).map(Command::Sweep).ok_or(
            ParseKnownError::Usage(
                "sweep <file.emath> --function NAME --grid name=v1,v2,... [--expect name=value] [--out <file>] [--json]",
            ),
        ),
        "fit" => match fit_cmd::parse_fit_args(rest) {
            Ok(parsed) => Ok(Command::Fit(parsed)),
            Err(message) => {
                eprintln!("error: {message}");
                Err(ParseKnownError::Usage("fit <file.emath> [--json]"))
            }
        },
        "repl" => eval_cmd::parse_repl_path(rest)
            .map(|path| Command::Repl { path })
            .ok_or(ParseKnownError::Usage("repl <file.emath>")),
        "world" => parse_show_named(rest)
            .map(|(id, dir)| Command::WorldShow { id, dir })
            .ok_or(ParseKnownError::Usage("world show WORLD_ID --dir <dir>")),
        "portfolio" => parse_show_named(rest)
            .map(|(id, dir)| Command::PortfolioShow { id, dir })
            .ok_or(ParseKnownError::Usage(
                "portfolio show PORTFOLIO_ID --dir <dir>",
            )),
        "meaning" => meaning_cmd::parse_meaning_request(rest)
            .map(Command::Meaning)
            .map_err(ParseKnownError::Usage),
        "import" => parse_import_modelica(rest)
            .map(|(path, json)| Command::ImportModelica { path, json })
            .ok_or(ParseKnownError::Usage("import modelica <file.mo> [--json]")),
        "artifact" => match rest {
            [sub, dir] if sub == "check" => Ok(Command::ArtifactCheck(PathBuf::from(dir))),
            [sub, dir] if sub == "battery" => Ok(Command::ArtifactBattery(PathBuf::from(dir))),
            _ => Err(ParseKnownError::Usage("artifact check|battery <dir>")),
        },
        "architecture" => {
            if rest.iter().all(|arg| arg.starts_with('-') && arg != "-") {
                Ok(Command::Architecture {
                    json: wants_json(rest),
                })
            } else {
                Err(ParseKnownError::Usage("architecture [--json]"))
            }
        }
        "web" => match crate::serve_cmd::parse_serve_args(rest) {
            Ok(parsed) => Ok(Command::Web(parsed)),
            Err(message) => {
                eprintln!("error: {message}");
                Err(ParseKnownError::Usage(
                    "web [--port N] [--no-open] [--dist PATH]",
                ))
            }
        },
        "serve" => match crate::serve_cmd::parse_serve_args(rest) {
            Ok(parsed) => Ok(Command::Serve(parsed)),
            Err(message) => {
                eprintln!("error: {message}");
                Err(ParseKnownError::Usage(
                    "serve [--port N] [--no-open] [--dist PATH]",
                ))
            }
        },
        "capabilities" => {
            if rest.iter().all(|arg| arg.starts_with('-') && arg != "-") {
                Ok(Command::Capabilities)
            } else {
                Err(ParseKnownError::Usage("capabilities [--json]"))
            }
        }
        "coverage" => Ok(Command::Coverage(rest.to_vec())),
        "robot-docs" => match rest {
            [] => Ok(Command::RobotDocs),
            [guide] if guide == "guide" || guide == "--guide" => Ok(Command::RobotDocs),
            _ => Err(ParseKnownError::Usage("robot-docs [guide]")),
        },
        "bench" => parse_required_path(rest)
            .map(|path| Command::Bench { path })
            .ok_or(ParseKnownError::Usage("bench <file.emath>")),
        "vendor" => parse_vendor_request(rest)
            .map(|out| Command::Vendor { out })
            .ok_or(ParseKnownError::Usage("vendor --out <dir>")),
        "provider" => parse_provider_request(rest)
            .map(Command::Provider)
            .ok_or(ParseKnownError::Usage(
                "provider list|inspect <id>|test <id> [--json]",
            )),
        "fork" => parse_fork_request(rest)
            .map(Command::Fork)
            .ok_or(ParseKnownError::Usage(
                "fork status|sync [--dry-run] [--json]",
            )),
        "agent" => parse_agent_request(rest)
            .map(Command::Agent)
            .ok_or(ParseKnownError::Usage(
                "agent check|plan|build|triage|propose <file> [--out <dir>]",
            )),
        _ => Err(ParseKnownError::Unknown),
    }
}

#[allow(unreachable_code, unused_variables)]
fn run_command(command: Command) -> CliExit {
    if let Some(code) = language_gate(&command) {
        return code;
    }
    match command {
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
        Command::Parse(ParseRequest::Ready { path, out, forest_only }) => {
            genesis_cmd::parse_cmd(&path, out.as_ref(), forest_only)
        }
        Command::Compile(request) => genesis_cmd::compile_cmd(request),
        Command::LibraryMount { name } => crate::library_cmd::mount_cmd(&name),
        Command::Signature(SignatureRequest::Ready { path, out }) => {
            genesis_cmd::signature_cmd(&path, out.as_ref())
        }
        Command::Genesis(GenesisRequest::Ready { path, out }) => {
            genesis_cmd::genesis_cmd(&path, &out)
        }
        Command::Eval(args) => eval_cmd::dispatch_eval(args),
        Command::Sweep(args) => eval_cmd::dispatch_sweep(args),
        Command::Fit(args) => fit_cmd::dispatch_fit(&args),
        Command::Repl { path } => eval_cmd::dispatch_repl(&path),
        Command::WorldShow { id, dir } => genesis_cmd::world_show_cmd(&id, &dir),
        Command::PortfolioShow { id, dir } => genesis_cmd::portfolio_show_cmd(&id, &dir),
        Command::Meaning(request) => meaning_cmd::dispatch(request),
        Command::ImportModelica { path, json } => import_modelica_cmd(&path, json),
        Command::ArtifactCheck(dir) => emath_cli::artifact_check(&dir),
        Command::ArtifactBattery(dir) => artifact_battery(&dir),
        Command::Architecture { json } => architecture(json),
        Command::Web(args) | Command::Serve(args) => crate::serve_cmd::web_cmd(args),
        Command::RobotDocs => {
            print!("{}", catalog::robot_docs_guide());
            EXIT_OK
        }
        Command::Provider(request) => provider_cmd::provider_cmd(request),
        Command::Fork(request) => provider_cmd::fork_cmd(request),
        Command::Capabilities => {
            print!("{}", catalog::capabilities_json());
            EXIT_OK
        }
        Command::Agent(request) => agent_cmd::agent_cmd(request),
        Command::Coverage(rest) => crate::coverage_cmd::coverage_cmd(&rest),
        Command::Vendor { out } => vendor_cmd::vendor_cmd(&out),
        Command::Bench { path } => host_cmd::bench_cmd(&path),
    }
}

fn language_gate(command: &Command) -> Option<CliExit> {
    let (name, json) = match command {
        Command::Eval(args) => ("eval", args.json),
        Command::Sweep(args) => ("sweep", args.json),
        Command::Fit(_) => ("fit", false),
        Command::Solve(_) => ("solve", false),
        Command::Expand(_) => ("expand", false),
        Command::Assumptions(_) => ("assumptions", false),
        Command::Exactness(_) => ("exactness", false),
        Command::Freeze(_) => ("freeze", false),
        Command::Why(_) => ("why", false),
        Command::Parse(_) => ("parse", false),
        Command::Compile(_) => ("compile", false),
        Command::LibraryMount { .. } => ("library", false),
        Command::Signature(_) => ("signature", false),
        Command::Genesis(_) => ("genesis", false),
        Command::Repl { .. } => ("repl", false),
        Command::WorldShow { .. } => ("world", false),
        Command::PortfolioShow { .. } => ("portfolio", false),
        Command::Meaning(_) => ("meaning", false),
        Command::ImportModelica { json, .. } => ("import", *json),
        Command::ArtifactCheck(_) | Command::ArtifactBattery(_) => ("artifact", false),
        Command::Architecture { json } => ("architecture", *json),
        Command::Web(_) => ("web", false),
        Command::Serve(_) => ("serve", false),
        Command::RobotDocs => ("robot-docs", false),
        Command::Provider(_) => ("provider", false),
        Command::Fork(_) => ("fork", false),
        Command::Capabilities => ("capabilities", false),
        Command::Agent(_) => ("agent", false),
        Command::Coverage(_) => ("coverage", false),
        Command::Vendor { .. } => ("vendor", false),
        Command::Bench { .. } => ("bench", false),
    };
    Some(refuse_extracted(name, json))
}

fn refuse_extracted(name: &str, json: bool) -> CliExit {
    refuse_coded(
        name,
        json,
        EXIT_USAGE,
        "E-KIND-GONE",
        &format!(
            "`emath-lab {name}` is not a constructor command. Write an ordinary `emath function` or `emath query` and `emath run`."
        ),
    )
}

fn help_cmd(args: &[String]) -> CliExit {
    match args {
        [] => {
            print!("{}", catalog::help_text());
            EXIT_OK
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print!("{}", catalog::help_text());
            EXIT_OK
        }
        [command] if catalog::is_extracted_token(command) => refuse_extracted(command, false),
        [command] => catalog::print_command_help(command),
        _ => usage("help [<command>]"),
    }
}

fn unknown_command(other: &str) -> CliExit {
    if catalog::is_extracted_token(other) {
        return refuse_extracted(other, false);
    }
    eprintln!("error: unknown command `{other}`");
    if let Some(hint) = emath_cli::catalog::suggest_command(other) {
        eprintln!("did you mean `emath {hint}`?");
        eprintln!("try: emath help {hint}");
    } else {
        eprintln!("try: emath help");
    }
    EXIT_USAGE
}

fn parse_import_modelica(rest: &[String]) -> Option<(PathBuf, bool)> {
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
            other => {
                if path.is_some() {
                    return None;
                }
                path = Some(PathBuf::from(other));
            }
        }
    }
    Some((path?, json))
}

fn parse_required_path(args: &[String]) -> Option<PathBuf> {
    let mut path = None;
    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        if path.is_some() {
            return None;
        }
        path = Some(PathBuf::from(arg));
    }
    path
}

fn parse_vendor_request(args: &[String]) -> Option<PathBuf> {
    let mut out = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" | "-o" => {
                index += 1;
                let value = args.get(index)?;
                if value.starts_with("--") || matches!(value.as_str(), "-o" | "-h" | "-V") {
                    return None;
                }
                if out.is_some() {
                    return None;
                }
                out = Some(PathBuf::from(value));
            }
            other if other.starts_with('-') && other != "-" => return None,
            _ => return None,
        }
        index += 1;
    }
    out
}

fn parse_provider_request(rest: &[String]) -> Option<ProviderRequest> {
    let json = wants_json(rest);
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

fn parse_fork_request(rest: &[String]) -> Option<ForkRequest> {
    let json = wants_json(rest);
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

fn parse_agent_request(args: &[String]) -> Option<AgentRequest> {
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
                index += 1;
                let value = args.get(index)?;
                if value.starts_with("--") || matches!(value.as_str(), "-o" | "-h" | "-V") {
                    return None;
                }
                if out.is_some() {
                    return None;
                }
                out = Some(PathBuf::from(value));
            }
            other if other.starts_with('-') && other != "-" => return None,
            other => {
                if path.is_some() {
                    return None;
                }
                path = Some(PathBuf::from(other));
            }
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
