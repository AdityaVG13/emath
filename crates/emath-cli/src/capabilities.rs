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
        ("check", "Semantic admission and typecheck", "check <file.emath|-> [--verify-data] [--json]"),
        ("plan", "Deterministic resolution plan", "plan <file.emath> [--json]"),
        ("planner", "Low-level planner inspection", "planner <file.emath> [--json] [--parametric]"),
        ("build", "Generate and verify Cargo artifact", "build <file.emath> [--out <dir>] [--verify] [--bin <entry>] [--dry-run] [--json]"),
        ("simulate", "Integrate admitted ODE/DAE models", "simulate <file.emath> [--model NAME] [--dt N] [--method euler|rk4|rk45] [--json]"),
        ("new", "Deterministic project scaffold", "new <name> [--out <dir>] [--dry-run] [--force] [--json]"),
        ("fmt", "Canonical formatting and unit-preserving display", "fmt <file.emath|-> | fmt --value <literal> [--sf N] [--from UNIT]"),
        ("migrate", "Lossless receipt-driven syntax migrations", "migrate <file.emath> [--fix] [--check] [--dry-run] [--receipt <path>] [--json]"),
        ("explain", "Plan explanation, provenance DAG, or error code", "explain <file.emath> [<symbol>] | explain <E-CODE> [--list-codes] [--json]"),
        ("run", "Execute source mathematics with saved authored methods", "run <file.emath> [--function NAME] [--set name=value] [--json]"),
        ("search", "Semantic search across compiled capabilities", "search <query> [--json]"),
        ("step", "Continue execution with single work-unit commits", "step <checkpoint.json> [--work N] [--json]"),
        ("api", "Report command interface and language distribution", "api [--search text] [--source file.emath] [--json]"),
        ("test", "Build and verify test fixtures", "test <file.emath> [--out <dir>]"),
        ("verify", "Check published artifacts or saved certificates", "verify <dir|checkpoint.json> [--json]"),
        ("inspect", "Read saved mathematical results or manifests", "inspect <dir|checkpoint.json> [--json]"),
        ("diff", "Content-id fingerprint comparison", "diff <a.emath> <b.emath> [--json]"),
        ("doctor", "Toolchain presence and environment health", "doctor [--json]"),
        ("capabilities", "Machine-readable contract and capabilities export", "capabilities [--json]"),
        ("catalog", "Full command matrix export with flags and examples", "catalog [--json]"),
        ("triage", "Mega-command to orient, check health, and get recommendations", "triage [<file.emath>] [--json]"),
        ("next", "Next-action engine returning top action and claim command", "next [<file.emath>] [--json]"),
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
        ("0", "ok - command completed successfully"),
        ("1", "refused - admission, verification, or mathematical refusal"),
        ("2", "usage - invalid arguments, unknown flag, or syntax error"),
        ("3", "environment - missing toolchain or broken environment"),
        ("4", "io - filesystem or file access error"),
        ("5", "safety - destructive or unguarded operation blocked"),
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
        ("EMATH_WEB_DIST", "Override path to web playground assets"),
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
        "json_streaming".to_string(),
        "deterministic_builds".to_string(),
        "pure_stdout_stderr_separation".to_string(),
        "robot_mode".to_string(),
        "intent_recovery".to_string(),
        "command_aliases".to_string(),
        "environment_conventions".to_string(),
        "ansi_color_control".to_string(),
        "next_action_engine".to_string(),
        "provable_artifacts".to_string(),
        "safe_mutation_dry_run".to_string(),
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
    println!("  • Deterministic mathematical compilation (EMIR -> Cargo artifact)");
    println!("  • Explicit & adaptive numerical solvers (Euler, RK4, RK45)");
    println!("  • Typechecked dimensional analysis & unit preservation");
    println!("  • Machine-readable JSON streaming on all inspection commands");
    println!("  • Structured exit code contracts and error pedagogy");
    println!("  • Single-letter and intuitive command aliases (c, b, p, sim, doc, fmt)");
    println!("  • Environment conventions (NO_COLOR, CI, TERM=dumb, --color control)");
    println!("  • Stdin pipelines: `check -` and `fmt -` read source from stdin");
    println!();
    println!("For machine-readable JSON schema contract, run: emath capabilities --json");
}
