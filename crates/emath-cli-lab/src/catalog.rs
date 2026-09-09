//! Combined catalog for `emath-lab`. Usage/summary strings live in
//! `emath_cli::catalog` so production and lab cannot drift.

use emath_cli::catalog::{command_summary, command_usage};
pub use emath_cli::catalog::{flags_for, reject_unknown_flags, suggest_command, wants_help, wants_json};
use emath_cli::{CliExit, EXIT_OK};

/// Production keep-set (also accepted by this host via forwarding).
pub const CORE_COMMANDS: &[&str] = &[
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
    "test",
    "verify",
    "inspect",
    "diff",
    "doctor",
    "help",
    "version",
];

/// Commands extracted from production `emath` (emath-qbk53).
pub const LAB_COMMANDS: &[&str] = &[
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
    "capabilities",
    "robot-docs",
];

/// Full historical catalog order so `emath.capabilities` stays one document.
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
    "sweep",
    "simulate",
    "fit",
    "repl",
    "compile",
    "world",
    "portfolio",
    "meaning",
    "import",
    "artifact",
    "architecture",
    "coverage",
    "web",
    "serve",
    "new",
    "fmt",
    "migrate",
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

#[must_use]
pub fn is_lab_command(name: &str) -> bool {
    LAB_COMMANDS.contains(&name)
}

#[must_use]
pub fn is_core_command(name: &str) -> bool {
    CORE_COMMANDS.contains(&name)
}

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

#[must_use]
pub fn robot_docs_guide() -> String {
    format!(
        "\
emath agent handbook
====================

Identity
  {}
  First command to try: emath-lab capabilities --json
  (alias document schema remains emath.capabilities)
  Human help: emath help [<command>]   or   emath-lab help [<command>]

Exit codes (stable)
  0  success
  1  refused / admission or build diagnostics (look for E-* codes)
  2  usage or io error (stderr names the exact next command)

Canonical agent loop
  1. emath-lab capabilities --json
  2. emath check <file.emath> --json
  3. emath plan <file.emath> --json
  4. emath build <file.emath> --json            # default out: target/emath
  5. emath-lab agent check|plan|build <file.emath>  # same paths; cannot bypass checks

Rules
  - Never invent a passing test surface: empty tests are E-TLT-012.
  - bench is a typed refusal (E-TLT-004). Measure via cargo bench --profile release-perf --bench comprehensive_bench.
  - fork sync is offline-refused (E-TLT-006); use --dry-run.
  - Typos print `did you mean` on stderr; do not grep a catalog dump.
  - JSON is deterministic (in-tree writer). stdout is data; stderr is diagnostics.
  - Production `emath` does not accept extracted lab/host tokens; use `emath-lab`.
",
        emath_cli::catalog::version_text()
    )
}

#[must_use]
pub fn help_text() -> String {
    let mut out = String::from(
        "emath-lab (extracted lab/host commands; production compiler is `emath`)\n\nusage:\n",
    );
    for command in COMMANDS {
        let Some(usage) = command_usage(command) else {
            continue;
        };
        let Some(summary) = command_summary(command) else {
            continue;
        };
        let host = if is_core_command(command) {
            "emath"
        } else {
            "emath-lab"
        };
        out.push_str("  ");
        out.push_str(host);
        out.push(' ');
        out.push_str(usage);
        out.push('\n');
        out.push_str("      ");
        out.push_str(summary);
        out.push('\n');
    }
    out.push_str("\nexit codes: 0 ok, 1 refused/admission diagnostics, 2 usage or io error\n");
    out
}

pub(crate) fn print_command_help(command: &str) -> CliExit {
    match emath_cli::catalog::command_help_text(command) {
        Some(text) => {
            print!("{text}");
            EXIT_OK
        }
        None => {
            eprintln!("error: unknown command `{command}`");
            eprintln!("try: emath-lab help");
            emath_cli::EXIT_USAGE
        }
    }
}
