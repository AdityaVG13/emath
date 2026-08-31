//! CLI ergonomics tests, moved from `crates/emath-cli/src/lib.rs`.

use std::process::Command;

use emath_cli::{
    check_json_document, diagnostics_json_document, json_diagnostic_entry, run, run_check, CliExit,
    EXIT_OK, EXIT_REFUSED, EXIT_USAGE,
};

fn diagnostic_codes(body: &str) -> Vec<String> {
    let parsed = emath_artifact::parse_json_document(body).expect("json");
    match parsed.field("diagnostics").expect("diagnostics") {
        emath_artifact::JsonValue::Arr(items) => items
            .iter()
            .map(|item| item.string_field("code").expect("code"))
            .collect(),
        other => panic!("diagnostics must be array, got {other:?}"),
    }
}

fn args(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_string).collect()
}

#[test]
fn bare_and_help_exit_ok() {
    assert_eq!(run(&[]), EXIT_OK);
    assert_eq!(run(&args("help")), EXIT_OK);
    assert_eq!(run(&args("--help")), EXIT_OK);
    assert_eq!(run(&args("-h")), EXIT_OK);
}

#[test]
fn version_aliases_exit_ok() {
    assert_eq!(run(&args("version")), EXIT_OK);
    assert_eq!(run(&args("--version")), EXIT_OK);
    assert_eq!(run(&args("-V")), EXIT_OK);
}

#[test]
fn command_help_is_first_try() {
    assert_eq!(run(&args("check --help")), EXIT_OK);
    assert_eq!(run(&args("help check")), EXIT_OK);
    assert_eq!(run(&args("agent --help")), EXIT_OK);
}

#[test]
fn unknown_command_is_usage() {
    assert_eq!(run(&args("chek")), EXIT_USAGE);
    assert_eq!(run(&args("buld")), EXIT_USAGE);
    assert_eq!(run(&args("zzzzzzzz")), EXIT_USAGE);
}

#[test]
fn capabilities_and_robot_docs_exit_ok() {
    assert_eq!(run(&args("capabilities")), EXIT_OK);
    assert_eq!(run(&args("capabilities --json")), EXIT_OK);
    assert_eq!(run(&args("robot-docs")), EXIT_OK);
    assert_eq!(run(&args("robot-docs guide")), EXIT_OK);
    assert_eq!(run(&args("robot-docs --guide")), EXIT_OK);
    assert_eq!(run(&args("robot-docs waffle")), EXIT_USAGE);
    assert_eq!(run(&args("robot-docs guide extra")), EXIT_USAGE);
    assert_eq!(run(&args("version extra")), EXIT_USAGE);
    assert_eq!(run(&args("help check extra")), EXIT_USAGE);
}

#[test]
fn read_side_json_and_triage_help_exit_ok() {
    assert_eq!(run(&args("architecture --json")), EXIT_OK);
    assert!(
        matches!(run(&args("doctor --json")), EXIT_OK | EXIT_REFUSED),
        "doctor may refuse when a required host tool is unavailable"
    );
    assert_eq!(run(&args("provider list --json")), EXIT_OK);
    assert_eq!(run(&args("agent triage --help")), EXIT_OK);
}

// F040 (emath-mock-stubtest-assertions-e3wv): the JSON commands above
// were exit-code-only smoke; the tests below parse the real stdout and
// pin the schema id + required keys + non-empty rows, so a run that
// prints `{}` (or drops the schema id) with exit 0 FAILS here.
// End-to-end: the built binary is execed directly (see `common`), so
// the assertions cover what the CLI actually prints.

mod common;

fn json_output(line: &str) -> (emath_artifact::JsonValue, CliExit, String) {
    let output = Command::new(common::emath_bin())
        .args(line.split_whitespace().collect::<Vec<_>>())
        .output()
        .expect("run emath binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = match output.status.code() {
        Some(0) => EXIT_OK,
        Some(1) => EXIT_REFUSED,
        _ => EXIT_USAGE,
    };
    let parsed = match emath_artifact::parse_json_document(&stdout) {
        Ok(parsed) => parsed,
        Err(error) => panic!(
            "stdout must be valid JSON for `{line}`: {error}\n{stdout}"
        ),
    };
    (parsed, code, stdout)
}

fn rows_of(parsed: &emath_artifact::JsonValue, key: &str) -> Vec<emath_artifact::JsonValue> {
    let value = match parsed.field(key) {
        Ok(value) => value,
        Err(error) => panic!("{key} lookup failed: {error}"),
    };
    match value {
        emath_artifact::JsonValue::Arr(items) => {
            assert!(
                !items.is_empty(),
                "{key} must be a NON-EMPTY array (an empty payload with exit \
                 0 is exactly the weak-smoke failure this pins)"
            );
            items.clone()
        }
        other => panic!("{key} must be array, got {other:?}"),
    }
}

#[test]
fn capabilities_json_pins_schema_and_command_rows() {
    let (parsed, code, stdout) = json_output("capabilities --json");
    assert_eq!(code, EXIT_OK);
    assert_ne!(stdout.trim(), "{}", "empty payload with exit 0 must fail");
    assert_eq!(
        parsed.string_field("schema").expect("schema id"),
        "emath.capabilities"
    );
    assert_eq!(parsed.string_field("tool").expect("tool"), "emath");
    let commands = rows_of(&parsed, "commands");
    for row in &commands {
        let _ = row.string_field("name").expect("command row has name");
        let _ = row.string_field("usage").expect("command row has usage");
        let _ = row.string_field("summary").expect("command row has summary");
    }
    // The machine contract must cover the core verbs, not an empty list.
    let names: Vec<String> = commands
        .iter()
        .map(|r| r.string_field("name").expect("name"))
        .collect();
    for required in ["check", "capabilities", "fmt", "migrate"] {
        assert!(
            names.iter().any(|n| n == required),
            "capabilities must list `{required}`; got: {names:?}"
        );
    }
}

#[test]
fn architecture_json_pins_schema_and_required_paths() {
    let (parsed, code, stdout) = json_output("architecture --json");
    assert_eq!(code, EXIT_OK);
    assert_ne!(stdout.trim(), "{}");
    assert_eq!(
        parsed.string_field("schema").expect("schema id"),
        "emath.architecture"
    );
    let pipeline = parsed.string_field("pipeline").expect("pipeline");
    assert!(
        pipeline.contains("EMIR"),
        "the pipeline description must name the IR spine; got: {pipeline}"
    );
    let _ = rows_of(&parsed, "required_paths");
}

#[test]
fn doctor_json_pins_schema_and_probe_rows() {
    let (parsed, code, stdout) = json_output("doctor --json");
    assert!(
        matches!(code, EXIT_OK | EXIT_REFUSED),
        "doctor may refuse when a required host tool is unavailable"
    );
    assert_ne!(stdout.trim(), "{}");
    assert_eq!(
        parsed.string_field("schema").expect("schema id"),
        "emath.doctor"
    );
    assert!(
        parsed.field("ok").is_ok(),
        "doctor must carry an aggregate ok field"
    );
    let checks = rows_of(&parsed, "checks");
    let names: Vec<String> = checks
        .iter()
        .map(|c| c.string_field("name").expect("probe row has name"))
        .collect();
    for required in ["rustc", "cargo"] {
        assert!(
            names.iter().any(|n| n == required),
            "doctor must probe `{required}`; got: {names:?}"
        );
    }
}

#[test]
fn provider_list_json_pins_schema_and_nonempty_rows() {
    let (parsed, code, stdout) = json_output("provider list --json");
    assert_eq!(code, EXIT_OK);
    assert_ne!(stdout.trim(), "{}");
    assert_eq!(
        parsed.string_field("schema").expect("schema id"),
        "emath.provider-list"
    );
    let providers = rows_of(&parsed, "providers");
    let statuses: Vec<String> = providers
        .iter()
        .map(|p| p.string_field("status").expect("provider row has status"))
        .collect();
    for p in &providers {
        let _ = p.string_field("id").expect("provider row has id");
        let _ = p.string_field("capability").expect("provider row has capability");
    }
    assert!(
        statuses.iter().any(|s| s == "implemented"),
        "in-tree providers must be present and marked implemented; got: {statuses:?}"
    );
}

#[test]
fn unknown_flag_is_usage() {
    assert_eq!(run(&args("check --jason file.emath")), EXIT_USAGE);
    assert_eq!(run(&args("plan --jason file.emath")), EXIT_USAGE);
    assert_eq!(run(&args("run file.emath --verify")), EXIT_USAGE);
    assert_eq!(run(&args("run file.emath --json")), EXIT_USAGE);
    assert_eq!(run(&args("test file.emath --json")), EXIT_USAGE);
    assert_eq!(run(&args("new name --verify")), EXIT_USAGE);
    assert_eq!(run(&args("vendor --out d --verify")), EXIT_USAGE);
    assert_eq!(run(&args("capabilities --waffle")), EXIT_USAGE);
    assert_eq!(run(&args("simulate --help")), EXIT_OK);
    assert_eq!(run(&args("simulate")), EXIT_USAGE);
}

#[test]
fn catalog_commands_honor_help_and_unknown_flags() {
    assert_eq!(run(&args("version --help")), EXIT_OK);
    assert_eq!(run(&args("capabilities --help")), EXIT_OK);
    assert_eq!(run(&args("robot-docs --help")), EXIT_OK);
}

#[test]
fn causalized_residual_model_admits_and_plain_models_keep_admitting() {
    // The library's `run` assumes the parser backend is installed (the
    // binary does this in main); parsing tests must install it once.
    emath_syntax::install_source_parser();
    let dir = std::env::temp_dir().join(format!("emath-cli-causalized-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let causalized = dir.join("causalized.emath");
    let plain = dir.join("plain.emath");
    std::fs::write(
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
    )
    .expect("write causalized");
    std::fs::write(
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
    )
    .expect("write plain");
    assert_eq!(
        run(&["check".into(), causalized.to_string_lossy().into_owned()]),
        EXIT_OK,
        "causalized residuals must admit at check (codegen now handles them)"
    );
    assert_eq!(
        run(&["check".into(), plain.to_string_lossy().into_owned()]),
        EXIT_OK,
        "single-rate models keep admitting"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_file_is_epkg080_on_check_eval_compile_simulate() {
    emath_syntax::install_source_parser();
    let missing = std::env::temp_dir().join(format!(
        "emath-cli-missing-pass28-{}.emath",
        std::process::id()
    ));
    let path = missing.to_string_lossy().into_owned();
    let err = match emath_cli::genesis_cmd::analyze(&missing) {
        Ok(_) => panic!("missing source must refuse"),
        Err(error) => error,
    };
    assert!(
        err.contains("E-PKG-080"),
        "eval/compile analyze must name E-PKG-080, got {err}"
    );
    let (diagnostics, package_id, units_profiles) = run_check(&missing);
    let body = check_json_document(false, &package_id, &diagnostics, None, &units_profiles);
    assert!(
        diagnostic_codes(&body)
            .iter()
            .any(|code| code == "E-PKG-080"),
        "check --json must name E-PKG-080, got {body}"
    );
    assert_eq!(
        run(&["check".into(), path.clone(), "--json".into()]),
        EXIT_REFUSED
    );
    assert_eq!(
        run(&["eval".into(), path.clone(), "--json".into()]),
        EXIT_REFUSED
    );
    let out = missing.with_extension("out");
    assert_eq!(
        run(&[
            "compile".into(),
            "--parametric".into(),
            path.clone(),
            "--out".into(),
            out.to_string_lossy().into_owned(),
        ]),
        EXIT_REFUSED
    );
    assert_eq!(
        run(&["simulate".into(), path.clone(), "--json".into()]),
        EXIT_REFUSED
    );
    for args in [
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
        assert_eq!(
            run(&args),
            EXIT_USAGE,
            "missing provided file --json must be IO usage, got {args:?}"
        );
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
    assert!(
        diagnostic_codes(&envelope)
            .iter()
            .any(|code| code == "E-PKG-080"),
        "refusal envelope must name E-PKG-080, got {envelope}"
    );
}

#[test]
fn eval_on_function_file_uses_spec_oracle() {
    emath_syntax::install_source_parser();
    let example = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../language/examples/intro/hello-square.emath");
    let path = example.to_string_lossy().into_owned();
    assert_eq!(
        run(&["check".into(), path.clone(), "--json".into()]),
        EXIT_OK,
        "hello-square must admit at check"
    );
    // Plain `emath eval` on a standard function spec runs the spec's own
    // worked example as the input oracle (deterministic, no invented
    // bindings): `given x = 3` -> `y == 9` must exit OK.
    assert_eq!(
        run(&["eval".into(), path.clone(), "--json".into()]),
        EXIT_OK,
        "a function file with a passing example must eval (spec oracle), not refuse"
    );
    assert_eq!(
        run(&["solve".into(), "--check".into(), path, "--json".into()]),
        EXIT_REFUSED,
        "hello-square has no solve intent; --json must still refuse"
    );
}

#[test]
fn check_and_eval_json_refuse_junk() {
    emath_syntax::install_source_parser();
    let dir = std::env::temp_dir().join(format!("emath-cli-junk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let spec = dir.join("junk.emath");
    std::fs::write(&spec, "this is not emath at all").expect("write junk");
    let path = spec.to_string_lossy().into_owned();
    assert_eq!(
        run(&["check".into(), path.clone(), "--json".into()]),
        EXIT_REFUSED
    );
    assert_eq!(run(&["eval".into(), path, "--json".into()]), EXIT_REFUSED);
    let _ = std::fs::remove_dir_all(&dir);
}

fn invalid_fixture(name: &str) -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/invalid")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn assert_check_eval_simulate_refuse(path: &str) {
    assert_eq!(
        run(&["check".into(), path.into(), "--json".into()]),
        EXIT_REFUSED,
        "check must refuse {path}"
    );
    assert_eq!(
        run(&["eval".into(), path.into(), "--json".into()]),
        EXIT_REFUSED,
        "eval must refuse {path}"
    );
    assert_eq!(
        run(&["simulate".into(), path.into(), "--json".into()]),
        EXIT_REFUSED,
        "simulate must refuse {path}"
    );
}

#[test]
fn empty_file_check_eval_simulate_all_refuse() {
    emath_syntax::install_source_parser();
    let dir = std::env::temp_dir().join(format!("emath-cli-empty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let empty = dir.join("empty.emath");
    let comments = dir.join("comments.emath");
    std::fs::write(&empty, "").expect("write empty");
    std::fs::write(&comments, "# comment only\n").expect("write comments");
    let mut session = emath_sema::CompilerSession::new(emath_core::limits::Limits::default());
    let package = session.load_package(&empty).expect("empty source loads");
    let result = session.check(package.file);
    let body = check_json_document(
        !result.diagnostics.has_errors(),
        &result.package.content_id().0,
        &result.diagnostics,
        None,
        &result.units_profiles,
    );
    assert!(
        diagnostic_codes(&body)
            .iter()
            .any(|code| code == "E-PKG-081"),
        "check --json empty must name E-PKG-081, got {body}"
    );
    assert_check_eval_simulate_refuse(&empty.to_string_lossy());
    assert_eq!(
        run(&[
            "expand".into(),
            empty.to_string_lossy().into_owned(),
            "--json".into()
        ]),
        EXIT_REFUSED,
        "expand must not admit empty source (E-PKG-081)"
    );
    assert_eq!(
        run(&[
            "expand".into(),
            comments.to_string_lossy().into_owned(),
            "--json".into()
        ]),
        EXIT_REFUSED,
        "expand must not admit comment-only source (E-PKG-081)"
    );
    let empty_path = empty.to_string_lossy().into_owned();
    for args in [
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
        assert_eq!(
            run(&args),
            EXIT_REFUSED,
            "empty source --json must be E-PKG-081 refused, got {args:?}"
        );
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
    assert!(
        diagnostic_codes(&envelope)
            .iter()
            .any(|code| code == "E-PKG-081"),
        "empty refusal envelope must name E-PKG-081, got {envelope}"
    );
    assert_check_eval_simulate_refuse(&comments.to_string_lossy());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_fixtures_check_eval_simulate_all_refuse() {
    emath_syntax::install_source_parser();
    for name in [
        "empty.emath",
        "duplicate_output.emath",
        "unit_mismatch.emath",
        "unknown_section.emath",
        "compile_junk.emath",
        "named_call_arg.emath",
    ] {
        assert_check_eval_simulate_refuse(&invalid_fixture(name));
    }
}

#[test]
fn simulate_mass_spring_accepts_vector_state_set() {
    emath_syntax::install_source_parser();
    let example = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../language/examples/numerical/explicit-mass-spring.emath");
    let path = example.to_string_lossy().into_owned();
    assert_eq!(
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
        "vector --set s=[1,0] must bind MassSpring state"
    );
    assert_eq!(
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
        "scalar --set s=1 must not silently stand in for Vector[2]"
    );
}

#[test]
fn simulate_scalar_set_still_binds() {
    emath_syntax::install_source_parser();
    let dir = std::env::temp_dir().join(format!("emath-cli-sim-scalar-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let spec = dir.join("decay.emath");
    std::fs::write(
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
    )
    .expect("write decay");
    assert_eq!(
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
        "scalar --set must keep working after vector bindings"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_accepts_implicit_and_symplectic_methods() {
    emath_syntax::install_source_parser();
    let models = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../language/examples/numerical/solver-methods.emath");
    assert_eq!(
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
        "backward Euler must be selectable through the public simulate command"
    );

    assert_eq!(
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
        "velocity Verlet must be selectable through the public simulate command"
    );
}

#[test]
fn notation_function_builds_to_an_artifact() {
    emath_syntax::install_source_parser();
    let dir = std::env::temp_dir().join(format!("emath-cli-notation-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let spec = dir.join("notation.emath");
    let out = dir.join("artifacts");
    std::fs::write(
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
    )
    .expect("write notation spec");
    assert_eq!(
        run(&["check".into(), spec.to_string_lossy().into_owned()]),
        EXIT_OK,
        "notation function must admit at check"
    );
    assert_eq!(
        run(&[
            "build".into(),
            spec.to_string_lossy().into_owned(),
            "--out".into(),
            out.to_string_lossy().into_owned(),
        ]),
        EXIT_OK,
        "notation function must build to an artifact"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reserved_notation_glyph_refused_at_build() {
    emath_syntax::install_source_parser();
    let dir =
        std::env::temp_dir().join(format!("emath-cli-notation-reject-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let spec = dir.join("reserved.emath");
    let out = dir.join("artifacts");
    std::fs::write(
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
    )
    .expect("write reserved spec");
    assert_eq!(
        run(&[
            "build".into(),
            spec.to_string_lossy().into_owned(),
            "--out".into(),
            out.to_string_lossy().into_owned()
        ]),
        EXIT_REFUSED,
        "reserved glyph must refuse at build"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
