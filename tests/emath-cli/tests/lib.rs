//! CLI ergonomics tests, moved from `crates/emath-cli/src/lib.rs`.

use emath_cli::{EXIT_OK, EXIT_REFUSED, EXIT_USAGE, run};

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
    assert_eq!(run(&args("robot-docs waffle")), EXIT_USAGE);
}

#[test]
fn read_side_json_and_triage_help_exit_ok() {
    assert_eq!(run(&args("architecture --json")), EXIT_OK);
    assert_eq!(run(&args("doctor --json")), EXIT_OK);
    assert_eq!(run(&args("provider list --json")), EXIT_OK);
    assert_eq!(run(&args("agent triage --help")), EXIT_OK);
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
        run(&["simulate".into(), path, "--json".into()]),
        EXIT_REFUSED
    );
}

#[test]
fn eval_json_refuses_invalid_function_file() {
    emath_syntax::install_source_parser();
    let example = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../language/examples/intro/hello-square.emath");
    let path = example.to_string_lossy().into_owned();
    assert_eq!(
        run(&["check".into(), path.clone(), "--json".into()]),
        EXIT_OK,
        "hello-square must admit at check"
    );
    assert_eq!(
        run(&["eval".into(), path, "--json".into()]),
        EXIT_REFUSED,
        "eval is genesis-only; a function file must not silently succeed"
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
    assert_check_eval_simulate_refuse(&empty.to_string_lossy());
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
