//! Catalog behavioral tests, moved from `crates/emath-cli/src/catalog.rs`.
//!
//! `jason_suggests_json` was dropped: it asserts the private
//! `catalog::suggest_flag`, which the public API does not expose.

use emath_cli::catalog::{
    capabilities_json, command_help_text, command_summary, command_usage, flags_for,
    reject_unknown_flags, robot_docs_guide, suggest_command, COMMANDS,
};
use emath_cli::{architecture_json, run, CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE};

#[test]
fn cli_exit_is_three_way() {
    assert_eq!(CliExit::Ok as u8, 0);
    assert_eq!(CliExit::Refused as u8, 1);
    assert_eq!(CliExit::Usage as u8, 2);
    let lock = |e: CliExit| match e {
        CliExit::Ok | CliExit::Refused | CliExit::Usage => e as u8,
    };
    assert_eq!(lock(CliExit::Ok), 0);
    assert_eq!(lock(EXIT_OK), 0);
    assert_eq!(lock(EXIT_REFUSED), 1);
    assert_eq!(lock(EXIT_USAGE), 2);
}

#[test]
fn typo_chek_suggests_check() {
    assert_eq!(suggest_command("chek"), Some("check"));
}

#[test]
fn typo_buld_suggests_build() {
    assert_eq!(suggest_command("buld"), Some("build"));
}

#[test]
fn typo_verson_suggests_version() {
    assert_eq!(suggest_command("verson"), Some("version"));
}

#[test]
fn nonsense_has_no_suggestion() {
    assert_eq!(suggest_command("zzzzzzzz"), None);
}

#[test]
fn every_catalogued_command_has_usage_and_summary() {
    for command in COMMANDS {
        assert!(command_usage(command).is_some(), "{command} usage");
        assert!(command_summary(command).is_some(), "{command} summary");
        assert!(command_help_text(command).is_some(), "{command} help");
    }
}

#[test]
fn capabilities_json_names_schema_and_exit_codes() {
    let body = capabilities_json();
    let parsed = emath_artifact::parse_json_document(&body).expect("capabilities json");
    assert_eq!(
        parsed.string_field("schema").expect("schema"),
        "emath.capabilities"
    );
    assert_eq!(parsed.string_field("tool").expect("tool"), "emath");
    assert!(!parsed.string_field("version").expect("version").is_empty());
    assert!(!parsed
        .string_field("contract")
        .expect("contract")
        .is_empty());
    let codes = parsed.field("exit_codes").expect("exit_codes");
    assert_eq!(codes.string_field("0").expect("0"), "ok");
    assert_eq!(
        codes.string_field("1").expect("1"),
        "refused or admission/build diagnostics"
    );
    assert_eq!(codes.string_field("2").expect("2"), "usage or io error");
    match parsed.field("env_vars").expect("env_vars") {
        emath_artifact::JsonValue::Arr(_) => {}
        other => panic!("env_vars must be array, got {other:?}"),
    }
    let commands = match parsed.field("commands").expect("commands") {
        emath_artifact::JsonValue::Arr(items) => items,
        other => panic!("commands must be array, got {other:?}"),
    };
    let names: Vec<String> = commands
        .iter()
        .map(|cmd| cmd.string_field("name").expect("command name"))
        .collect();
    assert_eq!(COMMANDS.len(), 46, "{COMMANDS:?}");
    assert_eq!(names.len(), 46, "{names:?}");
    assert_eq!(names.len(), COMMANDS.len(), "{names:?}");
    for must in ["check", "expand", "solve", "exactness", "freeze"] {
        assert!(
            COMMANDS.contains(&must),
            "{must} missing from COMMANDS {COMMANDS:?}"
        );
        assert!(
            names.iter().any(|name| name == must),
            "{must} missing from capabilities commands {names:?}"
        );
    }
    for command in COMMANDS {
        assert!(
            names.iter().any(|name| name == *command),
            "{command} missing from capabilities commands {names:?}"
        );
    }
    for cmd in commands {
        let name = cmd.string_field("name").expect("name");
        let usage = cmd.string_field("usage").expect("usage");
        assert!(!usage.is_empty(), "{name} usage");
        let _ = cmd.string_field("summary").expect("summary");
    }
}

#[test]
fn architecture_json_schema_and_required_paths_parse_back() {
    let body = architecture_json();
    let parsed = emath_artifact::parse_json_document(&body).expect("architecture json");
    assert_eq!(
        parsed.string_field("schema").expect("schema"),
        "emath.architecture"
    );
    assert!(!parsed
        .string_field("pipeline")
        .expect("pipeline")
        .is_empty());
    match parsed.field("required_paths").expect("required_paths") {
        emath_artifact::JsonValue::Arr(items) => {
            assert!(!items.is_empty(), "required_paths must not be empty");
            for item in items {
                match item {
                    emath_artifact::JsonValue::Str(path) => assert!(!path.is_empty(), "{path:?}"),
                    other => panic!("required_paths item must be string, got {other:?}"),
                }
            }
        }
        other => panic!("required_paths must be array, got {other:?}"),
    }
}

#[test]
fn robot_docs_names_capabilities() {
    let body = robot_docs_guide();
    assert!(body.contains("emath capabilities --json"), "{body}");
    assert!(body.contains("Exit codes"), "{body}");
}

#[test]
fn flags_for_matches_implemented_usage() {
    assert!(flags_for("build").contains(&"--verify"));
    assert!(flags_for("build").contains(&"--json"));
    assert!(!flags_for("run").contains(&"--json"));
    assert!(!flags_for("run").contains(&"--verify"));
    assert!(flags_for("run").contains(&"--out"));
    assert!(!flags_for("test").contains(&"--json"));
    assert!(!flags_for("new").contains(&"--verify"));
    assert!(!flags_for("vendor").contains(&"--json"));
    assert!(flags_for("exactness").contains(&"--raise"));
    assert!(!flags_for("check").contains(&"--raise"));
    assert!(!flags_for("freeze").contains(&"--raise"));
    assert!(!flags_for("solve").contains(&"--raise"));
    assert_eq!(
        reject_unknown_flags("check", &["--raise".into()]),
        Some(EXIT_USAGE)
    );
    assert_eq!(
        run(&[
            "check".into(),
            "f.emath".into(),
            "--raise".into(),
            "units".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "expand".into(),
            "f.emath".into(),
            "--raise".into(),
            "units".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "solve".into(),
            "--check".into(),
            "f.emath".into(),
            "--raise".into(),
            "units".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "exactness".into(),
            "f.emath".into(),
            "--raise".into(),
            "evidence".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "exactness".into(),
            "f.emath".into(),
            "--raise".into(),
            "units".into(),
            "--raise".into(),
            "units".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        reject_unknown_flags("run", &["f.emath".into(), "--verify".into()]),
        Some(EXIT_USAGE)
    );
    assert_eq!(
        reject_unknown_flags("vendor", &["--out".into(), "d".into(), "--json".into()]),
        Some(EXIT_USAGE)
    );
}

#[test]
fn value_taking_flag_at_eol_is_usage_not_silent_ok() {
    assert_eq!(
        reject_unknown_flags("agent", &["build".into(), "f.emath".into(), "--out".into()]),
        Some(EXIT_USAGE)
    );
    assert_eq!(
        reject_unknown_flags("simulate", &["f.emath".into(), "--dt".into()]),
        Some(EXIT_USAGE)
    );
    assert_eq!(
        reject_unknown_flags("web", &["--port".into()]),
        Some(EXIT_USAGE)
    );
    assert_eq!(
        reject_unknown_flags("eval", &["f.emath".into(), "--world".into()]),
        Some(EXIT_USAGE)
    );
    assert_eq!(
        reject_unknown_flags(
            "agent",
            &["build".into(), "f.emath".into(), "--out".into(), "d".into()]
        ),
        None
    );
}

#[test]
fn malformed_numeric_flags_are_usage_not_silent_default() {
    assert_eq!(
        run(&[
            "simulate".into(),
            "f.emath".into(),
            "--dt".into(),
            "nan".into()
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "simulate".into(),
            "f.emath".into(),
            "--dt".into(),
            "0".into()
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["web".into(), "--port".into(), "abc".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["web".into(), "--port".into(), "0".into()]),
        EXIT_USAGE
    );
}

#[test]
fn leftover_parser_refuses_catalog_allowed_flag_on_wrong_subcommand() {
    // flags_for(agent) admits --out, but only agent build consumes it.
    assert_eq!(
        run(&[
            "agent".into(),
            "check".into(),
            "f.emath".into(),
            "--out".into(),
            "d".into(),
        ]),
        EXIT_USAGE
    );
    // flags_for(meaning) is a union; list does not consume --world.
    assert_eq!(
        run(&[
            "meaning".into(),
            "list".into(),
            "--world".into(),
            "w".into()
        ]),
        EXIT_USAGE
    );
    // flags_for(meaning) admits --json; set does not consume it.
    assert_eq!(
        run(&[
            "meaning".into(),
            "set".into(),
            "f.emath".into(),
            "--world".into(),
            "w".into(),
            "--json".into(),
        ]),
        EXIT_USAGE
    );
}

#[test]
fn catalog_json_flags_are_not_stolen_as_ids() {
    // --json is catalog-legal for provider; must not become the inspect id.
    assert_eq!(
        run(&[
            "provider".into(),
            "inspect".into(),
            "--json".into(),
            "native.rust".into(),
        ]),
        EXIT_OK
    );
    // --json on provider test is honored as JSON refusal, not Usage.
    assert_eq!(
        run(&[
            "provider".into(),
            "test".into(),
            "native.rust".into(),
            "--json".into(),
        ]),
        EXIT_REFUSED
    );
    // Extra provider positionals fail closed (list foo used to swallow).
    assert_eq!(
        run(&["provider".into(), "list".into(), "native.rust".into()]),
        EXIT_USAGE
    );
}

#[test]
fn fork_sync_json_is_not_ignored() {
    assert_eq!(
        run(&["fork".into(), "sync".into(), "--json".into()]),
        EXIT_REFUSED
    );
    assert_eq!(
        run(&[
            "fork".into(),
            "sync".into(),
            "--dry-run".into(),
            "--json".into(),
        ]),
        EXIT_OK
    );
}

#[test]
fn web_extra_positional_does_not_keep_default_port() {
    assert_eq!(run(&["web".into(), "8080".into()]), EXIT_USAGE);
    assert_eq!(run(&["serve".into(), "8080".into()]), EXIT_USAGE);
}

#[test]
fn leftover_parser_refuses_world_show_without_id() {
    assert_eq!(
        run(&["world".into(), "show".into(), "--dir".into(), "d".into(),]),
        EXIT_USAGE
    );
}

#[test]
fn leftover_parser_refuses_extra_positionals() {
    assert_eq!(
        run(&["fmt".into(), "a.emath".into(), "b.emath".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["new".into(), "pkg".into(), "extra".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "explain".into(),
            "f.emath".into(),
            "sym".into(),
            "extra".into()
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "agent".into(),
            "check".into(),
            "f.emath".into(),
            "g.emath".into()
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "check".into(),
            "a.emath".into(),
            "b.emath".into(),
            "--json".into()
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["eval".into(), "a.emath".into(), "b.emath".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["repl".into(), "a.emath".into(), "b.emath".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["simulate".into(), "a.emath".into(), "b.emath".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "eval".into(),
            "a.emath".into(),
            "--world".into(),
            "one_point".into(),
            "--world".into(),
            "free_symbolic".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "meaning".into(),
            "set".into(),
            "a.emath".into(),
            "--world".into(),
            "one_point".into(),
            "--world".into(),
            "free_symbolic".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "meaning".into(),
            "list".into(),
            "--dir".into(),
            "a".into(),
            "--dir".into(),
            "b".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "simulate".into(),
            "a.emath".into(),
            "--dt".into(),
            "0.1".into(),
            "--dt".into(),
            "0.2".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "web".into(),
            "--port".into(),
            "8080".into(),
            "--port".into(),
            "9090".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "web".into(),
            "--dist".into(),
            "a".into(),
            "--dist".into(),
            "b".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "meaning".into(),
            "set".into(),
            "a.emath".into(),
            "b.emath".into(),
            "--world".into(),
            "one_point".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["meaning".into(), "list".into(), "extra".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["vendor".into(), "--out".into(), "d".into(), "extra".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "vendor".into(),
            "--out".into(),
            "d".into(),
            "--out".into(),
            "e".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "artifact".into(),
            "check".into(),
            "d".into(),
            "extra".into()
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "artifact".into(),
            "battery".into(),
            "d".into(),
            "extra".into()
        ]),
        EXIT_USAGE
    );
    assert_eq!(run(&["architecture".into(), "extra".into()]), EXIT_USAGE);
    assert_eq!(run(&["architecture".into(), "--json".into()]), EXIT_OK);
    assert_eq!(run(&["capabilities".into(), "extra".into()]), EXIT_USAGE);
    assert_eq!(run(&["capabilities".into(), "--json".into()]), EXIT_OK);
    assert_eq!(run(&["doctor".into(), "extra".into()]), EXIT_USAGE);
    assert_eq!(
        run(&["inspect".into(), "d".into(), "extra".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "why".into(),
            "a.emath".into(),
            "inference:1".into(),
            "extra".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "import".into(),
            "modelica".into(),
            "a.mo".into(),
            "b.mo".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "compile".into(),
            "--parametric".into(),
            "a.emath".into(),
            "--out".into(),
            "d".into(),
            "extra".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "compile".into(),
            "--parametric".into(),
            "a.emath".into(),
            "--out".into(),
            "d".into(),
            "--out".into(),
            "e".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "world".into(),
            "show".into(),
            "WID".into(),
            "--dir".into(),
            "d".into(),
            "extra".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "portfolio".into(),
            "show".into(),
            "PID".into(),
            "--dir".into(),
            "d".into(),
            "--dir".into(),
            "e".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "genesis".into(),
            "a.emath".into(),
            "--out".into(),
            "d".into(),
            "--out".into(),
            "e".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "build".into(),
            "a.emath".into(),
            "--out".into(),
            "d".into(),
            "--out".into(),
            "e".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "agent".into(),
            "build".into(),
            "a.emath".into(),
            "--out".into(),
            "d".into(),
            "--out".into(),
            "e".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "solve".into(),
            "--apply".into(),
            "one_point".into(),
            "a.emath".into(),
            "--apply".into(),
            "free_symbolic".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["fork".into(), "status".into(), "--dry-run".into()]),
        EXIT_OK
    );
    assert_eq!(
        run(&[
            "robot-docs".into(),
            "guide".into(),
            "extra".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(run(&["version".into(), "extra".into()]), EXIT_USAGE);
    assert_eq!(
        run(&["help".into(), "check".into(), "extra".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["web".into(), "--".into(), "extra".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["serve".into(), "--".into(), "extra".into()]),
        EXIT_USAGE
    );
}

#[test]
fn world_and_portfolio_show_refuse_path_traversal_ids() {
    // Bait file at dir/x.json == world-candidates/../x.json. Missing-file
    // also returns Usage, so a readable escape target is required.
    let dir = std::env::temp_dir().join(format!(
        "emath-cli-world-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let candidates = dir.join("world-candidates");
    std::fs::create_dir_all(&candidates).expect("temp dir");
    std::fs::write(dir.join("x.json"), "{\"ok\":true}\n").expect("escape bait");
    std::fs::write(candidates.join("okid.json"), "{\"ok\":true}\n").expect("confined");
    // Default portfolio file is the second candidate. Without it, a dead
    // confinement check for `../x` is indistinguishable from missing-file Usage.
    std::fs::write(dir.join("interpretation-portfolio.json"), "{\"ok\":true}\n")
        .expect("portfolio default bait");
    std::fs::write(
        dir.join("interpretation-portfolio-okid.json"),
        "{\"ok\":true}\n",
    )
    .expect("confined portfolio");
    let dir_s = dir.display().to_string();
    assert_eq!(
        run(&[
            "world".into(),
            "show".into(),
            "okid".into(),
            "--dir".into(),
            dir_s.clone(),
        ]),
        EXIT_OK,
        "confined id must succeed so ../x Usage is confinement, not IO"
    );
    assert_eq!(
        run(&[
            "portfolio".into(),
            "show".into(),
            "okid".into(),
            "--dir".into(),
            dir_s.clone(),
        ]),
        EXIT_OK,
        "confined portfolio id must succeed so ../x Usage is confinement, not IO"
    );
    assert_eq!(
        run(&[
            "world".into(),
            "show".into(),
            "../x".into(),
            "--dir".into(),
            dir_s.clone(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "world".into(),
            "show".into(),
            "../secret".into(),
            "--dir".into(),
            dir_s.clone(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "world".into(),
            "show".into(),
            "/etc/passwd".into(),
            "--dir".into(),
            dir_s.clone(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "portfolio".into(),
            "show".into(),
            "../x".into(),
            "--dir".into(),
            dir_s,
        ]),
        EXIT_USAGE
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn artifact_check_and_battery_refuse_missing_dir() {
    let missing = std::env::temp_dir().join(format!(
        "emath-no-artifact-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    assert!(!missing.exists());
    let path = missing.display().to_string();
    assert_eq!(
        run(&["artifact".into(), "check".into(), path.clone()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["artifact".into(), "battery".into(), path.clone()]),
        EXIT_USAGE
    );
    assert_eq!(run(&["verify".into(), path]), EXIT_USAGE);
}

#[test]
fn freeze_and_build_out_missing_or_flag_value_is_usage() {
    assert_eq!(
        run(&["freeze".into(), "f.emath".into(), "--out".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&["build".into(), "f.emath".into(), "--out".into()]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "freeze".into(),
            "f.emath".into(),
            "--out".into(),
            "--json".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "build".into(),
            "f.emath".into(),
            "--out".into(),
            "--verify".into(),
        ]),
        EXIT_USAGE
    );
}

#[test]
fn missing_path_is_usage_unknown_solve_label_is_refused() {
    assert_eq!(run(&["freeze".into()]), EXIT_USAGE);
    assert_eq!(run(&["solve".into(), "--check".into()]), EXIT_USAGE);
    assert_eq!(
        run(&["expand".into(), "--json".into()]),
        EXIT_USAGE,
        "no file operand stays usage even with --json"
    );
    let missing = std::env::temp_dir().join(format!(
        "emath-cli-catalog-missing-{}-{}.emath",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    assert_eq!(
        run(&[
            "expand".into(),
            missing.display().to_string(),
            "--json".into()
        ]),
        EXIT_USAGE,
        "provided missing file --json is IO usage with stdout envelope"
    );
    assert_eq!(
        run(&[
            "solve".into(),
            "--apply".into(),
            "--check".into(),
            "f.emath".into(),
        ]),
        EXIT_USAGE
    );
    assert_eq!(
        run(&[
            "solve".into(),
            "--apply".into(),
            "quaternion".into(),
            "f.emath".into(),
        ]),
        EXIT_REFUSED
    );
}

#[test]
fn agent_propose_duplicate_scalar_keys_are_usage() {
    let dir = std::env::temp_dir().join(format!(
        "emath-propose-dup-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let body = "\
problem: p1
kind: world-delta
base: 0
change: law|sym|desc|prov
obligation: 1|scope|prov
exec: lowering|f64|native|host|now
exec: lowering|f64|native|host|later
";
    let path = dir.join("proposal.txt");
    std::fs::write(&path, body).expect("proposal");
    assert_eq!(
        run(&["agent".into(), "propose".into(), path.display().to_string(),]),
        EXIT_USAGE
    );
    let once = dir.join("once.txt");
    std::fs::write(
        &once,
        "\
problem: p1
kind: world-delta
base: 0
change: law|sym|desc|prov
obligation: 1|scope|prov
exec: lowering|f64|native|host|now
",
    )
    .expect("once");
    assert_ne!(
        run(&["agent".into(), "propose".into(), once.display().to_string(),]),
        EXIT_USAGE,
        "a single `exec:` must parse; schema admission is not argv shape"
    );
}

#[test]
fn simulate_json_without_model_is_refused() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../language/examples/intro/hello-square.emath");
    assert_eq!(
        run(&[
            "simulate".into(),
            path.display().to_string(),
            "--json".into(),
        ]),
        EXIT_REFUSED,
        "admitted function pane is not a model; --json must still refuse"
    );
}

#[test]
fn agent_plan_and_build_missing_source_are_not_ok() {
    let missing = std::env::temp_dir().join(format!(
        "emath-agent-missing-{}-{}.emath",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let path = missing.display().to_string();
    assert_eq!(
        run(&["agent".into(), "plan".into(), path.clone()]),
        EXIT_USAGE,
        "agent plan missing file is usage, with JSON on stdout"
    );
    assert_eq!(
        run(&["agent".into(), "build".into(), path.clone()]),
        EXIT_USAGE,
        "agent build missing file is usage, with JSON on stdout"
    );
    assert_eq!(
        run(&["agent".into(), "propose".into(), path]),
        EXIT_USAGE,
        "agent propose missing file is usage, with JSON on stdout"
    );
}

#[test]
fn explain_provenance_json_empty_is_refused() {
    let dir = std::env::temp_dir().join(format!(
        "emath-prov-empty-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let empty = dir.join("empty.emath");
    std::fs::write(&empty, "").expect("empty");
    assert_eq!(
        run(&[
            "explain".into(),
            empty.display().to_string(),
            "--provenance".into(),
            "--json".into(),
        ]),
        EXIT_REFUSED,
        "explain --provenance --json must refuse empty source"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
