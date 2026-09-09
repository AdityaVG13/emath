//! CLI ergonomics tests, moved from `crates/emath-cli/src/lib.rs`.

use std::process::Command;

use emath_cli::{
    CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE, check_json_document, diagnostics_json_document,
    json_diagnostic_entry, run_check,
};
use emath_cli_lab::run;
use emath_test_harness::{Case, Probe, boot, check_all};

fn diagnostic_codes(body: &str) -> Vec<String> {
    let parsed = match emath_artifact::parse_json_document(body) {
        Ok(parsed) => parsed,
        Err(_) => return Vec::new(),
    };
    match parsed.field("diagnostics") {
        Ok(emath_artifact::JsonValue::Arr(items)) => items
            .iter()
            .filter_map(|item| item.string_field("code").ok())
            .collect(),
        _ => Vec::new(),
    }
}

fn args(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_string).collect()
}

mod common;

fn json_output(line: &str) -> Result<(emath_artifact::JsonValue, CliExit, String), String> {
    let output = Command::new(common::emath_lab_bin())
        .args(line.split_whitespace().collect::<Vec<_>>())
        .output()
        .map_err(|error| format!("run emath binary: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = match output.status.code() {
        Some(0) => EXIT_OK,
        Some(1) => EXIT_REFUSED,
        _ => EXIT_USAGE,
    };
    let parsed = emath_artifact::parse_json_document(&stdout)
        .map_err(|error| format!("stdout must be valid JSON for `{line}`: {error}\n{stdout}"))?;
    Ok((parsed, code, stdout))
}

fn rows_of(
    parsed: &emath_artifact::JsonValue,
    key: &str,
) -> Result<Vec<emath_artifact::JsonValue>, String> {
    let value = parsed
        .field(key)
        .map_err(|error| format!("{key} lookup failed: {error}"))?;
    match value {
        emath_artifact::JsonValue::Arr(items) => {
            if items.is_empty() {
                Err(format!(
                    "{key} must be a NON-EMPTY array (an empty payload with exit 0 is exactly the weak-smoke failure this pins)"
                ))
            } else {
                Ok(items.clone())
            }
        }
        other => Err(format!("{key} must be array, got {other:?}")),
    }
}

fn invalid_fixture(name: &str) -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/invalid")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn refuse_check_eval_simulate(p: &mut Probe, path: &str) {
    p.eq(
        "check",
        run(&["check".into(), path.into(), "--json".into()]),
        EXIT_REFUSED,
    );
    p.eq(
        "eval",
        run(&["eval".into(), path.into(), "--json".into()]),
        EXIT_REFUSED,
    );
    p.eq(
        "simulate",
        run(&["simulate".into(), path.into(), "--json".into()]),
        EXIT_REFUSED,
    );
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("emath-cli-{label}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[test]
fn probe() {
    boot();
    let mut p = Probe::new(
        "cli exits, JSON schemas, missing/empty refuse, simulate binds typed state, notation builds",
    );

    p.case("exits", |p| {
        if let Err(message) = check_all(
            &[
                Case::new("bare", Vec::new(), EXIT_OK),
                Case::new("help", args("help"), EXIT_OK),
                Case::new("--help", args("--help"), EXIT_OK),
                Case::new("-h", args("-h"), EXIT_OK),
                Case::new("version", args("version"), EXIT_OK),
                Case::new("--version", args("--version"), EXIT_OK),
                Case::new("-V", args("-V"), EXIT_OK),
                Case::new("check --help", args("check --help"), EXIT_OK),
                Case::new("help check", args("help check"), EXIT_OK),
                Case::new("agent --help", args("agent --help"), EXIT_OK),
                Case::new("chek", args("chek"), EXIT_USAGE),
                Case::new("buld", args("buld"), EXIT_USAGE),
                Case::new("zzzzzzzz", args("zzzzzzzz"), EXIT_USAGE),
                Case::new("capabilities", args("capabilities"), EXIT_OK),
                Case::new("capabilities --json", args("capabilities --json"), EXIT_OK),
                Case::new("robot-docs", args("robot-docs"), EXIT_OK),
                Case::new("robot-docs guide", args("robot-docs guide"), EXIT_OK),
                Case::new("robot-docs --guide", args("robot-docs --guide"), EXIT_OK),
                Case::new("robot-docs waffle", args("robot-docs waffle"), EXIT_USAGE),
                Case::new(
                    "robot-docs guide extra",
                    args("robot-docs guide extra"),
                    EXIT_USAGE,
                ),
                Case::new("version extra", args("version extra"), EXIT_USAGE),
                Case::new("help check extra", args("help check extra"), EXIT_USAGE),
                Case::new("architecture --json", args("architecture --json"), EXIT_OK),
                Case::new(
                    "provider list --json",
                    args("provider list --json"),
                    EXIT_OK,
                ),
                Case::new("agent triage --help", args("agent triage --help"), EXIT_OK),
                Case::new(
                    "check --jason",
                    args("check --jason file.emath"),
                    EXIT_USAGE,
                ),
                Case::new("plan --jason", args("plan --jason file.emath"), EXIT_USAGE),
                Case::new("run --verify", args("run file.emath --verify"), EXIT_USAGE),
                Case::new("test --json", args("test file.emath --json"), EXIT_USAGE),
                Case::new("new --verify", args("new name --verify"), EXIT_USAGE),
                Case::new(
                    "vendor --verify",
                    args("vendor --out d --verify"),
                    EXIT_USAGE,
                ),
                Case::new(
                    "capabilities --waffle",
                    args("capabilities --waffle"),
                    EXIT_USAGE,
                ),
                Case::new("simulate --help", args("simulate --help"), EXIT_OK),
                Case::new("simulate", args("simulate"), EXIT_USAGE),
                Case::new("version --help", args("version --help"), EXIT_OK),
                Case::new("capabilities --help", args("capabilities --help"), EXIT_OK),
                Case::new("robot-docs --help", args("robot-docs --help"), EXIT_OK),
            ],
            |argv| run(argv),
        ) {
            p.fail("table", message);
        }
        let doctor = run(&args("doctor --json"));
        p.demand(
            "doctor",
            doctor == EXIT_OK || doctor == EXIT_REFUSED,
            format!("doctor may refuse when a required host tool is unavailable, got {doctor:?}"),
        );
    });

    p.case("capabilities-json", |p| {
        match json_output("capabilities --json") {
            Err(error) => {
                p.fail("json", error);
            }
            Ok((parsed, code, stdout)) => {
                p.eq("exit", code, EXIT_OK);
                p.ne("empty", stdout.trim().to_string(), "{}".to_string());
                match parsed.string_field("schema") {
                    Ok(schema) => {
                        p.eq("schema", schema, "emath.capabilities".to_string());
                    }
                    Err(error) => {
                        p.fail("schema", format!("{error}"));
                    }
                }
                match parsed.string_field("tool") {
                    Ok(tool) => {
                        p.eq("tool", tool, "emath".to_string());
                    }
                    Err(error) => {
                        p.fail("tool", format!("{error}"));
                    }
                }
                match rows_of(&parsed, "commands") {
                    Err(error) => {
                        p.fail("commands", error);
                    }
                    Ok(commands) => {
                        let names: Vec<String> = commands
                            .iter()
                            .filter_map(|row| row.string_field("name").ok())
                            .collect();
                        for command in &commands {
                            p.demand(
                                "name",
                                command.string_field("name").is_ok(),
                                "command row has name",
                            );
                            p.demand(
                                "usage",
                                command.string_field("usage").is_ok(),
                                "command row has usage",
                            );
                            p.demand(
                                "summary",
                                command.string_field("summary").is_ok(),
                                "command row has summary",
                            );
                        }
                        for required in ["check", "capabilities", "fmt", "migrate"] {
                            p.demand(
                                required,
                                names.iter().any(|name| name == required),
                                format!("capabilities must list `{required}`; got: {names:?}"),
                            );
                        }
                    }
                }
            }
        }
    });

    p.case("architecture-json", |p| {
        match json_output("architecture --json") {
            Err(error) => {
                p.fail("json", error);
            }
            Ok((parsed, code, stdout)) => {
                p.eq("exit", code, EXIT_OK);
                p.ne("empty", stdout.trim().to_string(), "{}".to_string());
                match parsed.string_field("schema") {
                    Ok(schema) => {
                        p.eq("schema", schema, "emath.architecture".to_string());
                    }
                    Err(error) => {
                        p.fail("schema", format!("{error}"));
                    }
                }
                match parsed.string_field("pipeline") {
                    Ok(pipeline) => {
                        p.contains("EMIR", &pipeline, "EMIR");
                    }
                    Err(error) => {
                        p.fail("pipeline", format!("{error}"));
                    }
                }
                if let Err(error) = rows_of(&parsed, "required_paths") {
                    p.fail("required_paths", error);
                }
            }
        }
    });

    p.case("doctor-json", |p| match json_output("doctor --json") {
        Err(error) => {
            p.fail("json", error);
        }
        Ok((parsed, code, stdout)) => {
            p.demand(
                "exit",
                code == EXIT_OK || code == EXIT_REFUSED,
                format!("doctor may refuse when a required host tool is unavailable, got {code:?}"),
            );
            p.ne("empty", stdout.trim().to_string(), "{}".to_string());
            match parsed.string_field("schema") {
                Ok(schema) => {
                    p.eq("schema", schema, "emath.doctor".to_string());
                }
                Err(error) => {
                    p.fail("schema", format!("{error}"));
                }
            }
            p.demand(
                "ok",
                parsed.field("ok").is_ok(),
                "doctor must carry an aggregate ok field",
            );
            match rows_of(&parsed, "checks") {
                Err(error) => {
                    p.fail("checks", error);
                }
                Ok(checks) => {
                    let names: Vec<String> = checks
                        .iter()
                        .filter_map(|row| row.string_field("name").ok())
                        .collect();
                    for required in ["rustc", "cargo"] {
                        p.demand(
                            required,
                            names.iter().any(|name| name == required),
                            format!("doctor must probe `{required}`; got: {names:?}"),
                        );
                    }
                }
            }
        }
    });

    p.case("provider-list-json", |p| {
        match json_output("provider list --json") {
            Err(error) => {
                p.fail("json", error);
            }
            Ok((parsed, code, stdout)) => {
                p.eq("exit", code, EXIT_OK);
                p.ne("empty", stdout.trim().to_string(), "{}".to_string());
                match parsed.string_field("schema") {
                    Ok(schema) => {
                    p.eq("schema", schema, "emath.provider-list".to_string());
                },
                    Err(error) => {
                        p.fail("schema", format!("{error}"));
                    }
                }
                match rows_of(&parsed, "providers") {
                    Err(error) => {
                        p.fail("providers", error);
                    }
                    Ok(providers) => {
                        let statuses: Vec<String> = providers
                            .iter()
                            .filter_map(|row| row.string_field("status").ok())
                            .collect();
                        for provider in &providers {
                            p.demand("id", provider.string_field("id").is_ok(), "provider row has id");
                            p.demand(
                                "capability",
                                provider.string_field("capability").is_ok(),
                                "provider row has capability",
                            );
                        }
                        p.demand(
                            "implemented",
                            statuses.iter().any(|status| status == "implemented"),
                            format!(
                                "in-tree providers must be present and marked implemented; got: {statuses:?}"
                            ),
                        );
                    }
                }
            }
        }
    });

    p.case("causalized-check", |p| {
        let dir = temp_dir("causalized");
        let causalized = dir.join("causalized.emath");
        let plain = dir.join("plain.emath");
        let _ = std::fs::write(
            &causalized,
            "\
emath model CausalizedRC:
    inputs:
        V: Float64
    algebraic:
        I: Float64
    state:
        q: Float64
    equations:
        V - I - q == 0
        der(q) = I
",
        );
        let _ = std::fs::write(
            &plain,
            "\
emath model PlainRC:
    inputs:
        V: Float64
    state:
        q: Float64
    equations:
        der(q) = V - q
",
        );
        p.eq(
            "causalized",
            run(&["check".into(), causalized.to_string_lossy().into_owned()]),
            EXIT_OK,
        );
        p.eq(
            "plain",
            run(&["check".into(), plain.to_string_lossy().into_owned()]),
            EXIT_OK,
        );
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.case("missing-epkg080", |p| {
        let missing = std::env::temp_dir().join(format!(
            "emath-cli-missing-assert-{}.emath",
            std::process::id()
        ));
        let path = missing.to_string_lossy().into_owned();
        match emath_cli_lab::genesis_cmd::analyze(&missing) {
            Ok(_) => {
                p.fail("analyze", "missing source must refuse");
            }
            Err(error) => {
                p.contains("analyze", &error, "E-PKG-080");
            }
        }
        let (diagnostics, package_id, units_profiles) = run_check(&missing);
        let body = check_json_document(false, &package_id, &diagnostics, None, &units_profiles);
        p.demand(
            "check-json",
            diagnostic_codes(&body)
                .iter()
                .any(|code| code == "E-PKG-080"),
            format!("check --json must name E-PKG-080, got {body}"),
        );
        p.eq(
            "check",
            run(&["check".into(), path.clone(), "--json".into()]),
            EXIT_REFUSED,
        );
        p.eq(
            "eval",
            run(&["eval".into(), path.clone(), "--json".into()]),
            EXIT_REFUSED,
        );
        let out = missing.with_extension("out");
        p.eq(
            "compile",
            run(&[
                "compile".into(),
                "--parametric".into(),
                path.clone(),
                "--out".into(),
                out.to_string_lossy().into_owned(),
            ]),
            EXIT_REFUSED,
        );
        p.eq(
            "simulate",
            run(&["simulate".into(), path.clone(), "--json".into()]),
            EXIT_REFUSED,
        );
        for argv in [
            vec!["expand".into(), path.clone(), "--json".into()],
            vec!["plan".into(), path.clone(), "--json".into()],
            vec!["planner".into(), path.clone(), "--json".into()],
            vec!["build".into(), path.clone(), "--json".into()],
            vec!["freeze".into(), path.clone(), "--json".into()],
            vec!["exactness".into(), path.clone(), "--json".into()],
            vec![
                "solve".into(),
                "--check".into(),
                path.clone(),
                "--json".into(),
            ],
            vec!["assumptions".into(), path.clone(), "--json".into()],
            vec![
                "why".into(),
                path.clone(),
                "inference:1".into(),
                "--json".into(),
            ],
        ] {
            p.eq(format!("{argv:?}"), run(&argv), EXIT_USAGE);
        }
        let envelope = diagnostics_json_document(
            "expand",
            false,
            &[json_diagnostic_entry(
                "E-PKG-080",
                "error",
                "cannot read source file (missing.emath)",
            )],
        );
        p.demand(
            "envelope",
            diagnostic_codes(&envelope)
                .iter()
                .any(|code| code == "E-PKG-080"),
            format!("refusal envelope must name E-PKG-080, got {envelope}"),
        );
    });

    p.case("eval-spec-oracle", |p| {
        let example = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/language/intro/hello-square.emath");
        let path = example.to_string_lossy().into_owned();
        p.eq(
            "check",
            run(&["check".into(), path.clone(), "--json".into()]),
            EXIT_OK,
        );
        p.eq(
            "eval",
            run(&["eval".into(), path.clone(), "--json".into()]),
            EXIT_OK,
        );
        p.eq(
            "solve",
            run(&["solve".into(), "--check".into(), path, "--json".into()]),
            EXIT_REFUSED,
        );
    });

    p.case("junk", |p| {
        let dir = temp_dir("junk");
        let spec = dir.join("junk.emath");
        let _ = std::fs::write(&spec, "this is not emath at all");
        let path = spec.to_string_lossy().into_owned();
        p.eq(
            "check",
            run(&["check".into(), path.clone(), "--json".into()]),
            EXIT_REFUSED,
        );
        p.eq(
            "eval",
            run(&["eval".into(), path, "--json".into()]),
            EXIT_REFUSED,
        );
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.case("empty-epkg081", |p| {
        let dir = temp_dir("empty");
        let empty = dir.join("empty.emath");
        let comments = dir.join("comments.emath");
        let _ = std::fs::write(&empty, "");
        let _ = std::fs::write(&comments, "# comment only\n");
        let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
        match session.load_package(&empty) {
            Err(error) => {
                p.fail("load", format!("empty source loads: {error}"));
            }
            Ok(package) => {
                let result = session.check(package.file);
                let body = check_json_document(
                    result.diagnostics.has_errors() == false,
                    &result.package.content_id().0,
                    &result.diagnostics,
                    None,
                    &result.units_profiles,
                );
                p.demand(
                    "json",
                    diagnostic_codes(&body)
                        .iter()
                        .any(|code| code == "E-PKG-081"),
                    format!("check --json empty must name E-PKG-081, got {body}"),
                );
            }
        }
        refuse_check_eval_simulate(&mut *p, &empty.to_string_lossy());
        p.eq(
            "expand-empty",
            run(&[
                "expand".into(),
                empty.to_string_lossy().into_owned(),
                "--json".into(),
            ]),
            EXIT_REFUSED,
        );
        p.eq(
            "expand-comments",
            run(&[
                "expand".into(),
                comments.to_string_lossy().into_owned(),
                "--json".into(),
            ]),
            EXIT_REFUSED,
        );
        let empty_path = empty.to_string_lossy().into_owned();
        for argv in [
            vec!["freeze".into(), empty_path.clone(), "--json".into()],
            vec!["exactness".into(), empty_path.clone(), "--json".into()],
            vec![
                "solve".into(),
                "--check".into(),
                empty_path.clone(),
                "--json".into(),
            ],
            vec!["assumptions".into(), empty_path.clone(), "--json".into()],
            vec![
                "why".into(),
                empty_path.clone(),
                "inference:1".into(),
                "--json".into(),
            ],
        ] {
            p.eq(format!("{argv:?}"), run(&argv), EXIT_REFUSED);
        }
        let envelope = diagnostics_json_document(
            "expand",
            false,
            &[json_diagnostic_entry(
                "E-PKG-081",
                "error",
                "source has no declarations (empty.emath)",
            )],
        );
        p.demand(
            "envelope",
            diagnostic_codes(&envelope)
                .iter()
                .any(|code| code == "E-PKG-081"),
            format!("empty refusal envelope must name E-PKG-081, got {envelope}"),
        );
        refuse_check_eval_simulate(&mut *p, &comments.to_string_lossy());
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.case("invalid-fixtures", |p| {
        for name in [
            "empty.emath",
            "duplicate_output.emath",
            "unit_mismatch.emath",
            "unknown_section.emath",
            "compile_junk.emath",
            "named_call_arg.emath",
        ] {
            p.case(name, |p| {
                refuse_check_eval_simulate(p, &invalid_fixture(name))
            });
        }
    });

    p.case("simulate-vector-state", |p| {
        let example = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/language/numerical/explicit-mass-spring.emath");
        let path = example.to_string_lossy().into_owned();
        p.eq(
            "vector",
            run(&[
                "simulate".into(),
                path.clone(),
                "--set".into(),
                "m=1".into(),
                "--set".into(),
                "k=1".into(),
                "--set".into(),
                "c=0".into(),
                "--set".into(),
                "s=[1,0]".into(),
                "--dt".into(),
                "0.01".into(),
                "--t1".into(),
                "0.1".into(),
            ]),
            EXIT_OK,
        );
        p.eq(
            "scalar-refused",
            run(&[
                "simulate".into(),
                path,
                "--set".into(),
                "m=1".into(),
                "--set".into(),
                "k=1".into(),
                "--set".into(),
                "c=0".into(),
                "--set".into(),
                "s=1".into(),
            ]),
            EXIT_USAGE,
        );
    });

    p.case("simulate-scalar", |p| {
        let dir = temp_dir("sim-scalar");
        let spec = dir.join("decay.emath");
        let _ = std::fs::write(
            &spec,
            "\
emath model Decay:
    inputs:
        k: Float64
    state:
        x: Float64
    equations:
        der(x) = -k * x
",
        );
        p.eq(
            "bind",
            run(&[
                "simulate".into(),
                spec.to_string_lossy().into_owned(),
                "--set".into(),
                "k=1".into(),
                "--set".into(),
                "x=1".into(),
                "--method".into(),
                "euler".into(),
                "--dt".into(),
                "0.1".into(),
                "--t1".into(),
                "0.2".into(),
            ]),
            EXIT_OK,
        );
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.case("simulate-methods", |p| {
        let models = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../language/examples/numerical/solver-methods.emath");
        p.eq(
            "backward-euler",
            run(&[
                "simulate".into(),
                models.to_string_lossy().into_owned(),
                "--model".into(),
                "StiffDecay".into(),
                "--set".into(),
                "y=1".into(),
                "--method".into(),
                "backward-euler".into(),
                "--dt".into(),
                "0.1".into(),
                "--t1".into(),
                "0.3".into(),
            ]),
            EXIT_OK,
        );
        p.eq(
            "velocity-verlet",
            run(&[
                "simulate".into(),
                models.to_string_lossy().into_owned(),
                "--model".into(),
                "HarmonicOscillator".into(),
                "--set".into(),
                "q=1".into(),
                "--set".into(),
                "v=0".into(),
                "--method".into(),
                "velocity-verlet".into(),
                "--dt".into(),
                "0.01".into(),
                "--t1".into(),
                "0.1".into(),
            ]),
            EXIT_OK,
        );
    });

    p.case("notation-build", |p| {
        let dir = temp_dir("notation");
        let spec = dir.join("notation.emath");
        let out = dir.join("artifacts");
        let _ = std::fs::write(
            &spec,
            "\
package tst.notation
notation infixl 40 \"⊕\" => core::math::pow alias \"pw\"
notation prefix 80 \"√\" => core::math::sqrt
notation postfix 90 \"inv\" => core::math::recip
emath function F:
    inputs:
        x: Float64
        y: Float64
    outputs:
        r: Float64
    definitions:
        a = x pw y
        b = √ a
        r = b inv
    goals:
        evaluate <r>:
            produce rust.library
    tests:
        example <pow_sqrt_recip>:
            given x = 4.0
            given y = 3.0
            expect r == 0.125
",
        );
        p.eq(
            "check",
            run(&["check".into(), spec.to_string_lossy().into_owned()]),
            EXIT_OK,
        );
        p.eq(
            "build",
            run(&[
                "build".into(),
                spec.to_string_lossy().into_owned(),
                "--out".into(),
                out.to_string_lossy().into_owned(),
            ]),
            EXIT_OK,
        );
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.case("reserved-glyph", |p| {
        let dir = temp_dir("notation-reject");
        let spec = dir.join("reserved.emath");
        let out = dir.join("artifacts");
        let _ = std::fs::write(
            &spec,
            "\
package tst.reserved
emath function F:
    outputs:
        r: Float64
    definitions:
        r = 1.0
    goals:
        evaluate <r>:
            produce rust.library
notation prefix 90 \"or\" => core::logic::not
",
        );
        p.eq(
            "build",
            run(&[
                "build".into(),
                spec.to_string_lossy().into_owned(),
                "--out".into(),
                out.to_string_lossy().into_owned(),
            ]),
            EXIT_REFUSED,
        );
        let _ = std::fs::remove_dir_all(&dir);
    });

    p.finish();
}

#[test]
fn answer_first_retains_independent_values_after_failure() {
    use emath_artifact::{JsonValue, parse_json_document};

    emath_syntax::install_source_parser();
    let dir = temp_dir("answer-first-partial");
    let path = dir.join("partial.emath");
    std::fs::write(&path, "emath function Partial:\n    inputs:\n        x: Int\n    definitions:\n        a_kept = 2 + 1\n        b_fault = x + 1\n        c_blocked = b_fault + 1\n        d_later = 4 + 1\n").unwrap();
    assert_eq!(
        emath_cli::run(&[
            "run".into(),
            path.to_string_lossy().into_owned(),
            "--set".into(),
            format!("x={}", i64::MAX),
            "--out".into(),
            dir.to_string_lossy().into_owned(),
            "--json".into(),
        ]),
        EXIT_REFUSED
    );
    let result = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .find_map(|entry| {
            let bytes = std::fs::read_to_string(entry.path()).ok()?;
            let envelope = parse_json_document(&bytes).ok()?;
            let payload = parse_json_document(&envelope.string_field("payload").ok()?).ok()?;
            let JsonValue::Arr(completed) = payload.field("completed").ok()? else {
                return None;
            };
            let JsonValue::Str(result) = completed.first()? else {
                return None;
            };
            parse_json_document(result).ok()
        })
        .expect("a failed run must retain its completed case");
    let outputs = result.field("outputs").unwrap();
    assert_eq!(
        outputs
            .field("a_kept")
            .unwrap()
            .string_field("value")
            .unwrap(),
        "3"
    );
    assert_eq!(
        outputs
            .field("d_later")
            .unwrap()
            .string_field("value")
            .unwrap(),
        "5"
    );
    assert!(outputs.field("b_fault").is_err());
    assert!(outputs.field("c_blocked").is_err());
    assert_eq!(result.field("goal_met").unwrap(), &JsonValue::Bool(false));
}

#[test]
fn approved_exact_arithmetic_preserves_rational_values() {
    use emath_artifact::{JsonValue, parse_json_document};
    emath_syntax::install_source_parser();
    let dir = temp_dir("approved-exact-arithmetic");
    let source = dir.join("exact.emath");
    std::fs::write(&source, "emath function Exact:\n    definitions:\n        difference = rat_sub(rat(7, 6), rat(1, 3))\n        multiplied = rat_mul(rat(6, 35), rat(14, 9))\n        quotient = rat_div(rat(-3, 7), rat(9, 14))\n        ordered = rat_lt(rat(-1, 3), rat(0, 1))\n").unwrap();
    assert_eq!(emath_cli::run(&["run".into(), source.to_string_lossy().into_owned(), "--out".into(), dir.to_string_lossy().into_owned(), "--json".into()]), EXIT_OK);
    let result = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).find_map(|entry| {
        let envelope = parse_json_document(&std::fs::read_to_string(entry.path()).ok()?).ok()?;
        let payload = parse_json_document(&envelope.string_field("payload").ok()?).ok()?;
        let JsonValue::Arr(completed) = payload.field("completed").ok()? else { return None; };
        let JsonValue::Str(result) = completed.first()? else { return None; };
        parse_json_document(result).ok()
    }).expect("the exact result must be saved");
    let outputs = result.field("outputs").unwrap();
    for (name, num, den) in [("difference", "5", "6"), ("multiplied", "4", "15"), ("quotient", "-2", "3")] {
        let value = outputs.field(name).unwrap();
        assert_eq!(value.string_field("numerator").unwrap(), num);
        assert_eq!(value.string_field("denominator").unwrap(), den);
    }
    assert_eq!(outputs.field("ordered").unwrap().string_field("value").unwrap(), "true");
}


#[test]
fn approved_root_method_survives_process_restart() {
    use emath_artifact::{JsonValue, parse_json_document};
    let directory = temp_dir("authored-root-continuation");
    let source = directory.join("root.emath");
    std::fs::write(&source, "emath function Root:\n    inputs:\n        denominator: Int\n    definitions:\n        bracket = root_enclose([rat(-2, 1), rat(0, 1), rat(1, 1)], rat(1, 1), rat(2, 1), rat(1, denominator))\n    goals:\n        evaluate <bracket>:\n            produce rust.library\n").unwrap();
    let call = |arguments: Vec<String>| {
        let output = Command::new(common::emath_bin()).args(arguments).output().unwrap();
        let document = parse_json_document(&String::from_utf8(output.stdout).unwrap()).unwrap();
        (document, output.status.code().unwrap())
    };
    let (initial, initial_exit) = call(vec!["run".into(), source.to_string_lossy().into_owned(), "--set".into(), "denominator=65536".into(), "--out".into(), directory.to_string_lossy().into_owned(), "--work".into(), "1".into(), "--json".into()]);
    assert_eq!(initial.field("goal_met").unwrap(), &JsonValue::Bool(false), "a wide certified bracket has not reached the requested tolerance");
    assert_eq!(initial_exit, 1);
    let checkpoint = initial.string_field("checkpoint").unwrap_or_else(|error| panic!("{error}: {initial:?}"));
    let (partial, partial_exit) = call(vec!["step".into(), checkpoint, "--work".into(), "8".into(), "--json".into()]);
    assert_eq!(partial_exit, 1);
    assert_eq!(partial.field("goal_met").unwrap(), &JsonValue::Bool(false));
    fn bracket(document: &JsonValue) -> &JsonValue {
        let JsonValue::Arr(results) = document.field("results").unwrap() else { panic!("results must be an array"); };
        results.last().unwrap().field("outputs").unwrap().field("bracket").unwrap().field("fields").unwrap()
    }
    assert_eq!(bracket(&partial).field("steps").unwrap().string_field("value").unwrap(), "8");
    let (verification, verify_exit) = call(vec!["verify".into(), partial.string_field("checkpoint").unwrap(), "--json".into()]);
    assert_eq!(verify_exit, 0);
    assert_eq!(verification.field("verified").unwrap(), &JsonValue::Bool(true));
    let (finished, finished_exit) = call(vec!["step".into(), partial.string_field("checkpoint").unwrap(), "--work".into(), "8".into(), "--json".into()]);
    assert_eq!(finished_exit, 0);
    assert_eq!(finished.field("goal_met").unwrap(), &JsonValue::Bool(true));
    let result = bracket(&finished);
    assert_eq!(result.field("steps").unwrap().string_field("value").unwrap(), "16");
    let lower = result.field("lower").unwrap();
    let upper = result.field("upper").unwrap();
    assert_eq!(lower.string_field("numerator").unwrap(), "92681");
    assert_eq!(lower.string_field("denominator").unwrap(), "65536");
    assert_eq!(upper.string_field("numerator").unwrap(), "46341");
    assert_eq!(upper.string_field("denominator").unwrap(), "32768");
    let (verification, verify_exit) = call(vec!["verify".into(), finished.string_field("checkpoint").unwrap(), "--json".into()]);
    assert_eq!(verify_exit, 0);
    assert_eq!(verification.field("verified").unwrap(), &JsonValue::Bool(true));
}
