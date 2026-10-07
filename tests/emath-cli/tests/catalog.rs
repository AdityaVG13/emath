//! Constructor catalog: live tokens compute, extracted tokens refuse.
use emath_cli::catalog::{
    COMMANDS, EXTRACTED_COMMANDS, command_help_text, command_summary, command_usage,
    reject_unknown_flags, suggest_command,
};
use emath_cli::{CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE, run};
use emath_test_harness::{Case, Probe, check_all, expect_ok};

fn owned(argv: &[&str]) -> Vec<String> {
    argv.iter().map(|s| s.to_string()).collect()
}

#[test]
fn catalog_contract() {
    let mut p = Probe::new("constructor catalog: live tokens, extracted tokens refuse");
    p.case("exit-values", |p| {
        p.eq("ok", CliExit::Ok as u8, 0);
        p.eq("refused", CliExit::Refused as u8, 1);
        p.eq("usage", CliExit::Usage as u8, 2);
        p.eq("EXIT_OK", EXIT_OK as u8, 0);
        p.eq("EXIT_REFUSED", EXIT_REFUSED as u8, 1);
        p.eq("EXIT_USAGE", EXIT_USAGE as u8, 2);
    });
    expect_ok(check_all(
        &[
            Case::new("chek", "chek", Some("check")),
            Case::new("buld", "buld", Some("build")),
            Case::new("verson", "verson", Some("version")),
            Case::new("nonsense", "zzzzzzzz", None),
        ],
        |word| suggest_command(word),
    ));
    p.eq("core-len", COMMANDS.len(), 26);
    p.case("live-help", |p| {
        for command in COMMANDS {
            p.demand(format!("{command}-usage"), command_usage(command).is_some(), "usage");
            p.demand(format!("{command}-summary"), command_summary(command).is_some(), "summary");
            p.demand(format!("{command}-help"), command_help_text(command).is_some(), "help");
        }
        // Help must describe the codes commands really return: a
        // partial answer exits EXIT_PARTIAL, an admission error
        // EXIT_ADMISSION, a fault EXIT_FAULT, a bad checkpoint
        // EXIT_CHECKPOINT, a rejected Lean claim EXIT_REFUSED.
        let help = command_help_text("run").unwrap_or_default();
        let caps = emath_cli::catalog::capabilities_json();
        for (exit, word) in [
            (emath_cli::EXIT_REFUSED, "rejected"),
            (emath_cli::EXIT_ADMISSION, "admission"),
            (emath_cli::EXIT_PARTIAL, "partial"),
            (emath_cli::EXIT_FAULT, "fault"),
            (emath_cli::EXIT_CHECKPOINT, "checkpoint"),
        ] {
            let code = exit as u8;
            let line = help.lines().find(|line| line.trim_start().starts_with(&format!("{code} "))).unwrap_or("");
            p.demand(format!("help-exit-{code}"), line.to_lowercase().contains(word), line.to_string());
            p.demand(format!("capabilities-exit-{code}"), caps.contains(&format!("\"code\": \"{code}\"")), "listed");
        }
    });
    p.case("extracted-refuse", |p| {
        for token in [
            "eval", "sweep", "genesis", "plan", "simulate", "expand", "solve", "fit", "repl",
            "world", "portfolio", "meaning", "architecture", "agent", "provider", "fork",
        ] {
            p.demand(
                format!("listed-{token}"),
                EXTRACTED_COMMANDS.contains(&token),
                "extracted",
            );
            p.eq(format!("emath-{token}"), run(&owned(&[token, "f.emath"])), EXIT_USAGE);
        }
    });
    p.case("capabilities", |p| {
        p.eq("caps", run(&owned(&["capabilities", "--json"])), EXIT_OK);
        let body = emath_cli::catalog::capabilities_json();
        let parsed = emath_artifact::parse_json_document(&body).expect("capabilities json");
        p.eq("tool", parsed.string_field("tool").expect("tool"), "emath".to_string());
        let commands = match parsed.field("commands").expect("commands") {
            emath_artifact::JsonValue::Arr(items) => items,
            other => panic!("commands must be array, got {other:?}"),
        };
        let names: Vec<String> = commands
            .iter()
            .map(|cmd| cmd.string_field("name").expect("command name"))
            .collect();
        for must in ["check", "run", "capabilities"] {
            p.demand(format!("live-{must}"), names.iter().any(|name| name == must), "live");
        }
        for gone in ["eval", "expand", "solve", "genesis", "sweep", "simulate", "plan"] {
            p.demand(format!("gone-{gone}"), names.iter().all(|name| name != gone), "not live");
        }
        for leftover in [
            "intent_recovery",
            "next_action_engine",
            "command_aliases",
            "provable_artifacts",
            "safe_mutation_dry_run",
            "EMATH_WEB_DIST",
        ] {
            p.demand(
                format!("no-{leftover}"),
                !body.contains(leftover),
                leftover,
            );
        }
        let catalog = emath_cli::catalog::command_examples("api")
            .iter()
            .chain(emath_cli::catalog::command_examples("run"))
            .chain(emath_cli::catalog::command_examples("check"))
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        for recipe in ["integral", "optimize", "rk4", "model.emath"] {
            p.demand(
                format!("no-example-{recipe}"),
                !catalog.contains(recipe),
                catalog.clone(),
            );
        }
    });
    p.case("flags", |p| {
        for (name, command, argv, expected) in [
            ("check-raise", "check", &["--raise"][..], Some(EXIT_USAGE)),
            ("run-verify", "run", &["f.emath", "--verify"][..], Some(EXIT_USAGE)),
        ] {
            p.eq(name, reject_unknown_flags(command, &owned(argv)), expected);
        }
    });
    p.case("extracted-usage", |p| {
        for argv in [
            &["eval", "a.emath"][..],
            &["genesis", "a.emath", "--out", "d"][..],
            &["sweep", "a.emath", "--function", "F", "--grid", "x=1"][..],
            &["plan", "a.emath", "--json"][..],
            &["simulate", "a.emath", "--json"][..],
            &["world", "show", "okid", "--dir", "d"][..],
            &["fork", "status", "--dry-run"][..],
            &["provider", "inspect", "--json", "native.rust"][..],
            &["solve", "--apply", "quaternion", "f.emath"][..],
        ] {
            p.eq(argv.join(" "), run(&owned(argv)), EXIT_USAGE);
        }
    });
    p.finish();
}
