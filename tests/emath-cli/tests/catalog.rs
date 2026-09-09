//! Catalog behavioral tests, moved from `crates/emath-cli/src/catalog.rs`.
use emath_cli::catalog::{command_help_text, command_summary, command_usage, reject_unknown_flags, suggest_command};
use emath_cli::{CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE};
use emath_cli_lab::catalog::{COMMANDS, capabilities_json, robot_docs_guide};
use emath_cli_lab::{architecture_json, run};
use emath_test_harness::{Case, Probe, check_all, expect_ok};

fn owned(argv: &[&str]) -> Vec<String> {
    argv.iter().map(|s| s.to_string()).collect()
}

#[test]
fn catalog_contract() {
    let mut p = Probe::new("catalog exit codes, suggestions, and capability JSON shape");
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
    p.eq("extracted-eval", emath_cli::run(&["eval".into(), "f.emath".into()]), EXIT_USAGE);
    p.case("usage-present", |p| {
        for command in COMMANDS {
            p.demand(format!("{command}-usage"), command_usage(command).is_some(), "usage");
            p.demand(format!("{command}-summary"), command_summary(command).is_some(), "summary");
            p.demand(format!("{command}-help"), command_help_text(command).is_some(), "help");
        }
    });
    p.case("capabilities", |p| {
        let body = capabilities_json();
        let parsed = emath_artifact::parse_json_document(&body).expect("capabilities json");
        p.eq("schema", parsed.string_field("schema").expect("schema"), "emath.capabilities".to_string());
        p.eq("tool", parsed.string_field("tool").expect("tool"), "emath".to_string());
        p.demand("version", !parsed.string_field("version").expect("version").is_empty(), "version");
        p.demand("contract", !parsed.string_field("contract").expect("contract").is_empty(), "contract");
        let codes = parsed.field("exit_codes").expect("exit_codes");
        p.eq("code-0", codes.string_field("0").expect("0"), "ok".to_string());
        p.eq("code-1", codes.string_field("1").expect("1"), "refused or admission/build diagnostics".to_string());
        p.eq("code-2", codes.string_field("2").expect("2"), "usage or io error".to_string());
        p.demand("env-arr", matches!(parsed.field("env_vars"), Ok(emath_artifact::JsonValue::Arr(_))), "env_vars array");
        let commands = match parsed.field("commands").expect("commands") {
            emath_artifact::JsonValue::Arr(items) => items,
            other => panic!("commands must be array, got {other:?}"),
        };
        let names: Vec<String> = commands.iter().map(|cmd| cmd.string_field("name").expect("command name")).collect();
        p.eq("commands-len", COMMANDS.len(), 47);
        p.eq("names-len", names.len(), 47);
        for must in ["check", "expand", "solve", "exactness", "freeze"] {
            p.demand(format!("catalog-{must}"), COMMANDS.contains(&must), "in COMMANDS");
            p.demand(format!("names-{must}"), names.iter().any(|name| name == must), "in capabilities");
        }
        for command in COMMANDS {
            p.demand(format!("covered-{command}"), names.iter().any(|name| name == *command), "covered");
        }
        for cmd in commands {
            let name = cmd.string_field("name").expect("name");
            p.demand(format!("{name}-usage"), !cmd.string_field("usage").expect("usage").is_empty(), "usage");
            let _ = cmd.string_field("summary").expect("summary");
        }
    });
    p.case("architecture", |p| {
        let body = architecture_json();
        let parsed = emath_artifact::parse_json_document(&body).expect("architecture json");
        p.eq("schema", parsed.string_field("schema").expect("schema"), "emath.architecture".to_string());
        p.demand("pipeline", !parsed.string_field("pipeline").expect("pipeline").is_empty(), "pipeline");
        match parsed.field("required_paths").expect("required_paths") {
            emath_artifact::JsonValue::Arr(items) => {
                p.demand("nonempty", !items.is_empty(), "required_paths");
                for item in items {
                    match item {
                        emath_artifact::JsonValue::Str(path) => { p.demand("path", !path.is_empty(), "path"); }
                        other => { p.fail("path-type", format!("required_paths item must be string, got {other:?}")); }
                    }
                }
            }
            other => { p.fail("required-paths", format!("required_paths must be array, got {other:?}")); }
        }
    });
    p.case("robot-docs", |p| {
        let body = robot_docs_guide();
        p.demand("capabilities", body.contains("emath-lab capabilities --json") || body.contains("emath capabilities --json"), "names capabilities");
        p.contains("exit-codes", &body, "Exit codes");
    });
    p.case("flags", |p| {
        for (name, command, argv, expected) in [
            ("check-raise", "check", &["--raise"][..], Some(EXIT_USAGE)),
            ("run-verify", "run", &["f.emath", "--verify"][..], Some(EXIT_USAGE)),
            ("vendor-json", "vendor", &["--out", "d", "--json"][..], Some(EXIT_USAGE)),
            ("agent-out-eol", "agent", &["build", "f.emath", "--out"][..], Some(EXIT_USAGE)),
            ("simulate-dt-eol", "simulate", &["f.emath", "--dt"][..], Some(EXIT_USAGE)),
            ("web-port-eol", "web", &["--port"][..], Some(EXIT_USAGE)),
            ("eval-world-eol", "eval", &["f.emath", "--world"][..], Some(EXIT_USAGE)),
            ("agent-out-ok", "agent", &["build", "f.emath", "--out", "d"][..], None),
        ] {
            p.eq(name, reject_unknown_flags(command, &owned(argv)), expected);
        }
    });
    p.case("run-usage", |p| {
        let rows: &[&[&str]] = &[
            &["check", "f.emath", "--raise", "units"], &["expand", "f.emath", "--raise", "units"],
            &["solve", "--check", "f.emath", "--raise", "units"], &["exactness", "f.emath", "--raise", "evidence"],
            &["exactness", "f.emath", "--raise", "units", "--raise", "units"],
            &["simulate", "f.emath", "--dt", "nan"], &["simulate", "f.emath", "--dt", "0"],
            &["web", "--port", "abc"], &["web", "--port", "0"],
            &["agent", "check", "f.emath", "--out", "d"], &["meaning", "list", "--world", "w"],
            &["meaning", "set", "f.emath", "--world", "w", "--json"],
            &["web", "8080"], &["serve", "8080"], &["world", "show", "--dir", "d"],
            &["fmt", "a.emath", "b.emath"], &["new", "pkg", "extra"],
            &["explain", "f.emath", "sym", "extra"], &["agent", "check", "f.emath", "g.emath"],
            &["check", "a.emath", "b.emath", "--json"], &["eval", "a.emath", "b.emath"],
            &["repl", "a.emath", "b.emath"], &["simulate", "a.emath", "b.emath"],
            &["eval", "a.emath", "--world", "one_point", "--world", "free_symbolic"],
            &["meaning", "set", "a.emath", "--world", "one_point", "--world", "free_symbolic"],
            &["meaning", "list", "--dir", "a", "--dir", "b"],
            &["simulate", "a.emath", "--dt", "0.1", "--dt", "0.2"],
            &["web", "--port", "8080", "--port", "9090"], &["web", "--dist", "a", "--dist", "b"],
            &["meaning", "set", "a.emath", "b.emath", "--world", "one_point"],
            &["meaning", "list", "extra"], &["vendor", "--out", "d", "extra"],
            &["vendor", "--out", "d", "--out", "e"], &["artifact", "check", "d", "extra"],
            &["artifact", "battery", "d", "extra"], &["architecture", "extra"],
            &["capabilities", "extra"], &["doctor", "extra"], &["inspect", "d", "extra"],
            &["why", "a.emath", "inference:1", "extra"], &["import", "modelica", "a.mo", "b.mo"],
            &["compile", "--parametric", "a.emath", "--out", "d", "extra"],
            &["compile", "--parametric", "a.emath", "--out", "d", "--out", "e"],
            &["world", "show", "WID", "--dir", "d", "extra"],
            &["portfolio", "show", "PID", "--dir", "d", "--dir", "e"],
            &["genesis", "a.emath", "--out", "d", "--out", "e"],
            &["build", "a.emath", "--out", "d", "--out", "e"],
            &["agent", "build", "a.emath", "--out", "d", "--out", "e"],
            &["solve", "--apply", "one_point", "a.emath", "--apply", "free_symbolic"],
            &["robot-docs", "guide", "extra"], &["version", "extra"],
            &["help", "check", "extra"], &["web", "--", "extra"], &["serve", "--", "extra"],
            &["freeze", "f.emath", "--out"], &["build", "f.emath", "--out"],
            &["freeze", "f.emath", "--out", "--json"], &["build", "f.emath", "--out", "--verify"],
            &["freeze"], &["solve", "--check"], &["expand", "--json"],
            &["solve", "--apply", "--check", "f.emath"],
            &["provider", "list", "native.rust"],
        ];
        for (i, argv) in rows.iter().enumerate() {
            p.eq(format!("usage-{i}"), run(&owned(argv)), EXIT_USAGE);
        }
        for argv in [&["architecture", "--json"][..], &["capabilities", "--json"][..], &["fork", "status", "--dry-run"][..]] {
            p.eq(argv.join(" "), run(&owned(argv)), EXIT_OK);
        }
        p.eq("provider-inspect-json", run(&owned(&["provider", "inspect", "--json", "native.rust"])), EXIT_OK);
        p.eq("provider-test-json", run(&owned(&["provider", "test", "native.rust", "--json"])), EXIT_REFUSED);
        p.eq("fork-sync-json", run(&owned(&["fork", "sync", "--json"])), EXIT_REFUSED);
        p.eq("fork-sync-dry-json", run(&owned(&["fork", "sync", "--dry-run", "--json"])), EXIT_OK);
        p.eq("solve-unknown-label", run(&owned(&["solve", "--apply", "quaternion", "f.emath"])), EXIT_REFUSED);
    });
    p.case("world-traversal", |p| {
        let dir = std::env::temp_dir().join(format!("emath-cli-world-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
        let candidates = dir.join("world-candidates");
        std::fs::create_dir_all(&candidates).expect("temp dir");
        std::fs::write(dir.join("x.json"), "{\"ok\":true}\n").expect("escape bait");
        std::fs::write(candidates.join("okid.json"), "{\"ok\":true}\n").expect("confined");
        std::fs::write(dir.join("interpretation-portfolio.json"), "{\"ok\":true}\n").expect("portfolio bait");
        std::fs::write(dir.join("interpretation-portfolio-okid.json"), "{\"ok\":true}\n").expect("confined portfolio");
        let dir_s = dir.display().to_string();
        p.eq("world-ok", run(&owned(&["world", "show", "okid", "--dir", &dir_s])), EXIT_OK);
        p.eq("portfolio-ok", run(&owned(&["portfolio", "show", "okid", "--dir", &dir_s])), EXIT_OK);
        for (name, argv) in [
            ("world-dotdot", vec!["world".to_string(), "show".to_string(), "../x".to_string(), "--dir".to_string(), dir_s.clone()]),
            ("world-secret", vec!["world".to_string(), "show".to_string(), "../secret".to_string(), "--dir".to_string(), dir_s.clone()]),
            ("world-abs", vec!["world".to_string(), "show".to_string(), "/etc/passwd".to_string(), "--dir".to_string(), dir_s.clone()]),
            ("portfolio-dotdot", vec!["portfolio".to_string(), "show".to_string(), "../x".to_string(), "--dir".to_string(), dir_s.clone()]),
        ] {
            p.eq(name, run(&argv), EXIT_USAGE);
        }
        let _ = std::fs::remove_dir_all(&dir);
    });
    p.case("missing-io", |p| {
        let missing = std::env::temp_dir().join(format!("emath-no-artifact-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
        let path = missing.display().to_string();
        for (name, argv) in [
            ("artifact-check", vec!["artifact".to_string(), "check".to_string(), path.clone()]),
            ("artifact-battery", vec!["artifact".to_string(), "battery".to_string(), path.clone()]),
            ("verify", vec!["verify".to_string(), path.clone()]),
        ] {
            p.eq(name, run(&argv), EXIT_USAGE);
        }
        let missing_emath = std::env::temp_dir().join(format!("emath-cli-catalog-missing-{}-{}.emath", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
        p.eq("expand-missing-json", run(&owned(&["expand", &missing_emath.display().to_string(), "--json"])), EXIT_USAGE);
        let agent_missing = std::env::temp_dir().join(format!("emath-agent-missing-{}-{}.emath", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
        let agent_s = agent_missing.display().to_string();
        for (name, argv) in [
            ("agent-plan", vec!["agent".to_string(), "plan".to_string(), agent_s.clone()]),
            ("agent-build", vec!["agent".to_string(), "build".to_string(), agent_s.clone()]),
            ("agent-propose-missing", vec!["agent".to_string(), "propose".to_string(), agent_s.clone()]),
        ] {
            p.eq(name, run(&argv), EXIT_USAGE);
        }
    });
    p.case("agent-propose", |p| {
        let dir = std::env::temp_dir().join(format!("emath-propose-dup-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let dup = dir.join("proposal.txt");
        std::fs::write(&dup, "problem: p1\nkind: world-delta\nbase: 0\nchange: law|sym|desc|prov\nobligation: 1|scope|prov\nexec: lowering|f64|native|host|now\nexec: lowering|f64|native|host|later\n").expect("proposal");
        let dup_argv = vec!["agent".to_string(), "propose".to_string(), dup.display().to_string()];
        p.eq("dup-exec", run(&dup_argv), EXIT_USAGE);
        let once = dir.join("once.txt");
        std::fs::write(&once, "problem: p1\nkind: world-delta\nbase: 0\nchange: law|sym|desc|prov\nobligation: 1|scope|prov\nexec: lowering|f64|native|host|now\n").expect("once");
        let argv: Vec<String> = vec!["agent".to_string(), "propose".to_string(), once.display().to_string()];
        p.ne("single-exec", run(&argv), EXIT_USAGE);
        let _ = std::fs::remove_dir_all(&dir);
    });
    p.case("model-and-provenance", |p| {
        let hello = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/language/intro/hello-square.emath");
        let hello_argv = vec!["simulate".to_string(), hello.display().to_string(), "--json".to_string()];
        p.eq("simulate-no-model", run(&hello_argv), EXIT_REFUSED);
        let dir = std::env::temp_dir().join(format!("emath-prov-empty-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let empty = dir.join("empty.emath");
        std::fs::write(&empty, "").expect("empty");
        p.eq("explain-empty", run(&owned(&["explain", &empty.display().to_string(), "--provenance", "--json"])), EXIT_REFUSED);
        let _ = std::fs::remove_dir_all(&dir);
    });
    p.finish();
}


#[test]
fn typed_cli_vector_extent() {
    use emath_cli::execution::parse_set_value_for;
    use emath_exec_ir::interp::Value;
    use emath_ir::{Extent, TypeNode};

    let declared = TypeNode::Vector {
        element: Box::new(TypeNode::Float64),
        extent: Some(Extent::Fixed(3)),
    };
    assert_eq!(
        parse_set_value_for(Some(&declared), "[1, 2, 3]"),
        Some(Value::Vector(vec![1.0, 2.0, 3.0]))
    );
    assert_eq!(parse_set_value_for(Some(&declared), "[1, 2]"), None);
}
