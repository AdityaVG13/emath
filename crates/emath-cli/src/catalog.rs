//! Command catalog used by help, `--help`, `--version`, and unknown-command
//! hints. Keep this list the single source for first-try discoverability.

use crate::CliExit;

/// Production `emath` tokens (compiler / user surface). Extracted tokens in
/// `EXTRACTED_COMMANDS` are historical names: they refuse, they are not
/// a second language.
pub const COMMANDS: &[&str] = &[
    "api",
    "check",
    "build",
    "new",
    "fmt",
    "migrate",
    "explain",
    "run",
    "loop",
    "dream",
    "keep",
    "verify",
    "export",
    "compare",
    "experiment",
    "step",
    "test",
    "inspect",
    "diff",
    "doctor",
    "catalog",
    "capabilities",
    "robot-docs",
    "triage",
    "next",
    "help",
    "version",
];

/// Historical tokens. Discovery refuses them (`E-KIND-GONE`); it does
/// not suggest a parallel lab language.
pub const EXTRACTED_COMMANDS: &[&str] = &[
    "plan",
    "planner",
    "simulate",
    "search",
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

/// Canonical command aliases mapping shorthand names to primary commands.
pub const ALIASES: &[(&str, &str)] = &[
    ("c", "check"),
    ("chk", "check"),
    ("b", "build"),
    ("doc", "doctor"),
    ("format", "fmt"),
    ("t", "test"),
    ("r", "run"),
    ("df", "diff"),
    ("caps", "capabilities"),
    ("guide", "robot-docs"),
    ("tr", "triage"),
    ("n", "next"),
];

/// Returns the primary command for a given alias, or None if not an alias.
#[must_use]
pub fn resolve_alias(name: &str) -> Option<&'static str> {
    for &(alias, canonical) in ALIASES {
        if alias == name {
            return Some(canonical);
        }
    }
    None
}

/// Returns list of aliases for a given primary command.
#[must_use]
pub fn command_aliases(command: &str) -> &'static [&'static str] {
    let canonical = resolve_alias(command).unwrap_or(command);
    match canonical {
        "check" => &["c", "chk"],
        "build" => &["b"],
        "doctor" => &["doc"],
        "fmt" => &["format"],
        "test" => &["t"],
        "run" => &["r"],
        "diff" => &["df"],
        "capabilities" => &["caps"],
        "robot-docs" => &["guide"],
        "triage" => &["tr"],
        "next" => &["n"],
        _ => &[],
    }
}

/// Returns true if the command is a recognized production emath command.
#[must_use]
pub fn is_known_command(command: &str) -> bool {
    COMMANDS.contains(&command) || resolve_alias(command).is_some_and(|c| COMMANDS.contains(&c))
}

/// One-line usage after `emath` for a known command.
#[must_use]
pub fn command_usage(command: &str) -> Option<&'static str> {
    let resolved = resolve_alias(command).unwrap_or(command);
    Some(match resolved {
        "api" => "api [--search text] [--offset N] [--limit N] [--source file.emath] [--json]",
        "check" => "check <file.emath|-> [--verify-data] [--json]",
        "build" => "build <file.emath> [--out <dir>] [--dry-run] [--worlds w1,w2,...] [--json]",
        "new" => "new <name> [--out <dir>] [--dry-run] [--force] [--json]",
        "fmt" => "fmt <file.emath|->",
        "migrate" => {
            "migrate <file.emath> [--fix] [--check] [--dry-run] [--receipt <path>] [--json] | migrate --list-rules"
        }
        "explain" => {
            "explain <E-CODE> [--list-codes] [--json]"
        }
        "run" => {
            "run <file.emath> [--function NAME] [--set name=value] [--set-file path.json] [--work N] [--worlds w1,w2,...] [--out dir] [--json]"
        }
        "loop" => "loop <file.emath> [--target StepFn] [--budget N] [--script path]",
        "dream" => "dream <file.emath> [--target StepFn] [--run NAME] [--budget N] [--worlds w1,w2,...] [--detach] | dream watch|resume|fork|export <run> | dream list",
        "export" => "export lean <file.emath|dir> [--out <dir>] [--check] [--work N]",
        "keep" => "keep <file.emath> --function F [--name NAME] --to <library.emath> [--set name=value] [--work N] [--overwrite]",
        "verify" => "verify <library.emath> [--work N] [--json]",
        "compare" => "compare <file.emath> --function F [--set name=value] [--grid name=v1,v2,... | name=LO..HI | name=LO..HI/N] [--world reference|exact|float64|float32|bfloat16|int64|interval|affine] [--work N] [--json]",
        "experiment" => "experiment <manifest.json> [--state dir] [--audit-ledger path] [--stop-after-pairs N]",
        "step" => {
            "step <checkpoint.json> [--work N] [--out dir] [--json]"
        }
        "test" => "test <file.emath> [--work N] [--out <dir>]",
        "inspect" => "inspect <checkpoint.json> [--json]",
        "diff" => "diff <a.emath> <b.emath> [--json]",
        "doctor" => "doctor [--json]",
        "catalog" => "catalog [--json]",
        "help" => "help [<command>]",
        "version" | "--version" | "-V" => "version",
        "capabilities" => "capabilities [--json]",
        "robot-docs" => "robot-docs [guide]",
        "triage" => "triage [<file.emath>] [--json]",
        "next" => "next [<file.emath>] [--json]",
        _ => return None,
    })
}

/// Short description printed by `emath help <command>` / `emath <command> --help`.
#[must_use]
pub fn command_summary(command: &str) -> Option<&'static str> {
    let resolved = resolve_alias(command).unwrap_or(command);
    Some(match resolved {
        "api" => {
            "discover commands, source syntax, active Language Image features, and module exports (name, import path, signature, summary, file:line; --search needs every word in the name, module, or summary and lists name matches first); executable status comes from installed reference/native implementations"
        }
        "check" => {
            "parse + admit, no codegen; `-` reads source from stdin (pipelines); `--verify-data` re-hashes declared sha256 provenance files (drift = E-OBS-HASH); `--json` emits codes and admission"
        }
        "build" => "emit fully lowered runnable Rust for an admitted constructor function",
        "new" => "deterministic project scaffold; refuses overwrite (E-TLT-011) unless --force is specified; --dry-run simulates actions",
        "fmt" => {
            "canonical-form check; `-` reads source from stdin"
        }
        "migrate" => {
            "format-only respell of constructor source (`--check` / `--dry-run` / `--fix`). Not a recipe translator and not a second language"
        }
        "explain" => {
            "diagnostic-code lookup (`emath explain E-TYPE-002`). File/plan explanation refuses: there is no goals planner"
        }
        "run" => {
            "evaluate an `emath function` or `emath query` under explicit inputs; `--json` prints the constructor receipt"
        }
        "loop" => {
            "open a research-loop session surface and drive it with line commands (step, run, show, grow-case, save, load); `--script` replays a command file deterministically"
        }
        "dream" => {
            "launch a research run as a background driver writing `.emath/runs/<name>/` and watch it live in a terminal dashboard; `watch`, `resume`, `fork`, `list`, `export` operate on runs; detaching never stops the run"
        }
        "export" => {
            "translate a file or a whole folder of emath functions and their examples into a Lean 4 project (`lean/` beside the sources unless `--out`); untranslatable declarations are named with the reason; without `--check` only the files are written, for review; `--check` runs `lake build`, where the Lean kernel proves every example; `--work N` sets the emath work budget each example runs under, as in `emath test`"
        }
        "keep" => {
            "evaluate a function that returns a closed quoted program and write it into a library file as an ordinary `emath function` with its source and content id; different existing text refuses without `--overwrite`"
        }
        "verify" => {
            "recheck every kept claim in a library file against the checkers the library itself names, one row per claim, never rerunning the search; a tampered certificate, a content id that does not match the claim's content, or a checker the library neither defines nor uses refuses with the claim named; `--work N` sets the work budget for producing each claim and replaying its checker, as `emath test --work`"
        }
        "compare" => {
            "evaluate one function under several carrier worlds (reference as authored, exact rationals, binary64) and report every answer, the gap between them, and whether they agree; never picks a winner"
        }
        "experiment" => {
            "build and measure an explicit baseline and candidate on one workload, then return an authored evaluator's decision; resumable with `--state`"
        }
        "step" => {
            "resume a constructor-layer continuation; incompatible checkpoints refuse"
        }
        "test" => "run authored `tests:` on constructor source",
        "inspect" => {
            "read a saved constructor checkpoint without executing"
        }
        "diff" => "content-id fingerprint comparison of parse-admitted sources",
        "doctor" => "toolchain presence: rustc, cargo, rustfmt, clippy",
        "help" => "this catalog; `emath help <command>` prints one command",
        "version" | "--version" | "-V" => "print the emath-cli crate version",
        "capabilities" => "machine contract: commands, flags, exit codes, env vars",
        "catalog" => "full command matrix export: usage, summary, aliases, flags, examples per command",
        "robot-docs" => "paste-ready agent handbook (`guide`)",
        "triage" => "mega-command: orient, inspect health, and get ranked next actions",
        "next" => "next-action engine: return highest-priority next step and claim command",
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
    if let Some(canonical) = resolve_alias(needle) {
        return Some(canonical);
    }
    let mut best: Option<(&'static str, usize)> = None;
    for &command in COMMANDS {
        if command == needle {
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
    for &(alias, canonical) in ALIASES {
        let distance = edit_distance(needle, alias);
        if distance <= 1 && best.is_none_or(|(_, current)| distance < current) {
            best = Some((canonical, distance));
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
    args.iter().any(|arg| arg == "--help" || arg == "-h" || arg == "help")
}

#[must_use]
pub fn wants_json(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

#[must_use]
pub fn flag_description(flag: &str) -> &'static str {
    match flag {
        "--json" => "emit machine-readable JSON output to stdout",
        "--help" | "-h" => "print help information",
        "--verify-data" => "re-hash declared sha256 provenance data files",
        "--out" | "-o" => "output directory or file path",
        "--set" => "parameter override (name=value); value may be a scalar or [v0, v1, …]",
        "--set-file" => "JSON object of name to scalar or array; --set overrides the same name",
        "--format" => "export format",
        "--fix" => "apply verified lossless migrations in-place",
        "--check" => "dry-run check without modifying files",
        "--receipt" => "path to write migration receipt JSON artifact",
        "--list-rules" => "list registered migration rules",
        "--list-codes" => "list the CLI diagnostic code registry",
        "--provenance" => "show binding provenance DAG",
        "--show-defaults" => "show implicit and inferred default assumptions",
        "--search" => "search query text",
        "--offset" => "pagination limit",
        "--limit" => "pagination limit",
        "--source" => "source file filter",
        "--color" => "control ANSI color output: auto, always, never",
        "--no-color" => "suppress ANSI color output (conforms to NO_COLOR)",
        "--function" => "function name to execute or search",
        "--work" => "maximum work units to execute",
        "--guide" | "guide" => "handbook guide section",
        "--grid" => "parameter grid (name=v1,v2,...; compare also takes name=LO..HI[/N])",
        "--world" => "target world label",
        "--worlds" => "carrier worlds quote.worlds() returns (comma-separated; default every world)",
        "--dry-run" => "dry-run without modifying state",
        "--force" => "force overwrite of existing project directories or files",
        "--state" => "experiment state directory (checkpoint and builds); reuse it to resume",
        "--audit-ledger" => "audit ledger file shared by experiments that must not reuse audits",
        "--stop-after-pairs" => "commit this many measured pairs, then stop resumably",
        _ => "command-specific option",
    }
}

#[must_use]
pub fn command_examples(command: &str) -> &'static [&'static str] {
    let resolved = resolve_alias(command).unwrap_or(command);
    match resolved {
        "check" => &[
            "emath check program.emath",
            "emath check - < program.emath",
            "emath check program.emath --json",
        ],
        "build" => &[
            "emath build program.emath",
            "emath build program.emath --out dist/",
            "emath build program.emath --dry-run",
            "emath build program.emath --json",
        ],
        "new" => &[
            "emath new my_program",
            "emath new my_program --out work/",
            "emath new my_program --dry-run",
            "emath new my_program --force",
        ],
        "fmt" => &[
            "emath fmt program.emath",
            "emath fmt - < program.emath",
        ],
        "migrate" => &[
            "emath migrate program.emath --check",
            "emath migrate program.emath --dry-run",
            "emath migrate program.emath --fix",
        ],
        "explain" => &[
            "emath explain E-TYPE-002",
            "emath explain E-TYPE-002 --json",
            "emath explain --list-codes",
        ],
        "api" => &[
            "emath api --json",
            "emath api --search object --json",
            "emath api --source program.emath --json",
        ],
        "run" => &[
            "emath run program.emath",
            "emath run program.emath --function AddExact --json",
            "emath run program.emath --set a=2 --set b=1 --json",
        ],
        "dream" => &[
            "emath dream research.emath --target StepDream",
            "emath dream research.emath --detach --run warmup",
            "emath dream watch warmup",
            "emath dream resume warmup --max-batches 500",
            "emath dream export warmup --format csv -o batches.csv",
        ],
        "step" => &[
            "emath step checkpoint.json --work 100 --json",
        ],
        "test" => &[
            "emath test program.emath",
            "emath test program.emath --work 5000000",
        ],
        "inspect" => &[
            "emath inspect checkpoint.json --json",
        ],
        "diff" => &[
            "emath diff a.emath b.emath --json",
        ],
        "doctor" => &[
            "emath doctor",
            "emath doctor --json",
        ],
        "triage" => &[
            "emath triage",
            "emath triage program.emath --json",
        ],
        "next" => &[
            "emath next",
            "emath next program.emath --json",
        ],
        "capabilities" => &[
            "emath capabilities",
            "emath capabilities --json",
        ],
        "catalog" => &[
            "emath catalog",
            "emath catalog --json",
        ],
        "robot-docs" => &[
            "emath robot-docs guide",
            "emath robot-docs --json",
        ],
        "help" => &[
            "emath help",
            "emath help check",
            "emath check --help",
            "emath help --json",
        ],
        _ => &[],
    }
}

/// Usage + one-line summary for a single command. Returns `None` if unknown.
#[must_use]
pub fn command_help_text(command: &str) -> Option<String> {
    let resolved = resolve_alias(command).unwrap_or(command);
    let usage = command_usage(resolved)?;
    let summary = command_summary(resolved)?;
    let flags = flags_for(resolved);
    let examples = command_examples(resolved);
    let aliases = command_aliases(resolved);

    let mut out = String::new();
    out.push_str(&format!("Usage:\n  emath {usage}\n\n"));
    out.push_str(&format!("Summary:\n  {summary}\n\n"));

    if !aliases.is_empty() {
        out.push_str(&format!("Aliases:\n  {}\n\n", aliases.join(", ")));
    }

    if !flags.is_empty() {
        out.push_str("Flags:\n");
        for flag in flags {
            let desc = flag_description(flag);
            out.push_str(&format!("  {flag:<20} {desc}\n"));
        }
        if !flags.contains(&"--color") {
            out.push_str(&format!("  {:<20} {}\n", "--color", flag_description("--color")));
            out.push_str(&format!("  {:<20} {}\n", "--no-color", flag_description("--no-color")));
        }
        out.push('\n');
    }

    if !examples.is_empty() {
        out.push_str("Examples:\n");
        for ex in examples {
            out.push_str(&format!("  {ex}\n"));
        }
        out.push('\n');
    }

    out.push_str("Exit Codes:\n");
    for (exit, meaning) in crate::EXIT_MEANINGS {
        out.push_str(&format!("  {}    {meaning}\n", exit as u8));
    }
    out.push('\n');
    out.push_str(
        "Environment Conventions:\n  NO_COLOR=1          Suppress ANSI colors and formatting (https://no-color.org)\n  TERM=dumb           Suppress terminal styling and interactive codes\n  CI=1                Force non-interactive batch mode\n\n",
    );
    out.push_str("See Also:\n  Run `emath help` for full command index, or `emath capabilities --json` for machine contract.\n");

    Some(out)
}

/// Structured JSON representation of single command help.
#[must_use]
pub fn command_help_json(command: &str) -> Option<String> {
    let resolved = resolve_alias(command).unwrap_or(command);
    let usage = command_usage(resolved)?;
    let summary = command_summary(resolved)?;
    let flags = flags_for(resolved);
    let examples = command_examples(resolved);
    let aliases = command_aliases(resolved);

    let mut obj = emath_core::JsonWriter::object();
    obj.string("status", "ok");
    obj.string("command", resolved);
    let full_usage = format!("emath {usage}");
    obj.string("usage", &full_usage);
    obj.string("summary", summary);

    if !aliases.is_empty() {
        let alias_strings: Vec<String> = aliases.iter().map(std::string::ToString::to_string).collect();
        obj.strings("aliases", &alias_strings);
    }

    let mut flag_items = Vec::new();
    for flag in flags {
        let mut flag_obj = emath_core::JsonWriter::object();
        flag_obj.string("flag", flag);
        flag_obj.string("description", flag_description(flag));
        flag_items.push(flag_obj.finish());
    }
    if !flags.contains(&"--color") {
        for common in ["--color", "--no-color"] {
            let mut flag_obj = emath_core::JsonWriter::object();
            flag_obj.string("flag", common);
            flag_obj.string("description", flag_description(common));
            flag_items.push(flag_obj.finish());
        }
    }
    obj.objects("flags", &flag_items);

    let ex_strings: Vec<String> = examples.iter().map(std::string::ToString::to_string).collect();
    obj.strings("examples", &ex_strings);

    let mut exits = emath_core::JsonWriter::object();
    for (exit, meaning) in crate::EXIT_MEANINGS {
        exits.string(&(exit as u8).to_string(), meaning);
    }
    let exits_body = exits.finish();
    obj.object_field("exit_codes", exits_body.trim());

    Some(obj.finish())
}

/// Structured JSON representation of full command catalog.
#[must_use]
pub fn catalog_help_json() -> String {
    let mut obj = emath_core::JsonWriter::object();
    obj.string("status", "ok");
    obj.string("description", "emath compiler command catalog");
    let mut cmd_items = Vec::new();
    for &command in COMMANDS {
        let Some(usage) = command_usage(command) else {
            continue;
        };
        let Some(summary) = command_summary(command) else {
            continue;
        };
        let mut c_obj = emath_core::JsonWriter::object();
        c_obj.string("name", command);
        let full_usage = format!("emath {usage}");
        c_obj.string("usage", &full_usage);
        c_obj.string("summary", summary);
        let aliases = command_aliases(command);
        if !aliases.is_empty() {
            let alias_strings: Vec<String> = aliases.iter().map(std::string::ToString::to_string).collect();
            c_obj.strings("aliases", &alias_strings);
        }
        cmd_items.push(c_obj.finish());
    }
    obj.objects("commands", &cmd_items);
    obj.finish()
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
  Guide: docs/guide/README.md in the emath repository (writing, running,
  solving a problem, and certifying it in Lean, end to end)

Aliases (single-letter & shorthand)
  c, chk -> check       b -> build
  doc -> doctor         format -> fmt
  t -> test             r -> run              df -> diff
  caps -> capabilities  guide -> robot-docs   tr -> triage
  n -> next

Exit codes
{}
Environment conventions
  NO_COLOR=1          Suppress all ANSI colors and formatting (https://no-color.org)
  TERM=dumb           Suppress terminal styling and interactive codes
  CI=1                Force non-interactive batch mode
  --color <mode>      Explicit override: auto (default), always, never
  --no-color          Explicit alias for --color never

Canonical agent loop
  1. emath capabilities --json
  2. emath check <file.emath> --json
  3. emath test <file.emath>
  4. emath run <file.emath> --function F --set name=value --json
  5. emath export lean <dir> --check
  plan, planner, simulate, eval, solve, and agent are not constructor commands.

Rules
  - Never invent a passing test surface: empty tests are E-TLT-012.
  - bench is a typed refusal (E-TLT-004). Measure via cargo bench --profile release-perf --bench comprehensive_bench.
  - fork sync is offline-refused (E-TLT-006); use --dry-run.
  - Typos print `did you mean` on stderr; do not grep a catalog dump.
  - JSON is deterministic (in-tree writer). stdout is data; stderr is diagnostics.
  - ANSI escapes are automatically suppressed when piped, under NO_COLOR, or with TERM=dumb.
",
        version_text(),
        crate::EXIT_MEANINGS.iter().map(|(exit, meaning)| format!("  {}  {meaning}\n", *exit as u8)).collect::<String>()
    )
}

pub fn flags_for(command: &str) -> &'static [&'static str] {
    let resolved = resolve_alias(command).unwrap_or(command);
    match resolved {
        "api" => &[
            "--search", "--offset", "--limit", "--source", "--json", "--help", "-h",
        ],
        "run" => &[
            "--function",
            "--set",
            "--set-file",
            "--work",
            "--worlds",
            "--out",
            "-o",
            "--json",
            "--help",
            "-h",
        ],
        "step" => &[
            "--work",
            "--out",
            "-o",
            "--json",
            "--help",
            "-h",
        ],
        "loop" => &["--target", "--budget", "--script", "--help", "-h"],
        "export" => &["--out", "-o", "--check", "--work", "--help", "-h"],
        "keep" => &["--function", "--name", "--to", "--set", "--work", "--overwrite", "--help", "-h"],
        "verify" => &["--json", "--work", "--help", "-h"],
        "compare" => &["--function", "--set", "--grid", "--world", "--work", "--json", "--help", "-h"],
        "dream" => &[
            "--target",
            "--run",
            "--overwrite",
            "--budget",
            "--budget-step",
            "--plateau-close",
            "--max-batches",
            "--max-units",
            "--checkpoint-every",
            "--keep-checkpoints",
            "--work",
            "--worlds",
            "--detach",
            "--theme",
            "--hide",
            "--no-measure",
            "--root",
            "--at",
            "--format",
            "--table",
            "-o",
            "--help",
            "-h",
        ],
        "experiment" => &["--state", "--audit-ledger", "--stop-after-pairs", "--help", "-h"],
        "explain" => &["--json", "--provenance", "--show-defaults", "--list-codes", "--help", "-h"],
        "check" => &["--json", "--verify-data", "--help", "-h"],
        "inspect" | "diff" | "doctor" | "capabilities" | "catalog" | "triage" | "next" => &["--json", "--help", "-h"],
        "build" => &["--json", "--out", "-o", "--dry-run", "--worlds", "--help", "-h"],
        "new" => &["--out", "-o", "--dry-run", "--force", "--json", "--help", "-h"],
        "test" => &["--work", "--out", "-o", "--help", "-h"],
        "robot-docs" => &["--guide", "guide", "--json", "--help", "-h"],
        "fmt" => &["--json", "--help", "-h"],
        "migrate" => &[
            "--fix",
            "--check",
            "--dry-run",
            "--receipt",
            "--list-rules",
            "--json",
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
            | "--world"
            | "--set"
            | "--set-file"
            | "--function"
            | "--work"
            | "--search"
            | "--offset"
            | "--limit"
            | "--source"
            | "--grid"
            | "--receipt"
            | "--target"
            | "--budget"
            | "--script"
    )
}

fn value_looks_like_flag(value: &str, known: &[&str]) -> bool {
    value.starts_with("--") || known.contains(&value)
}

/// `catalog [--json]`: the full command matrix. Human mode prints one
/// line per production command (name, summary, usage); JSON mode emits
/// `emath.catalog` with per-command `flags`, `aliases`, and `examples`
/// so an agent can plan invocations without parsing help text.
pub fn catalog_cmd(json: bool) -> CliExit {
    if json {
        use emath_core::JsonWriter;
        let mut out = JsonWriter::object();
        out.string("command", "catalog");
        out.string("schema", "emath.catalog");
        out.string("status", "ok");
        let mut rows = Vec::new();
        for &name in COMMANDS {
            let mut row = JsonWriter::object();
            row.string("name", name);
            if let Some(usage) = command_usage(name) {
                row.string("usage", usage);
            }
            if let Some(summary) = command_summary(name) {
                row.string("summary", summary);
            }
            let aliases = command_aliases(name);
            if !aliases.is_empty() {
                let alias_strings: Vec<String> = aliases.iter().map(std::string::ToString::to_string).collect();
                row.strings("aliases", &alias_strings);
            }
            let flags = flags_for(name);
            let flag_strings: Vec<String> = flags.iter().map(std::string::ToString::to_string).collect();
            row.strings("flags", &flag_strings);
            let examples: Vec<String> = command_examples(name)
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
            row.strings("examples", &examples);
            rows.push(row.finish().trim_end().to_string());
        }
        out.objects("commands", &rows);
        println!("{}", out.finish());
        return crate::EXIT_OK;
    }
    for &name in COMMANDS {
        let usage = command_usage(name).unwrap_or(name);
        let summary = command_summary(name).unwrap_or("");
        println!("{name}: {summary}");
        println!("  usage: emath {usage}");
    }
    println!(
        "historical lab tokens refuse constructor work; use `emath check` / `emath run`"
    );
    crate::EXIT_OK
}

fn missing_value_exit(command: &str, arg: &str, json: bool) -> CliExit {
    let mut err = crate::pedagogy::PedagogicError::new(
        "E-CLI-MISSING-VALUE",
        format!("flag `{arg}` requires a value for `emath {command}`"),
        format!("flag `{arg}` for `emath {command}` (expected value following `{arg}`)"),
        format!("pass a value following `{arg}`, e.g. `emath {command} ... {arg} <value>`"),
    )
    .with_command(command)
    .with_flag(arg);
    if let Some(usage) = command_usage(command) {
        err = err.with_usage(format!("emath {usage}"));
    }
    err.emit(json)
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
        if arg == "--no-color" {
            index += 1;
            continue;
        }
        if arg.starts_with("--color=") {
            index += 1;
            continue;
        }
        if arg == "--color" {
            let missing =
                index + 1 >= args.len() || value_looks_like_flag(args[index + 1].as_str(), known);
            if missing {
                return Some(missing_value_exit(command, arg, json));
            }
            index += 2;
            continue;
        }
        if arg.starts_with('-') && arg != "-" && !known.contains(&arg) {
            let hint = suggest_flag(arg, known);
            let remediation = match hint {
                Some(h) => format!("replace `{arg}` with `{h}`: `emath {command} {h}`"),
                None => format!("remove `{arg}` or run `emath help {command}` to see supported flags"),
            };
            let mut err = crate::pedagogy::PedagogicError::new(
                "E-CLI-UNKNOWN-FLAG",
                format!("unknown flag `{arg}` for `emath {command}`"),
                format!("flag `{arg}` in arguments for `emath {command}`"),
                remediation,
            )
            .with_command(command)
            .with_flag(arg);
            if let Some(h) = hint {
                err = err.with_did_you_mean(h);
            }
            if let Some(usage) = command_usage(command) {
                err = err.with_usage(format!("emath {usage}"));
            }
            return Some(err.emit(json));
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
    for &flag in known.iter().chain(&["--color", "--no-color"]) {
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
