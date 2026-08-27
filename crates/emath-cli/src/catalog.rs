//! Command catalog used by help, `--help`, `--version`, and unknown-command
//! hints. Keep this list the single source for first-try discoverability.

/// Every top-level token `emath <command>` accepts (plus version aliases).
pub const COMMANDS: &[&str] = &[
    "check",
    "plan",
    "planner",
    "build",
    "parse",
    "expand",
    "solve",
    "exactness",
    "freeze",
    "why",
    "assumptions",
    "signature",
    "genesis",
    "eval",
    "simulate",
    "repl",
    "compile",
    "world",
    "portfolio",
    "meaning",
    "import",
    "artifact",
    "architecture",
    "web",
    "serve",
    "new",
    "fmt",
    "explain",
    "run",
    "test",
    "bench",
    "verify",
    "inspect",
    "diff",
    "doctor",
    "vendor",
    "provider",
    "fork",
    "agent",
    "help",
    "version",
    "capabilities",
    "robot-docs",
];

/// One-line usage after `emath` for a known command.
#[must_use]
pub fn command_usage(command: &str) -> Option<&'static str> {
    Some(match command {
        "check" => "check <file.emath> [--json]",
        "plan" => "plan <file.emath> [--json]",
        "planner" => "planner <file.emath> [--json] [--parametric]",
        "build" => "build <file.emath> [--out <dir>] [--verify] [--json]",
        "parse" => "parse --forest <file.emath> [--out <dir>]",
        "expand" => "expand <file.emath> [--json]",
        "solve" => "solve --check <file.emath> [--json] [--apply <label>]",
        "exactness" => "exactness <file.emath> [--json] [--raise units]",
        "freeze" => "freeze <file.emath> [--out <file>] [--json]",
        "why" => "why <file.emath> inference:N [--json]",
        "assumptions" => "assumptions <file.emath> [--json]",
        "signature" => "signature <file.emath> [--out <dir>]",
        "genesis" => "genesis <file.emath> --out <dir>",
        "eval" => "eval <file.emath> [--world <name>] [--json]",
        "simulate" => {
            "simulate <file.emath> [--dt N] [--t0 N] [--t1 N] [--method euler|rk4|rk45] [--atol N] [--rtol N] [--dt-max N] [--event name=value] [--set name=value] [--json]"
        }
        "repl" => "repl <file.emath>",
        "compile" => "compile --parametric <file.emath> --out <dir> [--world LABEL]",
        "world" => "world show WORLD_ID --dir <dir>",
        "portfolio" => "portfolio show PORTFOLIO_ID --dir <dir>",
        "meaning" => "meaning list|set|unset|explain",
        "import" => "import modelica <file.mo> [--json]",
        "artifact" => "artifact check|battery <dir>",
        "architecture" => "architecture [--json]",
        "web" => "web [--port N] [--no-open] [--dist PATH]",
        "serve" => "serve [--port N] [--no-open] [--dist PATH]",
        "new" => "new <name> [--out <dir>]",
        "fmt" => "fmt <file.emath>",
        "explain" => "explain <file.emath> [<symbol>] [--provenance] | explain E-LAW-001 [--json]",
        "run" => "run <file.emath> [--out <dir>]",
        "test" => "test <file.emath> [--out <dir>]",
        "bench" => "bench <file.emath>",
        "verify" => "verify <artifact-dir>",
        "inspect" => "inspect <artifact-dir> [--json]",
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
        _ => return None,
    })
}

/// Short description printed by `emath help <command>` / `emath <command> --help`.
#[must_use]
pub fn command_summary(command: &str) -> Option<&'static str> {
    Some(match command {
        "check" => "parse + admit, no codegen; `--json` emits codes and admission",
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
            "evaluate an admitted `emath custom` term on the semantic VM; `--json` emits the answer envelope and diagnostic codes on refusal"
        }
        "simulate" => {
            "integrate an admitted `emath model` with explicit Euler/classic RK4/RK45; `--atol/--rtol` enable adaptive RK45; `--event` locates one zero crossing; `--set` binds inputs, algebraic guesses, and state (scalars, `[vector]`, or `[[matrix]]`)"
        }
        "repl" => "interactive eval session over the same admission and VM path",
        "compile" => {
            "parametric generated crate for an admitted world; `--world` selects one compiled world"
        }
        "world" => "print one world candidate artifact",
        "portfolio" => "print one interpretation portfolio artifact",
        "meaning" => "project-local interpretation lock (list|set|unset|explain)",
        "import" => "retain a Modelica subset as foreign-model declarations",
        "artifact" => "independent checker (`check`) or seeded negative-control battery",
        "architecture" => "provider-neutral pipeline map",
        "web" => "localhost web playground on 127.0.0.1; Ctrl-C to stop",
        "serve" => "localhost web playground on 127.0.0.1; Ctrl-C to stop (alias for `web`)",
        "new" => "deterministic project scaffold; refuses overwrite (E-TLT-011)",
        "fmt" => "canonical-form check (full rewrite is Phase 4)",
        "explain" => {
            "plan/provider explanation, binding provenance DAG, or `E-LAW-001` checker witness"
        }
        "run" => "build then execute the generated crate (library crates run example tests)",
        "test" => "build with `--verify`; empty test surface is E-TLT-012",
        "bench" => "typed refusal E-TLT-004 until the comparison ruleset lands",
        "verify" => "independent artifact re-verification (same as `artifact check`)",
        "inspect" => "print committed artifact manifests",
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
    for command in COMMANDS {
        if *command == needle {
            return Some(command);
        }
        let distance = edit_distance(needle, command);
        let prefix = command.starts_with(needle) || needle.starts_with(command);
        let score = if prefix {
            distance.saturating_sub(1)
        } else {
            distance
        };
        if score <= 2 && best.is_none_or(|(_, current)| score < current) {
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
        "emath {usage}\n{summary}\n\nexit codes: 0 ok, 1 refused/admission diagnostics, 2 usage or io error\nrun `emath help` for the full command list, or `emath capabilities --json` for the machine contract\n"
    ))
}

/// Machine contract. `emath capabilities` and `emath capabilities --json`
/// emit the same deterministic document.
#[must_use]
pub fn capabilities_json() -> String {
    let mut commands = Vec::new();
    for name in COMMANDS {
        let mut entry = emath_artifact::JsonWriter::object();
        entry.string("name", name);
        entry.string("usage", command_usage(name).unwrap_or(name));
        entry.string("summary", command_summary(name).unwrap_or(""));
        commands.push(entry.finish());
    }
    let mut codes = emath_artifact::JsonWriter::object();
    codes.string("0", "ok");
    codes.string("1", "refused or admission/build diagnostics");
    codes.string("2", "usage or io error");
    let mut out = emath_artifact::JsonWriter::object();
    out.string("schema", "emath.capabilities");
    out.string("tool", "emath");
    out.string("version", env!("CARGO_PKG_VERSION"));
    out.string("contract", "emath-cli Phase 1 + Semantic Genesis G0-G3");
    out.object_field("exit_codes", codes.finish().trim());
    out.strings("env_vars", &["EMATH_WEB_DIST".to_string()]);
    out.objects("commands", &commands);
    out.finish()
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

Exit codes (stable)
  0  success
  1  refused / admission or build diagnostics (look for E-* codes)
  2  usage or io error (stderr names the exact next command)

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
        "explain" => &["--json", "--provenance", "--help", "-h"],
        "check" | "plan" | "architecture" | "inspect" | "diff" | "doctor" | "capabilities"
        | "import" | "provider" | "expand" | "exactness" | "why" | "assumptions" => {
            &["--json", "--help", "-h", "--raise"]
        }
        "solve" => &["--check", "--json", "--apply", "--help", "-h"],
        "freeze" => &["--json", "--out", "-o", "--help", "-h"],
        "planner" => &["--json", "--parametric", "--help", "-h"],
        "build" => &["--json", "--out", "-o", "--verify", "--help", "-h"],
        "run" | "test" | "new" | "vendor" | "agent" | "signature" | "genesis" => {
            &["--out", "-o", "--help", "-h"]
        }
        "parse" => &["--forest", "--out", "-o", "--help", "-h"],
        "eval" => &["--world", "--json", "--help", "-h"],
        "simulate" => &[
            "--dt", "--t0", "--t1", "--method", "--atol", "--rtol", "--dt-max", "--event", "--set",
            "--json", "--help", "-h",
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
        "robot-docs" => &["--guide", "--help", "-h"],
        "web" | "serve" => &["--port", "--no-open", "--dist", "--help", "-h"],
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
            | "--atol"
            | "--rtol"
            | "--dt-max"
            | "--event"
            | "--set"
            | "--raise"
    )
}

/// Refuse unknown flags instead of silently ignoring them.
pub fn reject_unknown_flags(command: &str, args: &[String]) -> Option<u8> {
    let known = flags_for(command);
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--" {
            break;
        }
        if arg.starts_with('-') && arg != "-" && !known.contains(&arg) {
            eprintln!("error: unknown flag `{arg}` for `emath {command}`");
            if let Some(hint) = suggest_flag(arg, known) {
                eprintln!("did you mean `{hint}`?");
            }
            if let Some(usage) = command_usage(command) {
                eprintln!("usage: emath {usage}");
            }
            eprintln!("try: emath help {command}");
            return Some(2);
        }
        if flag_takes_value(arg) {
            // Value-taking flags at EOL used to fall through to silent
            // defaults (e.g. `agent build f --out`).
            if index + 1 >= args.len() {
                eprintln!("error: `{arg}` needs a value for `emath {command}`");
                if let Some(usage) = command_usage(command) {
                    eprintln!("usage: emath {usage}");
                }
                eprintln!("try: emath help {command}");
                return Some(2);
            }
            index += 1;
        }
        index += 1;
    }
    None
}

fn suggest_flag(unknown: &str, known: &'static [&'static str]) -> Option<&'static str> {
    let mut best: Option<(&'static str, usize)> = None;
    for flag in known {
        let distance = edit_distance(unknown, flag);
        if distance <= 3 && best.is_none_or(|(_, current)| distance < current) {
            best = Some((flag, distance));
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
