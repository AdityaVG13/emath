//! Combined catalog for `emath-lab`. Usage/summary strings live in
//! `emath_cli::catalog` so production and lab cannot drift.

use emath_cli::catalog::{command_summary, command_usage};
pub use emath_cli::catalog::{flags_for, reject_unknown_flags, suggest_command, wants_help, wants_json};
use emath_cli::{CliExit, EXIT_OK};

/// Constructor `emath` tokens. This host forwards them; it does not
/// keep a second compiler.
pub const CORE_COMMANDS: &[&str] = emath_cli::catalog::COMMANDS;

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

/// Historical catalog order. Not the live constructor surface.
#[allow(dead_code)]
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
pub fn is_extracted_token(name: &str) -> bool {
    !is_core_command(name)
        && (LAB_COMMANDS.contains(&name) || emath_cli::catalog::EXTRACTED_COMMANDS.contains(&name))
}

#[must_use]
pub fn is_core_command(name: &str) -> bool {
    CORE_COMMANDS.contains(&name)
}

#[must_use]
pub fn capabilities_json() -> String {
    let mut commands = Vec::new();
    for name in CORE_COMMANDS {
        let mut entry = emath_artifact::JsonWriter::object();
        entry.string("name", name);
        entry.string("usage", command_usage(name).unwrap_or(name));
        entry.string("summary", command_summary(name).unwrap_or(""));
        commands.push(entry.finish());
    }
    let mut codes = emath_artifact::JsonWriter::object();
    codes.string("0", "completed");
    codes.string("2", "admission");
    codes.string("3", "unmet or partial");
    codes.string("4", "fault");
    codes.string("5", "incompatible checkpoint");
    let mut out = emath_artifact::JsonWriter::object();
    out.string("schema", "emath.capabilities");
    out.string("tool", "emath");
    out.string("version", env!("CARGO_PKG_VERSION"));
    out.string("contract", "constructor-layer");
    out.object_field("exit_codes", codes.finish().trim());
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
  First command to try: emath capabilities --json
  Human help: emath help [<command>]

`emath-lab` is not a constructor execution surface. eval, sweep,
genesis, plan, simulate, and the other extracted tokens refuse
E-KIND-GONE. Write an ordinary emath function or emath query and
emath run.

Exit codes
  0  completed
  2  admission
  3  unmet or partial
  4  fault
  5  incompatible checkpoint

Canonical agent loop
  1. emath capabilities --json
  2. emath check <file.emath> --json
  3. emath run <file.emath> --function NAME --set name=value --json
",
        emath_cli::catalog::version_text()
    )
}

#[must_use]
pub fn help_text() -> String {
    let mut out = String::from(
        "emath-lab is not a constructor host. Use `emath`.\n\nconstructor commands (forwarded):\n",
    );
    for command in CORE_COMMANDS {
        let Some(usage) = command_usage(command) else {
            continue;
        };
        let Some(summary) = command_summary(command) else {
            continue;
        };
        out.push_str("  emath ");
        out.push_str(usage);
        out.push('\n');
        out.push_str("      ");
        out.push_str(summary);
        out.push('\n');
    }
    out.push_str(
        "\nextracted tokens refuse E-KIND-GONE. Write an ordinary function or query and `emath run`.\n",
    );
    out
}

pub(crate) fn print_command_help(command: &str) -> CliExit {
    if is_extracted_token(command) {
        eprintln!("error: `{command}` is not a constructor command (E-KIND-GONE)");
        eprintln!("write an ordinary emath function or emath query and `emath run`");
        return emath_cli::EXIT_USAGE;
    }
    match emath_cli::catalog::command_help_text(command) {
        Some(text) => {
            print!("{text}");
            EXIT_OK
        }
        None => {
            eprintln!("error: unknown command `{command}`");
            eprintln!("try: emath help");
            emath_cli::EXIT_USAGE
        }
    }
}
