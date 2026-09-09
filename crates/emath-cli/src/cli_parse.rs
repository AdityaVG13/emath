//! CLI entry (`run`), argument parsing, and the command grammar.

use super::*;

/// Entry used by main; keeps the CLI testable.
pub fn run(args: &[String]) -> CliExit {
    match parse_cli(args) {
        ParsedCli::Empty => {
            print!("{}", help_text());
            EXIT_OK
        }
        ParsedCli::MetaHelp { rest } => help_cmd(rest),
        ParsedCli::MetaVersion { rest } => catalog_read_cmd("version", rest, || {
            println!("{}", catalog::version_text());
            EXIT_OK
        }),
        ParsedCli::MetaCapabilities { rest } => catalog_read_cmd("capabilities", rest, || {
            capabilities::capabilities_cmd(catalog::wants_json(rest))
        }),
        ParsedCli::MetaRobotDocs { rest } => catalog_read_cmd("robot-docs", rest, || {
            if catalog::wants_json(rest) {
                let mut obj = emath_core::JsonWriter::object();
                obj.string("guide", &catalog::robot_docs_guide());
                println!("{}", obj.finish());
            } else {
                print!("{}", catalog::robot_docs_guide());
            }
            EXIT_OK
        }),
        ParsedCli::MetaTriage { target, json } => triage::triage_cmd(target, json),
        ParsedCli::CommandHelp { name } => print_command_help(name),
        ParsedCli::UnknownFlag { code } => {
            if catalog::wants_json(args)
                && matches!(
                    args.first().map(String::as_str),
                    Some("api" | "run" | "step" | "inspect" | "verify")
                )
            {
                execution::diagnostic(
                    true,
                    code,
                    "E-CLI-USAGE",
                    "invalid command arguments; use emath help for the accepted arguments",
                )
            } else {
                code
            }
        }
        ParsedCli::Usage(message) => {
            if catalog::wants_json(args)
                && matches!(
                    args.first().map(String::as_str),
                    Some("api" | "run" | "step" | "inspect" | "verify")
                )
            {
                execution::diagnostic(true, EXIT_USAGE, "E-CLI-USAGE", message)
            } else {
                usage(message)
            }
        }
        ParsedCli::Unknown(name) => unknown_command(name),
        ParsedCli::Known(command) => run_command(command),
    }
}

pub(super) enum ParsedCli<'a> {
    Empty,
    MetaHelp { rest: &'a [String] },
    MetaVersion { rest: &'a [String] },
    MetaCapabilities { rest: &'a [String] },
    MetaRobotDocs { rest: &'a [String] },
    MetaTriage { target: Option<PathBuf>, json: bool },
    CommandHelp { name: &'a str },
    UnknownFlag { code: CliExit },
    Usage(&'static str),
    Known(Command),
    Unknown(&'a str),
}

pub(super) enum Command {
    Check(FileJsonRequest),
    Plan(FileJsonRequest),
    Planner(PlannerRequest),
    Build(BuildRequest),
    Simulate(simulate_cmd::SimulateArgs),
    New {
        name: String,
        out: PathBuf,
    },
    Fmt {
        path: Option<PathBuf>,
        value: Option<String>,
        sf: Option<u32>,
        from: Option<String>,
        format: Option<String>,
    },
    Migrate {
        path: PathBuf,
        fix: bool,
        check_only: bool,
        receipt: Option<PathBuf>,
        list_rules: bool,
    },
    Explain(ExplainRequest),
    Run(execution::RunRequest),
    Search(compiled_search::SearchRequest),
    Step(execution::RunRequest),
    Api(language_cmd::ApiRequest),
    Test {
        path: PathBuf,
        out: PathBuf,
    },
    Verify {
        dir: PathBuf,
        json: bool,
    },
    Inspect {
        dir: PathBuf,
        json: bool,
    },
    Diff {
        a: PathBuf,
        b: PathBuf,
        json: bool,
    },
    Doctor {
        json: bool,
    },
}

pub(crate) enum ExplainRequest {
    File {
        path: PathBuf,
        symbol: Option<String>,
        provenance: bool,
        json: bool,
        show_defaults: bool,
    },
    Law {
        json: bool,
    },
}

pub(super) enum ParseKnownError {
    Usage(&'static str),
    Unknown,
}

pub(super) fn parse_cli(args: &[String]) -> ParsedCli<'_> {
    let Some(first) = args.first() else {
        return ParsedCli::Empty;
    };
    let rest = &args[1..];
    match first.as_str() {
        "help" | "--help" | "-h" => return ParsedCli::MetaHelp { rest },
        "version" | "--version" | "-V" => return ParsedCli::MetaVersion { rest },
        "capabilities" | "--capabilities" => return ParsedCli::MetaCapabilities { rest },
        "robot-docs" | "--robot-help" => return ParsedCli::MetaRobotDocs { rest },
        "triage" | "--robot-triage" => {
            if catalog::wants_help(rest) {
                return ParsedCli::CommandHelp { name: "triage" };
            }
            if let Some(code) = catalog::reject_unknown_flags("triage", rest) {
                return ParsedCli::UnknownFlag { code };
            }
            let json = catalog::wants_json(rest) || first == "--robot-triage";
            let mut target = None;
            for arg in rest {
                if !arg.starts_with('-') {
                    if target.is_some() {
                        return ParsedCli::Usage("triage accepts at most one target file");
                    }
                    target = Some(PathBuf::from(arg));
                }
            }
            return ParsedCli::MetaTriage { target, json };
        }
        _ => {}
    }
    if catalog::wants_help(rest) {
        return ParsedCli::CommandHelp { name: first };
    }
    if let Some(code) = catalog::reject_unknown_flags(first, rest) {
        return ParsedCli::UnknownFlag { code };
    }
    match parse_known(first.as_str(), rest) {
        Ok(command) => ParsedCli::Known(command),
        Err(ParseKnownError::Usage(message)) => ParsedCli::Usage(message),
        Err(ParseKnownError::Unknown) => ParsedCli::Unknown(first),
    }
}

pub(super) fn parse_known(name: &str, rest: &[String]) -> Result<Command, ParseKnownError> {
    match name {
        "check" => parse_check_request(rest)
            .map(Command::Check)
            .ok_or(ParseKnownError::Usage(
                "check <file.emath> [--verify-data] [--json]",
            )),
        "plan" => parse_file_json_request(rest)
            .map(Command::Plan)
            .ok_or(ParseKnownError::Usage("plan <file.emath> [--json]")),
        "planner" => {
            parse_planner_request(rest)
                .map(Command::Planner)
                .ok_or(ParseKnownError::Usage(
                    "planner <file.emath> [--json] [--parametric]",
                ))
        }
        "build" => parse_build_request(rest)
            .map(Command::Build)
            .ok_or(ParseKnownError::Usage(
                "build <file.emath> [--out <dir>] [--verify] [--json]",
            )),
        "simulate" => match simulate_cmd::parse_simulate_args(rest) {
            Ok(parsed) => Ok(Command::Simulate(parsed)),
            Err(message) => {
                eprintln!("error: {message}");
                Err(ParseKnownError::Usage(
                    "simulate <file.emath> [--model NAME] [--dt N] [--t0 N] [--t1 N] [--method euler|rk4|rk45|backward-euler|velocity-verlet] [--atol N] [--rtol N] [--dt-max N] [--event name=value] [--set name=value] [--json]",
                ))
            }
        },
        "new" => parse_new_request(rest)
            .map(|(name, out)| Command::New { name, out })
            .ok_or(ParseKnownError::Usage("new <name> [--out <dir>]")),
        "fmt" => parse_fmt_request(rest),
        "migrate" => parse_migrate_request(rest),
        "explain" => {
            parse_explain_request(rest)
                .map(Command::Explain)
                .ok_or(ParseKnownError::Usage(
                    "explain <file.emath> [<symbol>] [--provenance] [--show-defaults] | explain \
                     E-LAW-001 [--json]",
                ))
        }
        "api" => language_cmd::ApiRequest::parse(rest)
            .map(Command::Api)
            .ok_or(ParseKnownError::Usage("api [--search text] [--offset N] [--limit N] [--source file.emath] [--json]")),
        "search" => compiled_search::SearchRequest::parse(rest)
            .map(Command::Search)
            .ok_or(ParseKnownError::Usage(compiled_search::USAGE)),
        "run" => execution::RunRequest::parse(rest, false)
            .map(Command::Run)
            .ok_or(ParseKnownError::Usage("run <file.emath> [--function NAME] [--set name=value] [--work N] [--cancel-file path] [--measure N] [--branch-from checkpoint --relation relation] [--out dir] [--json]")),
        "step" => execution::RunRequest::parse(rest, true)
            .map(Command::Step)
            .ok_or(ParseKnownError::Usage("step <checkpoint.json> [--work N] [--expect-revision N] [--cancel-file path] [--out dir] [--json]")),
        "test" => parse_path_out_request(rest)
            .map(|(path, out)| Command::Test { path, out })
            .ok_or(ParseKnownError::Usage("test <file.emath> [--out <dir>]")),
        "verify" => parse_required_path(rest)
            .map(|dir| Command::Verify { dir, json: catalog::wants_json(rest) })
            .ok_or(ParseKnownError::Usage("verify <artifact-dir> | verify <checkpoint.json> [--json]")), 
        "inspect" => parse_inspect_request(rest)
            .map(|(dir, json)| Command::Inspect { dir, json })
            .ok_or(ParseKnownError::Usage("inspect <artifact-dir> [--json]")),
        "diff" => parse_diff_request(rest)
            .map(|(a, b, json)| Command::Diff { a, b, json })
            .ok_or(ParseKnownError::Usage("diff <a.emath> <b.emath> [--json]")),
        "doctor" => {
            if no_extra_positionals(rest) {
                Ok(Command::Doctor {
                    json: catalog::wants_json(rest),
                })
            } else {
                Err(ParseKnownError::Usage("doctor [--json]"))
            }
        }
        _ => Err(ParseKnownError::Unknown),
    }
}

/// `fmt [<file.emath>]` or value mode:
/// `fmt --value <literal> [--sf N] [--from UNIT] [--format "0.1 %"|preferred_unit UNIT]`
pub(super) fn parse_fmt_request(rest: &[String]) -> Result<Command, ParseKnownError> {
    const USAGE: &str = "fmt [<file.emath>] | fmt --value <literal> \
                         [--sf N] [--from UNIT] [--format \"0.1 %\"|preferred_unit UNIT]";
    let mut path: Option<PathBuf> = None;
    let mut value: Option<String> = None;
    let mut sf: Option<u32> = None;
    let mut from: Option<String> = None;
    let mut format: Option<String> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--value" => {
                i += 1;
                value = rest.get(i).map(|s| s.to_string());
                if value.is_none() {
                    return Err(ParseKnownError::Usage(USAGE));
                }
            }
            "--sf" => {
                i += 1;
                match rest.get(i).and_then(|s| s.parse::<u32>().ok()) {
                    Some(n) => sf = Some(n),
                    None => return Err(ParseKnownError::Usage(USAGE)),
                }
            }
            "--from" => {
                i += 1;
                from = rest.get(i).map(|s| s.to_string());
                if from.is_none() {
                    return Err(ParseKnownError::Usage(USAGE));
                }
            }
            "--format" => {
                i += 1;
                if rest.get(i).is_none() {
                    return Err(ParseKnownError::Usage(USAGE));
                }
                format = Some(rest[i..].join(" "));
                break;
            }
            other if !other.starts_with('-') && path.is_none() && value.is_none() => {
                path = Some(PathBuf::from(other));
            }
            _ => return Err(ParseKnownError::Usage(USAGE)),
        }
        i += 1;
    }
    // Exactly one of file mode or value mode.
    if value.is_some() == path.is_some() {
        return Err(ParseKnownError::Usage(USAGE));
    }
    Ok(Command::Fmt {
        path,
        value,
        sf,
        from,
        format,
    })
}

/// `migrate <file.emath> [--fix] [--check] [--receipt <path>] | migrate --list-rules`
/// (05 §5, / ). Lossless
/// rewrites only; the receipt is the canonical stable-JSON artifact.
pub(super) fn parse_migrate_request(rest: &[String]) -> Result<Command, ParseKnownError> {
    const USAGE: &str = "migrate <file.emath> [--fix] [--check] [--receipt <path>] | \
                         migrate --list-rules";
    if matches!(rest, [flag] if flag == "--list-rules") {
        return Ok(Command::Migrate {
            path: PathBuf::new(),
            fix: false,
            check_only: false,
            receipt: None,
            list_rules: true,
        });
    }
    let mut path: Option<PathBuf> = None;
    let mut fix = false;
    let mut check_only = false;
    let mut receipt: Option<PathBuf> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--fix" => fix = true,
            "--check" => check_only = true,
            "--receipt" => {
                i += 1;
                receipt = rest.get(i).map(PathBuf::from);
                if receipt.is_none() {
                    return Err(ParseKnownError::Usage(USAGE));
                }
            }
            other if !other.starts_with('-') && path.is_none() => {
                path = Some(PathBuf::from(other));
            }
            _ => return Err(ParseKnownError::Usage(USAGE)),
        }
        i += 1;
    }
    let Some(path) = path else {
        return Err(ParseKnownError::Usage(USAGE));
    };
    if check_only && fix {
        return Err(ParseKnownError::Usage(
            "migrate: --check and --fix are mutually exclusive",
        ));
    }
    Ok(Command::Migrate {
        path,
        fix,
        check_only,
        receipt,
        list_rules: false,
    })
}
