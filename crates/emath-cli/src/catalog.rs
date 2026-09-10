//! Command catalog used by help, `--help`, `--version`, and unknown-command
//! hints. Keep this list the single source for first-try discoverability.

use crate::CliExit;

/// Production `emath` tokens (compiler / user surface). Extracted tokens live
/// in `EXTRACTED_COMMANDS` and are served by `emath-lab`.
pub const COMMANDS: &[&str] = &[
    "api",
    "check",
    "plan",
    "planner",
    "build",
    "simulate",
    "new",
    "fmt",
    "migrate",
    "explain",
    "run",
    "search",
    "step",
    "test",
    "verify",
    "inspect",
    "diff",
    "doctor",
    "capabilities",
    "robot-docs",
    "triage",
    "help",
    "version",
];

/// Tokens moved to `emath-lab` (emath-qbk53). Production `emath` hints here.
pub const EXTRACTED_COMMANDS: &[&str] = &[
    "expand",
    "solve",
    "exactness",
    "freeze",
    "why",
    "assumptions",
    "parse",
    "compile",
    "library",
    "signature",
    "genesis",
    "eval",
    "sweep",
    "fit",
    "repl",
    "world",
    "portfolio",
    "meaning",
    "import",
    "artifact",
    "architecture",
    "coverage",
    "web",
    "serve",
    "bench",
    "vendor",
    "provider",
    "fork",
    "agent",
];

/// Returns true if the command is a recognized production emath command.
#[must_use]
pub fn is_known_command(command: &str) -> bool {
    COMMANDS.contains(&command)
}

/// One-line usage after `emath` for a known command.
#[must_use]
pub fn command_usage(command: &str) -> Option<&'static str> {
    Some(match command {
        "search" => crate::compiled_search::USAGE,
        "api" => "api [--search text] [--offset N] [--limit N] [--source file.emath] [--json]",
        "check" => "check <file.emath> [--verify-data] [--json]",
        "plan" => "plan <file.emath> [--json]",
        "planner" => "planner <file.emath> [--json] [--parametric]",
        "build" => "build <file.emath> [--out <dir>] [--verify] [--bin <entrypoint>] [--json]",
        "parse" => "parse --forest <file.emath> [--out <dir>]",
        "expand" => "expand <file.emath> [--json]",
        "solve" => "solve --check <file.emath> [--json] [--apply <label>]",
        "exactness" => "exactness <file.emath> [--json] [--raise units]",
        "freeze" => "freeze <file.emath> [--out <file>] [--json]",
        "why" => "why <file.emath> inference:N [--json]",
        "assumptions" => "assumptions <file.emath> [--json]",
        "signature" => "signature <file.emath> [--out <dir>]",
        "genesis" => "genesis <file.emath> --out <dir>",
        "eval" => {
            "eval <file.emath> [--world <name>] [--function NAME] [--set name=value] [--json]"
        }
        "sweep" => {
            "sweep <file.emath> --function NAME --grid name=v1,v2,... [--expect name=value] [--out <file>] [--json]"
        }
        "simulate" => {
            "simulate <file.emath> [--model NAME] [--dt N] [--t0 N] [--t1 N] [--method euler|rk4|rk45|backward-euler|velocity-verlet] [--atol N] [--rtol N] [--dt-max N] [--event name=value] [--set name=value] [--json]"
        }
        "fit" => "fit <file.emath> [--json]",
        "repl" => "repl <file.emath>",
        "compile" => "compile --parametric <file.emath> --out <dir> [--world LABEL]",
        "world" => "world show WORLD_ID --dir <dir>",
        "portfolio" => "portfolio show PORTFOLIO_ID --dir <dir>",
        "meaning" => "meaning list|set|unset|explain",
        "import" => "import modelica <file.mo> [--json]",
        "artifact" => "artifact check|battery <dir>",
        "architecture" => "architecture [--json]",
        "coverage" => "coverage [--emit json] [--check <ledger-file>]",
        "web" => "web [--port N] [--no-open] [--dist PATH]",
        "serve" => "serve [--port N] [--no-open] [--dist PATH]",
        "new" => "new <name> [--out <dir>]",
        "fmt" => {
            "fmt <file.emath> | fmt --value <literal> [--sf N] [--from UNIT] [--format \"0.1 %\"|preferred_unit UNIT]"
        }
        "migrate" => {
            "migrate <file.emath> [--fix] [--check] [--receipt <path>] | migrate --list-rules"
        }
        "explain" => {
            "explain <file.emath> [<symbol>] [--provenance] [--show-defaults] | explain \
                      E-LAW-001 [--json]"
        }
        "run" => {
            "run <file.emath> [--function NAME] [--set name=value] [--work N] [--cancel-file path] [--measure N] [--branch-from checkpoint --relation relation] [--out dir] [--json]"
        }
        "step" => {
            "step <checkpoint.json> [--work N] [--expect-revision N] [--cancel-file path] [--out dir] [--json]"
        }
        "test" => "test <file.emath> [--out <dir>]",
        "bench" => "bench <file.emath>",
        "verify" => "verify <artifact-dir> | verify <checkpoint.json> [--json]",
        "inspect" => "inspect <artifact-dir-or-checkpoint.json> [--json]",
        "diff" => "diff <a.emath> <b.emath> [--json]",
        "doctor" => "doctor [--json]",
        "vendor" => "vendor --out <dir>",
        "provider" => "provider list|inspect <id>|test <id> [--json]",
        "fork" => "fork status|sync [--dry-run] [--json]",
        "agent" => "agent check|plan|build|triage|propose <file> [--out <dir>]",
        "help" => "help [<command>]",
        "version" | "--version" | "-V" => "version",
        "capabilities" => "capabilities [--json]",
        "robot-docs" => "robot-docs [guide]",
        "triage" => "triage [<file.emath>] [--json]",
        _ => return None,
    })
}

/// Short description printed by `emath help <command>` / `emath <command> --help`.
#[must_use]
pub fn command_summary(command: &str) -> Option<&'static str> {
    Some(match command {
        "api" => {
            "discover commands, source syntax, and active Language Image features; executable status comes from installed reference/native implementations"
        }
        "check" => {
            "parse + admit, no codegen; `--verify-data` re-hashes declared sha256 provenance files (drift = E-OBS-HASH); `--json` emits codes and admission"
        }
        "plan" => "admit + goals + deterministic native resolution plan",
        "planner" => "provider-registry planning; `--parametric` lifts missing operators",
        "build" => "full pipeline to a published artifact (default out: target/emath)",
        "parse" => "genesis glyphs + bounded parse forest",
        "expand" => {
            "print the contracted form of L0/L1 scratch and L2 named shorthand; `--json` includes inferred-default notes"
        }
        "solve" => {
            "list labeled completions for a `solve` goal (`--check`); `--apply <label>` pins domain/holes. Never a naked numeric root"
        }
        "exactness" => {
            "print the declared/inferred/constructed/open meaning budget; `--raise units` declares one dimension"
        }
        "freeze" => {
            "write expanded source plus versioned emath.freeze.lock.v1; does not raise evidence authority or close open holes"
        }
        "why" => "explain one desugar/ledger inference (`inference:N`)",
        "assumptions" => "list inferred (not declared) meaning-budget rows",
        "signature" => "arity/fixity/type-variable signature inference",
        "genesis" => "world interpretation + portfolio + answer receipt",
        "eval" => {
            "evaluate a genesis-format reference term on the semantic VM (`--world`), or execute an admitted standard `emath function` spec through the generic EMIR/reference-VM stack (`--set name=value` binds inputs, `--function NAME` selects among several; plain eval runs the spec's own worked example); `--json` emits the `emath.eval-function` receipt and typed E-EVAL-* diagnostic codes on refusal"
        }
        "simulate" => {
            "integrate an admitted `emath model` with explicit Euler/classic RK4/RK45; `--atol/--rtol` enable adaptive RK45; `--event` locates one zero crossing; `--set` binds inputs, algebraic guesses, and state (scalars, `[vector]`, or `[[matrix]]`)"
        }
        "fit" => {
            "execute the declared fit goal to fitted values with linked Fitted provenance (model math stays in `.emath`); `--json` emits the deterministic envelope with parameters, confidence, and measured rows"
        }
        "repl" => "interactive eval session over the same admission and VM path",
        "sweep" => {
            "run a cartesian parameter grid over one admitted `emath function` through the same EMIR/reference-VM path as eval; per-cell pass/fail against `--expect name=value`; deterministic `emath.sweep.v1` artifact (meaning_id + grid + per-cell results, no wall-clock) on stdout with `--json` or to a file with `--out`; exit 0 only when every cell passes"
        }
        "compile" => {
            "parametric generated crate for an admitted world; `--world` selects one compiled world"
        }
        "world" => "print one world candidate artifact",
        "portfolio" => "print one interpretation portfolio artifact",
        "meaning" => "project-local interpretation lock (list|set|unset|explain)",
        "import" => "retain a Modelica subset as foreign-model declarations",
        "artifact" => "independent checker (`check`) or seeded negative-control battery",
        "architecture" => "provider-neutral pipeline map",
        "coverage" => {
            "language completeness coverage ledger: generated missing-math numbers with artifact-evidenced levels"
        }
        "web" => "localhost web playground on 127.0.0.1; Ctrl-C to stop",
        "serve" => "localhost web playground on 127.0.0.1; Ctrl-C to stop (alias for `web`)",
        "new" => "deterministic project scaffold; refuses overwrite (E-TLT-011)",
        "fmt" => {
            "canonical-form check (full rewrite is Phase 4); --value mode: sig-fig rounding + unit-preserving display (E-UNIT-FMT)"
        }
        "migrate" => {
            "lossless receipt-driven rewrites (05 section 5): `--check` reports without rewriting, `--fix` applies verified respells only (identity verified by re-lowering both sides), `--receipt <path>` writes the emath.migration-receipt v1 artifact; `--list-rules` prints the registry. Never rewrites a refusing source; identity-changing rewrites refuse"
        }
        "explain" => {
            "plan/provider explanation, binding provenance DAG, or `E-LAW-001` checker witness"
        }
        "run" => {
            "execute source mathematics with saved authored method states; --cancel-file stops between work units; --measure N records reference timings; changed-problem branches never satisfy the original goal"
        }
        "step" => {
            "continue a fixed target and saved authored methods with one commit per work unit; identical requests reuse committed work; competing requests cannot commit at the same revision"
        }
        "test" => "build with `--verify`; empty test surface is E-TLT-012",
        "bench" => "typed refusal E-TLT-004 until the comparison ruleset lands",
        "verify" => {
            "check published artifacts, or check saved certificates and source results without replaying refinement; no formal-proof or execution-history claim"
        }
        "inspect" => {
            "read saved mathematical results without execution, or print committed artifact manifests"
        }
        "diff" => "content-id fingerprint comparison of parse-admitted sources",
        "doctor" => "toolchain presence: rustc, cargo, rustfmt, clippy",
        "vendor" => "offline dependency lock snapshot",
        "provider" => "built-in provider descriptors; planned ids stay planned",
        "fork" => "upstream pin status; network sync refused offline (E-TLT-006)",
        "agent" => "structured emath.agent envelope; cannot bypass admission/plan/checks",
        "help" => "this catalog; `emath help <command>` prints one command",
        "version" | "--version" | "-V" => "print the emath-cli crate version",
        "capabilities" => "machine contract: commands, flags, exit codes, env vars",
        "robot-docs" => "paste-ready agent handbook (`guide`)",
        "triage" => "mega-command: orient, inspect health, and get ranked next actions",
        _ => return None,
    })
}

/// Closest known command for a typo, if edit distance is small.
#[must_use]
pub fn suggest_command(unknown: &str) -> Option<&'static str> {
    let needle = unknown.trim_start_matches('-');
    if needle.is_empty() {
        return None;
    }
    let mut best: Option<(&'static str, usize)> = None;
    for command in COMMANDS.iter().chain(EXTRACTED_COMMANDS) {
        if *command == needle {
            return Some(command);
        }
        let distance = edit_distance(needle, command);
        let prefix = command.starts_with(needle) || needle.starts_with(command);
        let score = if prefix {
            distance.saturating_sub(needle.len().min(2))
        } else {
            distance
        };
        if score <= 3 && best.is_none_or(|(_, current)| score < current) {
            best = Some((command, score));
        }
    }
    best.map(|(command, _)| command)
}

/// Deterministic `name version` line (no git SHA, no timestamp).
#[must_use]
pub fn version_text() -> String {
    format!("emath {}", env!("CARGO_PKG_VERSION"))
}

#[must_use]
pub fn wants_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

#[must_use]
pub fn wants_json(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

/// Usage + one-line summary for a single command. Returns `None` if unknown.
#[must_use]
pub fn command_help_text(command: &str) -> Option<String> {
    let usage = command_usage(command)?;
    let summary = command_summary(command)?;
    Some(format!(
        "emath {usage}\n{summary}\n\nexit codes: 0 ok, 1 refused/admission diagnostics, 2 usage or io error\nrun `emath help` for the full command list, or `emath api --json` for the machine contract\n"
    ))
}

/// Machine contract. `emath capabilities` and `emath capabilities --json`
/// emit the same deterministic document.
#[must_use]
pub fn capabilities_json() -> String {
    crate::capabilities::capabilities_json()
}

/// Paste-ready handbook for agents. No timestamps, no host paths.
#[must_use]
pub fn robot_docs_guide() -> String {
    format!(
        "\
emath agent handbook
====================

Identity
  {}
  First command to try: emath capabilities --json
  Human help: emath help [<command>]   or   emath <command> --help

Exit codes (stable contract)
  0  success (contract met)
  1  refused (admission / check / math refusal; look for E-* codes)
  2  usage (invalid syntax, missing arguments, unknown flag)
  3  toolchain (environment or toolchain missing; run `emath doctor`)
  4  io (file not found, cannot read/write, disk IO failure)
  5  safety (destructive mutation refused, overwrite blocked)

Canonical agent loop
  1. emath capabilities --json
  2. emath check <file.emath> --json
  3. emath plan <file.emath> --json
  4. emath build <file.emath> --json            # default out: target/emath
  5. emath agent check|plan|build <file.emath>  # same paths; cannot bypass checks

Rules
  - Never invent a passing test surface: empty tests are E-TLT-012.
  - bench is a typed refusal (E-TLT-004). Measure via cargo bench --profile release-perf --bench comprehensive_bench.
  - fork sync is offline-refused (E-TLT-006); use --dry-run.
  - Typos print `did you mean` on stderr; do not grep a catalog dump.
  - JSON is deterministic (in-tree writer). stdout is data; stderr is diagnostics.
",
        version_text()
    )
}

pub fn flags_for(command: &str) -> &'static [&'static str] {
    match command {
        "search" => &["--function", "--candidate", "--set", "--measure", "--out", "-o", "--json", "--help", "-h"],
        "api" => &[
            "--search", "--offset", "--limit", "--source", "--json", "--help", "-h",
        ],
        "run" => &[
            "--function",
            "--set",
            "--work",
            "--cancel-file",
            "--measure",
            "--branch-from",
            "--relation",
            "--out",
            "-o",
            "--json",
            "--help",
            "-h",
        ],
        "step" => &[
            "--work",
            "--expect-revision",
            "--cancel-file",
            "--out",
            "-o",
            "--json",
            "--help",
            "-h",
        ],
        "verify" => &["--json", "--help", "-h"],
        "explain" => &["--json", "--provenance", "--show-defaults", "--help", "-h"],
        "exactness" => &["--json", "--help", "-h", "--raise"],
        "check" => &["--json", "--verify-data", "--help", "-h"],
        "plan" | "architecture" | "inspect" | "diff" | "doctor" | "capabilities" | "triage" | "import"
        | "provider" | "expand" | "why" | "assumptions" => &["--json", "--help", "-h"],
        "coverage" => &["--emit", "--check", "--help", "-h"],
        "solve" => &["--check", "--json", "--apply", "--help", "-h"],
        "freeze" => &["--json", "--out", "-o", "--help", "-h"],
        "planner" => &["--json", "--parametric", "--help", "-h"],
        "fit" => &["--json", "--help", "-h"],
        "build" => &["--json", "--out", "-o", "--verify", "--bin", "--help", "-h"],
        "test" | "new" | "vendor" | "agent" | "signature" | "genesis" => {
            &["--out", "-o", "--help", "-h"]
        }
        "parse" => &["--forest", "--out", "-o", "--help", "-h"],
        "eval" => &["--world", "--function", "--set", "--json", "--help", "-h"],
        "sweep" => &[
            "--function",
            "--grid",
            "--expect",
            "--out",
            "-o",
            "--json",
            "--help",
            "-h",
        ],
        "simulate" => &[
            "--model", "--dt", "--t0", "--t1", "--method", "--atol", "--rtol", "--dt-max",
            "--event", "--set", "--json", "--help", "-h",
        ],
        "compile" => &["--parametric", "--out", "-o", "--world", "--help", "-h"],
        "world" | "portfolio" => &["--dir", "--out", "-o", "--help", "-h"],
        "meaning" => &[
            "--dir",
            "--world",
            "--hole",
            "--declaration",
            "--cap",
            "--json",
            "--help",
            "-h",
        ],
        "fork" => &["--dry-run", "--json", "--help", "-h"],
        "robot-docs" => &["--guide", "guide", "--json", "--help", "-h"],
        "web" | "serve" => &["--port", "--no-open", "--dist", "--help", "-h"],
        "fmt" => &["--value", "--sf", "--from", "--format", "--help", "-h"],
        "migrate" => &[
            "--fix",
            "--check",
            "--receipt",
            "--list-rules",
            "--help",
            "-h",
        ],
        _ => &["--help", "-h"],
    }
}

fn flag_takes_value(flag: &str) -> bool {
    matches!(
        flag,
        "--out"
            | "-o"
            | "--dir"
            | "--world"
            | "--port"
            | "--dist"
            | "--hole"
            | "--declaration"
            | "--cap"
            | "--dt"
            | "--t0"
            | "--t1"
            | "--method"
            | "--model"
            | "--atol"
            | "--rtol"
            | "--dt-max"
            | "--event"
            | "--set"
            | "--function"
            | "--work"
            | "--expect-revision"
            | "--cancel-file"
            | "--measure"
            | "--branch-from"
            | "--relation"
            | "--search"
            | "--offset"
            | "--limit"
            | "--source"
            | "--grid"
            | "--expect"
            | "--raise"
            | "--apply"
            | "--receipt"
    )
}

fn value_looks_like_flag(value: &str, known: &[&str]) -> bool {
    value.starts_with("--") || known.contains(&value)
}

fn missing_value_exit(command: &str, arg: &str, json: bool) -> CliExit {
    if json {
        let mut obj = emath_artifact::JsonWriter::object();
        obj.string("status", "error");
        obj.string("code", "E-CLI-MISSING-VALUE");
        let msg = format!("`{arg}` needs a value for `emath {command}`");
        obj.string("message", &msg);
        obj.string("flag", arg);
        obj.string("command", command);
        if let Some(usage) = command_usage(command) {
            let u = format!("emath {usage}");
            obj.string("usage", &u);
        }
        let t = format!("emath help {command}");
        obj.string("try", &t);
        println!("{}", obj.finish());
        return CliExit::Usage;
    }
    eprintln!("error: `{arg}` needs a value for `emath {command}`");
    if let Some(usage) = command_usage(command) {
        eprintln!("usage: emath {usage}");
    }
    eprintln!("try: emath help {command}");
    CliExit::Usage
}

/// Refuse unknown flags instead of silently ignoring them.
pub fn reject_unknown_flags(command: &str, args: &[String]) -> Option<CliExit> {
    let known = flags_for(command);
    let json = wants_json(args);
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--" {
            break;
        }
        if arg.starts_with('-') && arg != "-" && !known.contains(&arg) {
            if json {
                let mut obj = emath_artifact::JsonWriter::object();
                obj.string("status", "error");
                obj.string("code", "E-CLI-UNKNOWN-FLAG");
                let msg = format!("unknown flag `{arg}` for `emath {command}`");
                obj.string("message", &msg);
                if let Some(hint) = suggest_flag(arg, known) {
                    obj.string("did_you_mean", hint);
                }
                if let Some(usage) = command_usage(command) {
                    let u = format!("emath {usage}");
                    obj.string("usage", &u);
                }
                let t = format!("emath help {command}");
                obj.string("try", &t);
                println!("{}", obj.finish());
                return Some(CliExit::Usage);
            }
            eprintln!("error: unknown flag `{arg}` for `emath {command}`");
            if let Some(hint) = suggest_flag(arg, known) {
                eprintln!("did you mean `{hint}`?");
            }
            if let Some(usage) = command_usage(command) {
                eprintln!("usage: emath {usage}");
            }
            eprintln!("try: emath help {command}");
            return Some(CliExit::Usage);
        }
        if flag_takes_value(arg) {
            // Value-taking flags at EOL used to fall through to silent
            // defaults (e.g. `agent build f --out`). A following flag
            // token is not a value (`freeze f --out --json`).
            let missing =
                index + 1 >= args.len() || value_looks_like_flag(args[index + 1].as_str(), known);
            if missing {
                return Some(missing_value_exit(command, arg, json));
            }
            index += 1;
        }
        index += 1;
    }
    None
}

fn suggest_flag(unknown: &str, known: &'static [&'static str]) -> Option<&'static str> {
    let needle = unknown.trim_start_matches('-');
    let mut best: Option<(&'static str, usize)> = None;
    for flag in known {
        let flag_needle = flag.trim_start_matches('-');
        if needle.is_empty() || flag_needle.is_empty() {
            continue;
        }
        let distance = edit_distance(needle, flag_needle);
        let prefix = flag_needle.starts_with(needle) || needle.starts_with(flag_needle);
        let part_match = flag_needle
            .split('-')
            .any(|part| part == needle || edit_distance(needle, part) <= 1);
        let score = if part_match {
            1
        } else if prefix {
            distance.saturating_sub(needle.len().min(3))
        } else {
            distance
        };
        if score <= 3 && best.is_none_or(|(_, current)| score < current) {
            best = Some((flag, score));
        }
    }
    best.map(|(flag, _)| flag)
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (i, left_ch) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_ch) in right.iter().enumerate() {
            let cost = usize::from(left_ch != right_ch);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}
