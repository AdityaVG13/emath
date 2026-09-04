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
        ParsedCli::CommandHelp { name } => print_command_help(name),
        ParsedCli::UnknownFlag { code } => code,
        ParsedCli::Usage(message) => usage(message),
        ParsedCli::Unknown(name) => unknown_command(name),
        ParsedCli::Known(command) => run_command(command),
    }
}

pub(super) enum ParsedCli<'a> {
    Empty,
    MetaHelp { rest: &'a [String] },
    MetaVersion { rest: &'a [String] },
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
    Expand(FileJsonRequest),
    Assumptions(FileJsonRequest),
    Solve(ParsedSolve),
    Exactness(ExactnessRequest),
    Freeze(FreezeRequest),
    Why(WhyRequest),
    Parse(ParseRequest),
    Signature(SignatureRequest),
    Genesis(GenesisRequest),
    Compile(CompileRequest),
    RobotDocs,
    Provider(ProviderRequest),
    Fork(ForkRequest),
    Capabilities,
    Eval(eval_cmd::EvalArgs),
    Sweep(eval_cmd::SweepArgs),
    Simulate(simulate_cmd::SimulateArgs),
    Fit(fit_cmd::FitArgs),
    Repl {
        path: PathBuf,
    },
    WorldShow {
        id: String,
        dir: PathBuf,
    },
    PortfolioShow {
        id: String,
        dir: PathBuf,
    },
    Meaning(meaning_cmd::MeaningRequest),
    LibraryMount {
        name: String,
    },
    ImportModelica {
        path: PathBuf,
        json: bool,
    },
    ArtifactCheck(PathBuf),
    ArtifactBattery(PathBuf),
    Architecture {
        json: bool,
    },
    Web(serve_cmd::ServeArgs),
    Serve(serve_cmd::ServeArgs),
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
    Run {
        path: PathBuf,
        out: PathBuf,
    },
    Test {
        path: PathBuf,
        out: PathBuf,
    },
    Bench {
        path: PathBuf,
    },
    Verify {
        dir: PathBuf,
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
    Vendor {
        out: PathBuf,
    },
    Agent(AgentRequest),
    Coverage(Vec<String>),
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

pub(crate) enum AgentRequest {
    Check { path: PathBuf },
    Plan { path: PathBuf },
    Build { path: PathBuf, out: PathBuf },
    Triage { path: PathBuf },
    Propose { path: PathBuf },
}

pub(crate) enum ProviderRequest {
    List { json: bool },
    Inspect { id: String },
    Test { id: String, json: bool },
}

pub(crate) enum ForkRequest {
    Status { json: bool },
    Sync { dry_run: bool, json: bool },
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
        "parse" => parse_parse_request(rest)
            .map(Command::Parse)
            .ok_or(ParseKnownError::Usage(
                "parse --forest <file.emath> [--out <dir>]",
            )),
        "expand" => parse_file_json_request(rest)
            .map(Command::Expand)
            .ok_or(ParseKnownError::Usage("expand <file.emath> [--json]")),
        "solve" => Ok(Command::Solve(parse_solve_request(rest))),
        "exactness" => {
            parse_exactness_request(rest)
                .map(Command::Exactness)
                .ok_or(ParseKnownError::Usage(
                    "exactness <file.emath> [--json] [--raise units]",
                ))
        }
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
        "signature" => {
            parse_signature_request(rest)
                .map(Command::Signature)
                .ok_or(ParseKnownError::Usage(
                    "signature <file.emath> [--out <dir>]",
                ))
        }
        "genesis" => parse_genesis_request(rest)
            .map(Command::Genesis)
            .ok_or(ParseKnownError::Usage("genesis <file.emath> --out <dir>")),
        "eval" => eval_cmd::parse_eval_args(rest)
            .map(Command::Eval)
            .ok_or(ParseKnownError::Usage(
                "eval <file.emath> [--world <name>] [--function NAME] [--set name=value] [--json]",
            )),
        "sweep" => eval_cmd::parse_sweep_args(rest)
            .map(Command::Sweep)
            .ok_or(ParseKnownError::Usage(
                "sweep <file.emath> --function NAME --grid name=v1,v2,... [--expect name=value] [--out <file>] [--json]",
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
        "compile" => {
            parse_compile_request(rest)
                .map(Command::Compile)
                .ok_or(ParseKnownError::Usage(
                    "compile --parametric <file.emath> --out <dir> [--world LABEL]",
                ))
        }
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
        "library" => match rest {
            [sub, name] if sub == "mount" => Ok(Command::LibraryMount {
                name: name.clone(),
            }),
            _ => Err(ParseKnownError::Usage("library mount <name>")),
        },
        "import" => parse_import_modelica(rest)
            .map(|(path, json)| Command::ImportModelica { path, json })
            .ok_or(ParseKnownError::Usage("import modelica <file.mo> [--json]")),
        "artifact" => match rest {
            [sub, dir] if sub == "check" => Ok(Command::ArtifactCheck(PathBuf::from(dir))),
            [sub, dir] if sub == "battery" => Ok(Command::ArtifactBattery(PathBuf::from(dir))),
            _ => Err(ParseKnownError::Usage("artifact check|battery <dir>")),
        },
        "architecture" => {
            if no_extra_positionals(rest) {
                Ok(Command::Architecture {
                    json: catalog::wants_json(rest),
                })
            } else {
                Err(ParseKnownError::Usage("architecture [--json]"))
            }
        }
        "web" => match serve_cmd::parse_serve_args(rest) {
            Ok(parsed) => Ok(Command::Web(parsed)),
            Err(message) => {
                eprintln!("error: {message}");
                Err(ParseKnownError::Usage(
                    "web [--port N] [--no-open] [--dist PATH]",
                ))
            }
        },
        "serve" => match serve_cmd::parse_serve_args(rest) {
            Ok(parsed) => Ok(Command::Serve(parsed)),
            Err(message) => {
                eprintln!("error: {message}");
                Err(ParseKnownError::Usage(
                    "serve [--port N] [--no-open] [--dist PATH]",
                ))
            }
        },
        "capabilities" => {
            if no_extra_positionals(rest) {
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
        "run" => parse_path_out_request(rest)
            .map(|(path, out)| Command::Run { path, out })
            .ok_or(ParseKnownError::Usage("run <file.emath> [--out <dir>]")),
        "test" => parse_path_out_request(rest)
            .map(|(path, out)| Command::Test { path, out })
            .ok_or(ParseKnownError::Usage("test <file.emath> [--out <dir>]")),
        "bench" => parse_required_path(rest)
            .map(|path| Command::Bench { path })
            .ok_or(ParseKnownError::Usage("bench <file.emath>")),
        "verify" => parse_required_path(rest)
            .map(|dir| Command::Verify { dir })
            .ok_or(ParseKnownError::Usage("verify <artifact-dir>")),
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
        "vendor" => parse_vendor_request(rest)
            .map(|out| Command::Vendor { out })
            .ok_or(ParseKnownError::Usage("vendor --out <dir>")),
        "provider" => {
            parse_provider_request(rest)
                .map(Command::Provider)
                .ok_or(ParseKnownError::Usage(
                    "provider list|inspect <id>|test <id> [--json]",
                ))
        }
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
