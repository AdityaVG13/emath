//! In-tool capabilities contract export for AI agents and machine integration.
//!
//! Emits machine-readable feature flags, command definitions, exit codes,
//! and environment variables so agents don't require external documentation lookups.

use crate::CliExit;
use emath_core::JsonWriter;

pub fn capabilities_cmd(json: bool) -> CliExit {
    if json {
        print!("{}", capabilities_json());
    } else {
        print_human_summary();
    }
    CliExit::Ok
}

pub fn capabilities_json() -> String {
    let mut root = JsonWriter::object();
    root.string("tool", "emath");
    root.string("version", env!("CARGO_PKG_VERSION"));
    root.string("contract_version", "1.0.0");
    root.string("protocol_version", "1");
    root.string(
        "description",
        "Deterministic compiler and runtime engine for mathematics that computes",
    );

    let commands: &[(&str, &str, &str)] = &[
        ("check", "Parse and admit constructor source", "check <file.emath|-> [--json]"),
        ("run", "Evaluate an emath function or query; print a constructor receipt", "run <file.emath> [--function NAME] [--set name=value] [--set-file path.json] [--json]"),
        ("step", "Resume a constructor-layer continuation", "step <checkpoint.json> [--work N] [--json]"),
        ("inspect", "Read a saved constructor checkpoint", "inspect <checkpoint.json> [--json]"),
        ("verify", "Replay recorded observations; not a theorem", "verify <checkpoint.json> [--json]"),
        ("test", "Authored tests", "test <file.emath>"),
        ("build", "Emit fully lowered runnable Rust", "build <file.emath> [--out <dir>] [--json]"),
        ("api", "Constructor contracts and imported module exports", "api [--search text] [--json]"),
        ("new", "Deterministic project scaffold", "new <name> [--out <dir>] [--dry-run] [--force] [--json]"),
        ("fmt", "Canonical-form check", "fmt <file.emath|->"),
        ("migrate", "Format-only respell; not a recipe translator", "migrate <file.emath> [--check] [--dry-run] [--fix]"),
        ("explain", "Diagnostic-code lookup. File/plan explanation refuses", "explain <E-CODE> [--list-codes] [--json]"),
        ("diff", "Content-id fingerprint comparison", "diff <a.emath> <b.emath> [--json]"),
        ("doctor", "Toolchain presence and environment health", "doctor [--json]"),
        ("capabilities", "Machine-readable constructor CLI contract", "capabilities [--json]"),
        ("catalog", "Command matrix; historical tokens are marked refuse", "catalog [--json]"),
        ("help", "Command catalog and help text", "help [<command>]"),
        ("version", "Print emath-cli version", "version"),
    ];

    let mut command_objects = Vec::new();
    for (name, purpose, usage) in commands {
        let mut obj = JsonWriter::object();
        obj.string("name", name);
        obj.string("purpose", purpose);
        obj.string("usage", usage);
        let aliases = crate::catalog::command_aliases(name);
        if !aliases.is_empty() {
            let alias_strings: Vec<String> = aliases.iter().map(|s| s.to_string()).collect();
            obj.strings("aliases", &alias_strings);
        }
        command_objects.push(obj.finish());
    }
    root.objects("commands", &command_objects);

    let exit_codes: &[(&str, &str)] = &[
        ("0", "ok - command completed its declared operation"),
        ("2", "admission - syntax, type, or input admission failure"),
        ("3", "partial - unmet, partial, or suspended requested answer"),
        ("4", "fault - execution or backend fault"),
        ("5", "checkpoint - incompatible or corrupt checkpoint"),
    ];
    let mut exit_code_objects = Vec::new();
    for (code, meaning) in exit_codes {
        let mut obj = JsonWriter::object();
        obj.string("code", code);
        obj.string("meaning", meaning);
        exit_code_objects.push(obj.finish());
    }
    root.objects("exit_codes", &exit_code_objects);

    let env_vars: &[(&str, &str)] = &[
        ("NO_COLOR", "Suppress all ANSI terminal colors and formatting"),
        ("TERM", "Terminal type; TERM=dumb suppresses ANSI styling and cursor movement"),
        ("CI", "Continuous integration flag; suppresses interactive prompts"),
        ("EMATH_LOG", "Diagnostics logging level (error, warn, info, debug)"),
        ("SOURCE_DATE_EPOCH", "Deterministic UNIX timestamp for generated artifacts"),
    ];
    let mut env_objects = Vec::new();
    for (name, desc) in env_vars {
        let mut obj = JsonWriter::object();
        obj.string("name", name);
        obj.string("description", desc);
        env_objects.push(obj.finish());
    }
    root.objects("environment_variables", &env_objects);

    let features: Vec<String> = vec![
        "constructor_layer".to_string(),
        "json_streaming".to_string(),
        "deterministic_builds".to_string(),
        "pure_stdout_stderr_separation".to_string(),
        "environment_conventions".to_string(),
        "stdin_pipelines".to_string(),
        "diagnostic_code_explainer".to_string(),
    ];
    root.strings("features", &features);

    format!("{}\n", root.finish())
}

fn print_human_summary() {
    println!("emath version {} (capabilities v1.0.0)", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Core Capabilities:");
    println!("  • Constructor layer: object, function, recur, quote, query");
    println!("  • `emath check` / `emath run` on ordinary constructor source");
    println!("  • Scalar carriers Int, Rat, Float64, Bool; no recipe FeatureIDs");
    println!("  • Machine-readable JSON on inspection commands");
    println!("  • Structured exit codes: 0 ok, 2 admission, 3 partial, 4 fault, 5 checkpoint");
    println!("  • Extracted tokens refuse E-KIND-GONE; there is no goals: layer");
    println!("  • Environment conventions (NO_COLOR, CI, TERM=dumb)");
    println!("  • Stdin pipelines: `check -` and `fmt -` read source from stdin");
    println!();
    println!("For machine-readable JSON schema contract, run: emath capabilities --json");
}
