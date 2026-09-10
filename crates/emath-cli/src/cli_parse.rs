//! CLI entry (`run`), argument parsing, and the command grammar.

use super::*;

/// Entry used by main; keeps the CLI testable.
pub fn run(args: &[String]) -> CliExit {
    let cleaned_args = match terminal::extract_color_flags(args) {
        Ok(c) => c,
        Err(err) => return err.emit(catalog::wants_json(args)),
    };
    match parse_cli(&cleaned_args) {
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
        ParsedCli::MetaCatalog { rest } => catalog_read_cmd("catalog", rest, || {
            catalog::catalog_cmd(catalog::wants_json(rest))
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
        ParsedCli::MetaNext { target, json } => triage::next_cmd(target, json),
        ParsedCli::CommandHelp { name, json } => print_command_help(name, json),
        ParsedCli::UnknownFlag { code } => code,
        ParsedCli::Pedagogic(err) => err.emit(catalog::wants_json(&cleaned_args)),
        ParsedCli::Usage(message) => {
            let cmd = cleaned_args.first().map(String::as_str).unwrap_or("help");
            let canonical = catalog::resolve_alias(cmd).unwrap_or(cmd);
            let err = PedagogicError::new(
                "E-CLI-USAGE",
                format!("invalid or missing arguments for `emath {canonical}`"),
                format!("arguments for `emath {canonical}`"),
                format!("emath {message}"),
            )
            .with_command(canonical)
            .with_usage(format!("emath {message}"));
            err.emit(catalog::wants_json(&cleaned_args))
        }
        ParsedCli::Unknown(name) => unknown_command(name, catalog::wants_json(&cleaned_args)),
        ParsedCli::Known(command) => run_command(command),
    }
}

pub(super) enum ParsedCli<'a> {
    Empty,
    MetaHelp { rest: &'a [String] },
    MetaVersion { rest: &'a [String] },
    MetaCapabilities { rest: &'a [String] },
    MetaCatalog { rest: &'a [String] },
    MetaRobotDocs { rest: &'a [String] },
    MetaTriage { target: Option<PathBuf>, json: bool },
    MetaNext { target: Option<PathBuf>, json: bool },
    CommandHelp { name: &'a str, json: bool },
    UnknownFlag { code: CliExit },
    Pedagogic(PedagogicError),
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
        dry_run: bool,
        force: bool,
        json: bool,
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
        dry_run: bool,
        receipt: Option<PathBuf>,
        list_rules: bool,
        json: bool,
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
    Code {
        code: Option<String>,
        json: bool,
    },
}

pub(super) enum ParseKnownError {
    Pedagogic(PedagogicError),
    Usage(&'static str),
    Unknown,
}

pub(super) fn parse_cli(args: &[String]) -> ParsedCli<'_> {
    let Some(first) = args.first() else {
        return ParsedCli::Empty;
    };
    let rest = &args[1..];
    let canonical = catalog::resolve_alias(first.as_str()).unwrap_or(first.as_str());
    match canonical {
        "help" | "--help" | "-h" => return ParsedCli::MetaHelp { rest },
        "version" | "--version" | "-V" => return ParsedCli::MetaVersion { rest },
        "capabilities" | "--capabilities" => return ParsedCli::MetaCapabilities { rest },
        "catalog" => return ParsedCli::MetaCatalog { rest },
        "robot-docs" | "--robot-help" => return ParsedCli::MetaRobotDocs { rest },
        "triage" | "--robot-triage" => {
            if catalog::wants_help(rest) {
                return ParsedCli::CommandHelp {
                    name: "triage",
                    json: catalog::wants_json(rest),
                };
            }
            if let Some(code) = catalog::reject_unknown_flags("triage", rest) {
                return ParsedCli::UnknownFlag { code };
            }
            let json = catalog::wants_json(rest) || first == "--robot-triage";
            let mut target = None;
            for arg in rest {
                if !arg.starts_with('-') {
                    if target.is_some() {
                        return ParsedCli::Pedagogic(
                            PedagogicError::new(
                                "E-CLI-USAGE",
                                "triage accepts at most one target file",
                                "arguments for `emath triage`",
                                "emath triage [<file.emath>] [--json]",
                            )
                            .with_command("triage")
                            .with_usage("emath triage [<file.emath>] [--json]"),
                        );
                    }
                    target = Some(PathBuf::from(arg));
                }
            }
            return ParsedCli::MetaTriage { target, json };
        }
        "next" | "--robot-next" => {
            if catalog::wants_help(rest) {
                return ParsedCli::CommandHelp {
                    name: "next",
                    json: catalog::wants_json(rest),
                };
            }
            if let Some(code) = catalog::reject_unknown_flags("next", rest) {
                return ParsedCli::UnknownFlag { code };
            }
            let json = catalog::wants_json(rest) || first == "--robot-next";
            let mut target = None;
            for arg in rest {
                if !arg.starts_with('-') {
                    if target.is_some() {
                        return ParsedCli::Pedagogic(
                            PedagogicError::new(
                                "E-CLI-USAGE",
                                "next accepts at most one target file",
                                "arguments for `emath next`",
                                "emath next [<file.emath>] [--json]",
                            )
                            .with_command("next")
                            .with_usage("emath next [<file.emath>] [--json]"),
                        );
                    }
                    target = Some(PathBuf::from(arg));
                }
            }
            return ParsedCli::MetaNext { target, json };
        }
        _ => {}
    }
    if catalog::wants_help(rest) {
        return ParsedCli::CommandHelp {
            name: canonical,
            json: catalog::wants_json(rest),
        };
    }
    if !catalog::is_known_command(canonical) {
        return ParsedCli::Unknown(first);
    }
    if let Some(code) = catalog::reject_unknown_flags(canonical, rest) {
        return ParsedCli::UnknownFlag { code };
    }
    match parse_known(canonical, rest) {
        Ok(command) => ParsedCli::Known(command),
        Err(ParseKnownError::Pedagogic(err)) => ParsedCli::Pedagogic(err),
        Err(ParseKnownError::Usage(message)) => ParsedCli::Usage(message),
        Err(ParseKnownError::Unknown) => ParsedCli::Unknown(first),
    }
}

fn require_single_file<T>(
    cmd: &'static str,
    usage: &'static str,
    rest: &[String],
    f: impl FnOnce(&[String]) -> Option<T>,
) -> Result<T, ParseKnownError> {
    if let Some(val) = f(rest) {
        return Ok(val);
    }
    let positionals: Vec<&str> = rest
        .iter()
        .filter(|arg| !arg.starts_with('-') || *arg == "-")
        .map(String::as_str)
        .collect();
    if positionals.is_empty() {
        Err(ParseKnownError::Pedagogic(
            PedagogicError::new(
                "E-CLI-USAGE",
                format!("missing required argument `<file.emath>` for `emath {cmd}`"),
                "positional argument 1 (expected path to `.emath` source file)",
                format!("emath {cmd} <file.emath>"),
            )
            .with_command(cmd)
            .with_usage(format!("emath {usage}")),
        ))
    } else if positionals.len() > 1 {
        Err(ParseKnownError::Pedagogic(
            PedagogicError::new(
                "E-CLI-USAGE",
                format!(
                    "unexpected positional argument `{}` for `emath {cmd}`",
                    positionals[1]
                ),
                format!(
                    "argument `{}` (expected exactly 1 `.emath` source file)",
                    positionals[1]
                ),
                format!("emath {cmd} {}", positionals[0]),
            )
            .with_command(cmd)
            .with_usage(format!("emath {usage}")),
        ))
    } else {
        Err(ParseKnownError::Usage(usage))
    }
}

pub(super) fn parse_known(name: &str, rest: &[String]) -> Result<Command, ParseKnownError> {
    match name {
        "check" => require_single_file(
            "check",
            "check <file.emath> [--verify-data] [--json]",
            rest,
            parse_check_request,
        )
        .map(Command::Check),
        "plan" => require_single_file(
            "plan",
            "plan <file.emath> [--json]",
            rest,
            parse_file_json_request,
        )
        .map(Command::Plan),
        "planner" => require_single_file(
            "planner",
            "planner <file.emath> [--json] [--parametric]",
            rest,
            parse_planner_request,
        )
        .map(Command::Planner),
        "build" => require_single_file(
            "build",
            "build <file.emath> [--out <dir>] [--verify] [--bin <entrypoint>] [--dry-run] [--json]",
            rest,
            parse_build_request,
        )
        .map(Command::Build),
        "simulate" => match simulate_cmd::parse_simulate_args(rest) {
            Ok(parsed) => Ok(Command::Simulate(parsed)),
            Err(message) => {
                let positionals: Vec<&str> = rest
                    .iter()
                    .filter(|arg| !arg.starts_with('-') || *arg == "-")
                    .map(String::as_str)
                    .collect();
                if positionals.is_empty() {
                    Err(ParseKnownError::Pedagogic(
                        PedagogicError::new(
                            "E-CLI-USAGE",
                            "missing required argument `<file.emath>` for `emath simulate`",
                            "positional argument 1 (expected path to `.emath` source file)",
                            "emath simulate <file.emath> [--method rk4] [--json]",
                        )
                        .with_command("simulate")
                        .with_usage("emath simulate <file.emath> [--model NAME] [--dt N] [--t0 N] [--t1 N] [--method euler|rk4|rk45|backward-euler|velocity-verlet] [--atol N] [--rtol N] [--dt-max N] [--event name=value] [--set name=value] [--json]"),
                    ))
                } else {
                    Err(ParseKnownError::Pedagogic(
                        PedagogicError::new(
                            "E-CLI-USAGE",
                            format!("invalid simulation argument: {message}"),
                            "arguments for `emath simulate`",
                            "emath simulate <file.emath> [--method rk4] [--json]",
                        )
                        .with_command("simulate")
                        .with_usage("emath simulate <file.emath> [--model NAME] [--dt N] [--t0 N] [--t1 N] [--method euler|rk4|rk45|backward-euler|velocity-verlet] [--atol N] [--rtol N] [--dt-max N] [--event name=value] [--set name=value] [--json]"),
                    ))
                }
            }
        },
        "new" => match parse_new_request(rest) {
            Some((name, out, dry_run, force, json)) => Ok(Command::New {
                name,
                out,
                dry_run,
                force,
                json,
            }),
            None => Err(ParseKnownError::Pedagogic(
                PedagogicError::new(
                    "E-CLI-USAGE",
                    "missing required argument `<name>` for `emath new`",
                    "positional argument 1 (expected model name)",
                    "emath new <model_name> [--out <dir>] [--dry-run] [--force] [--json]",
                )
                .with_command("new")
                .with_usage("emath new <name> [--out <dir>] [--dry-run] [--force] [--json]"),
            )),
        },
        "fmt" => parse_fmt_request(rest),
        "migrate" => parse_migrate_request(rest),
        "explain" => match parse_explain_request(rest) {
            Some(req) => Ok(Command::Explain(req)),
            None => Err(ParseKnownError::Pedagogic(
                PedagogicError::new(
                    "E-CLI-USAGE",
                    "missing target for `emath explain`",
                    "positional argument 1 (expected `.emath` file or diagnostic code such as `E-LAW-001`)",
                    "emath explain <file.emath> [<symbol>] or emath explain E-LAW-001 [--json]",
                )
                .with_command("explain")
                .with_usage("emath explain <file.emath> [<symbol>] [--provenance] [--show-defaults] | explain E-LAW-001 [--json]"),
            )),
        },
        "api" => match language_cmd::ApiRequest::parse(rest) {
            Some(req) => Ok(Command::Api(req)),
            None => Err(ParseKnownError::Pedagogic(
                PedagogicError::new(
                    "E-CLI-USAGE",
                    "invalid arguments for `emath api`",
                    "arguments for `emath api`",
                    "emath api [--search text] [--offset N] [--limit N] [--source file.emath] [--json]",
                )
                .with_command("api")
                .with_usage("emath api [--search text] [--offset N] [--limit N] [--source file.emath] [--json]"),
            )),
        },
        "search" => match compiled_search::SearchRequest::parse(rest) {
            Some(req) => Ok(Command::Search(req)),
            None => Err(ParseKnownError::Pedagogic(
                PedagogicError::new(
                    "E-CLI-USAGE",
                    "invalid arguments for `emath search`",
                    "arguments for `emath search`",
                    "emath search --function <name> [--candidate <name>] [--out <dir>] [--json]",
                )
                .with_command("search")
                .with_usage(format!("emath {}", compiled_search::USAGE)),
            )),
        },
        "run" => require_single_file(
            "run",
            "run <file.emath> [--function NAME] [--set name=value] [--work N] [--cancel-file path] [--measure N] [--branch-from checkpoint --relation relation] [--out dir] [--json]",
            rest,
            |r| execution::RunRequest::parse(r, false),
        )
        .map(Command::Run),
        "step" => match execution::RunRequest::parse(rest, true) {
            Some(req) => Ok(Command::Step(req)),
            None => Err(ParseKnownError::Pedagogic(
                PedagogicError::new(
                    "E-CLI-USAGE",
                    "missing required argument `<checkpoint.json>` for `emath step`",
                    "positional argument 1 (expected path to checkpoint JSON file)",
                    "emath step <checkpoint.json> [--work N] [--out <dir>] [--json]",
                )
                .with_command("step")
                .with_usage("emath step <checkpoint.json> [--work N] [--expect-revision N] [--cancel-file path] [--out dir] [--json]"),
            )),
        },
        "test" => require_single_file(
            "test",
            "test <file.emath> [--out <dir>]",
            rest,
            |r| parse_path_out_request(r).map(|(path, out)| (path, out)),
        )
        .map(|(path, out)| Command::Test { path, out }),
        "verify" => match parse_required_path(rest) {
            Some(dir) => Ok(Command::Verify {
                dir,
                json: catalog::wants_json(rest),
            }),
            None => Err(ParseKnownError::Pedagogic(
                PedagogicError::new(
                    "E-CLI-USAGE",
                    "missing required argument `<artifact-dir>` for `emath verify`",
                    "positional argument 1 (expected path to artifact directory or checkpoint.json)",
                    "emath verify target/emath [--json]",
                )
                .with_command("verify")
                .with_usage("emath verify <artifact-dir> | verify <checkpoint.json> [--json]"),
            )),
        },
        "inspect" => match parse_inspect_request(rest) {
            Some((dir, json)) => Ok(Command::Inspect { dir, json }),
            None => Err(ParseKnownError::Pedagogic(
                PedagogicError::new(
                    "E-CLI-USAGE",
                    "missing required argument `<artifact-dir>` for `emath inspect`",
                    "positional argument 1 (expected path to artifact directory or checkpoint.json)",
                    "emath inspect target/emath [--json]",
                )
                .with_command("inspect")
                .with_usage("emath inspect <artifact-dir> [--json]"),
            )),
        },
        "diff" => match parse_diff_request(rest) {
            Some((a, b, json)) => Ok(Command::Diff { a, b, json }),
            None => {
                let positionals: Vec<&str> = rest
                    .iter()
                    .filter(|arg| !arg.starts_with('-') || *arg == "-")
                    .map(String::as_str)
                    .collect();
                let err = match positionals.len() {
                    0 => PedagogicError::new(
                        "E-CLI-USAGE",
                        "missing required arguments `<a.emath>` and `<b.emath>` for `emath diff`",
                        "positional arguments 1 and 2 (expected two file paths to compare)",
                        "emath diff <a.emath> <b.emath> [--json]",
                    ),
                    1 => PedagogicError::new(
                        "E-CLI-USAGE",
                        "missing second comparison file `<b.emath>` for `emath diff`",
                        "positional argument 2 (expected second file path)",
                        format!("emath diff {} <b.emath> [--json]", positionals[0]),
                    ),
                    _ => PedagogicError::new(
                        "E-CLI-USAGE",
                        format!(
                            "unexpected extra positional argument `{}` for `emath diff`",
                            positionals[2]
                        ),
                        "positional arguments (expected exactly two files to compare)",
                        format!("emath diff {} {} [--json]", positionals[0], positionals[1]),
                    ),
                };
                Err(ParseKnownError::Pedagogic(
                    err.with_command("diff")
                        .with_usage("emath diff <a.emath> <b.emath> [--json]"),
                ))
            }
        },
        "doctor" => {
            if no_extra_positionals(rest) {
                Ok(Command::Doctor {
                    json: catalog::wants_json(rest),
                })
            } else {
                Err(ParseKnownError::Pedagogic(
                    PedagogicError::new(
                        "E-CLI-USAGE",
                        "unexpected positional arguments for `emath doctor`",
                        "positional arguments (`emath doctor` accepts only flags)",
                        "emath doctor [--json]",
                    )
                    .with_command("doctor")
                    .with_usage("emath doctor [--json]"),
                ))
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
                    return Err(ParseKnownError::Pedagogic(
                        PedagogicError::new(
                            "E-CLI-MISSING-VALUE",
                            "flag `--value` requires a literal value",
                            "flag `--value` in `emath fmt`",
                            "emath fmt --value 3.14159 [--sf 3]",
                        )
                        .with_command("fmt")
                        .with_flag("--value")
                        .with_usage(format!("emath {USAGE}")),
                    ));
                }
            }
            "--sf" => {
                i += 1;
                match rest.get(i).and_then(|s| s.parse::<u32>().ok()) {
                    Some(n) => sf = Some(n),
                    None => {
                        return Err(ParseKnownError::Pedagogic(
                            PedagogicError::new(
                                "E-CLI-MISSING-VALUE",
                                "flag `--sf` requires a positive integer",
                                "flag `--sf` in `emath fmt`",
                                "emath fmt --value 3.14159 --sf 3",
                            )
                            .with_command("fmt")
                            .with_flag("--sf")
                            .with_usage(format!("emath {USAGE}")),
                        ));
                    }
                }
            }
            "--from" => {
                i += 1;
                from = rest.get(i).map(|s| s.to_string());
                if from.is_none() {
                    return Err(ParseKnownError::Pedagogic(
                        PedagogicError::new(
                            "E-CLI-MISSING-VALUE",
                            "flag `--from` requires a unit name",
                            "flag `--from` in `emath fmt`",
                            "emath fmt --value 100 --from m",
                        )
                        .with_command("fmt")
                        .with_flag("--from")
                        .with_usage(format!("emath {USAGE}")),
                    ));
                }
            }
            "--format" => {
                i += 1;
                if rest.get(i).is_none() {
                    return Err(ParseKnownError::Pedagogic(
                        PedagogicError::new(
                            "E-CLI-MISSING-VALUE",
                            "flag `--format` requires a format string",
                            "flag `--format` in `emath fmt`",
                            "emath fmt --value 0.5 --format \"0.1 %\"",
                        )
                        .with_command("fmt")
                        .with_flag("--format")
                        .with_usage(format!("emath {USAGE}")),
                    ));
                }
                format = Some(rest[i..].join(" "));
                break;
            }
            other if (other == "-" || !other.starts_with('-'))
                && path.is_none()
                && value.is_none() =>
            {
                path = Some(PathBuf::from(other));
            }
            _ => {
                return Err(ParseKnownError::Pedagogic(
                    PedagogicError::new(
                        "E-CLI-USAGE",
                        "invalid arguments for `emath fmt`",
                        "arguments for `emath fmt`",
                        "emath fmt <file.emath> or emath fmt --value <literal> [--sf N]",
                    )
                    .with_command("fmt")
                    .with_usage(format!("emath {USAGE}")),
                ));
            }
        }
        i += 1;
    }
    // Exactly one of file mode or value mode.
    if value.is_some() == path.is_some() {
        return Err(ParseKnownError::Pedagogic(
            PedagogicError::new(
                "E-CLI-USAGE",
                "fmt requires either a file path or `--value <literal>` (not both or neither)",
                "arguments for `emath fmt`",
                "emath fmt <file.emath> or emath fmt --value 3.14159 [--sf 3]",
            )
            .with_command("fmt")
            .with_usage(format!("emath {USAGE}")),
        ));
    }
    Ok(Command::Fmt {
        path,
        value,
        sf,
        from,
        format,
    })
}

/// `migrate <file.emath> [--fix] [--check] [--dry-run] [--receipt <path>] [--json] | migrate --list-rules`
/// (05 §5, / ). Lossless
/// rewrites only; the receipt is the canonical stable-JSON artifact.
pub(super) fn parse_migrate_request(rest: &[String]) -> Result<Command, ParseKnownError> {
    const USAGE: &str = "migrate <file.emath> [--fix] [--check] [--dry-run] [--receipt <path>] [--json] | \
                         migrate --list-rules";
    if matches!(rest, [flag] if flag == "--list-rules") {
        return Ok(Command::Migrate {
            path: PathBuf::new(),
            fix: false,
            check_only: false,
            dry_run: false,
            receipt: None,
            list_rules: true,
            json: false,
        });
    }
    let mut path: Option<PathBuf> = None;
    let mut fix = false;
    let mut check_only = false;
    let mut dry_run = false;
    let mut receipt: Option<PathBuf> = None;
    let mut json = false;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--fix" => fix = true,
            "--check" => check_only = true,
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--receipt" => {
                i += 1;
                receipt = rest.get(i).map(PathBuf::from);
                if receipt.is_none() {
                    return Err(ParseKnownError::Pedagogic(
                        PedagogicError::new(
                            "E-CLI-MISSING-VALUE",
                            "flag `--receipt` requires a file path",
                            "flag `--receipt` in `emath migrate`",
                            "emath migrate <file.emath> --receipt receipt.json",
                        )
                        .with_command("migrate")
                        .with_flag("--receipt")
                        .with_usage(format!("emath {USAGE}")),
                    ));
                }
            }
            other if !other.starts_with('-') && path.is_none() => {
                path = Some(PathBuf::from(other));
            }
            _ => {
                return Err(ParseKnownError::Pedagogic(
                    PedagogicError::new(
                        "E-CLI-USAGE",
                        "invalid arguments for `emath migrate`",
                        "arguments for `emath migrate`",
                        "emath migrate <file.emath> [--check|--fix] [--dry-run] [--json]",
                    )
                    .with_command("migrate")
                    .with_usage(format!("emath {USAGE}")),
                ));
            }
        }
        i += 1;
    }
    let Some(path) = path else {
        return Err(ParseKnownError::Pedagogic(
            PedagogicError::new(
                "E-CLI-USAGE",
                "missing required argument `<file.emath>` for `emath migrate`",
                "positional argument 1 (expected path to `.emath` source file)",
                "emath migrate <file.emath> --check",
            )
            .with_command("migrate")
            .with_usage(format!("emath {USAGE}")),
        ));
    };
    if check_only && fix {
        return Err(ParseKnownError::Pedagogic(
            PedagogicError::new(
                "E-CLI-USAGE",
                "conflicting flags: `--check` and `--fix` cannot be used together",
                "flags `--check` and `--fix` in `emath migrate`",
                "choose either `--check` (dry run) or `--fix` (in-place modification)",
            )
            .with_command("migrate")
            .with_usage(format!("emath {USAGE}")),
        ));
    }
    Ok(Command::Migrate {
        path,
        fix,
        check_only,
        dry_run,
        receipt,
        list_rules: false,
        json,
    })
}
