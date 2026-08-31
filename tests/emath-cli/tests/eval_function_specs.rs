//! Bead `emath-cli-eval-function-specs-unuh` — conformance harness for
//! `emath eval` over ordinary admitted function specs.
//!
//! Contract under test: `emath eval` executes an admitted `emath
//! function` declaration through the EXISTING generic stack — sema
//! admission, `definition_order`, `lower_definition` (EMIR), and the
//! reference VM (`evaluate_with_budget`) — and returns a deterministic
//! receipt (JSON `emath.eval-function` or text) with the numeric result
//! and meaning-ID provenance. No genesis-only behavior for function
//! files, no second evaluator, no domain branch.
//!
//! Refusal closure (typed, on stdout in `--json` mode):
//! E-EVAL-001 unsupported entrypoint, E-EVAL-002 unknown named
//! entrypoint, E-EVAL-003 ambiguous entrypoint, E-EVAL-004 missing
//! input, E-EVAL-005 malformed/unknown/duplicate `--set`, E-EVAL-006
//! unsupported input type, E-EVAL-007 lowering/evaluation fault,
//! E-EVAL-008 `--world` on a function eval.
//!
//! Failure-first: every test in this file was written BEFORE the eval
//! function-spec path existed and ran red on the genesis-only refusal.

use std::path::{Path, PathBuf};
use std::process::Command;

use emath_cli::{run, CliExit, EXIT_OK, EXIT_REFUSED};

fn fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("emath-cli-eval-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write_fixture(dir: &PathBuf, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write fixture");
    path
}

fn hello_square() -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../language/examples/intro/hello-square.emath")
        .to_string_lossy()
        .into_owned()
}

/// The standard function spec used across this harness: `Square`, one
/// Float64 input `x`, one output `y = x * x`, with an existing
/// `example <three_squared>: given x = 3, expect y == 9` test (the
/// spec's own oracle).
const SQUARE_SPEC: &str = "\
emath function Square:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
";

const TWO_FUNCTION_SPEC: &str = "\
emath function A:
    inputs:
        x: Float64
    outputs:
        r: Float64
    definitions:
        r = x + 1.0

emath function B:
    inputs:
        x: Float64
    outputs:
        r: Float64
    definitions:
        r = x * 2.0
";

const TWO_INPUT_SPEC: &str = "\
emath function Add:
    inputs:
        a: Float64
        b: Float64
    outputs:
        r: Float64
    definitions:
        r = a + b
";

const FAILING_EXAMPLE_SPEC: &str = "\
emath function Wrong:
    inputs:
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x * x
    tests:
        example <expects_one>:
            given x = 3
            expect y == 1
";

const MODEL_ONLY_SPEC: &str = "\
emath model Decay:
    inputs:
        k: Float64
    state:
        x: Float64
    equations:
        der(x) = -k * x
";

/// Spawn the real binary for `eval <file> [...]` and return parsed
/// stdout JSON + exit code. Mirrors `json_output` in the sibling suite
/// so the assertions cover what the binary actually prints. Runs the
/// prebuilt `emath` binary directly (never `cargo run` per invocation):
/// parallel cargo invocations interleave build chatter with stdout and
/// splice the receipt frames (observed in CI-style parallel runs).
fn eval_json(args: &[&str]) -> (emath_artifact::JsonValue, CliExit, String) {
    let output = Command::new(eval_binary())
        .arg("eval")
        .args(args)
        .output()
        .expect("run emath binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = match output.status.code() {
        Some(0) => EXIT_OK,
        Some(1) => EXIT_REFUSED,
        _ => CliExit::Usage,
    };
    let parsed = match emath_artifact::parse_json_document(&stdout) {
        Ok(parsed) => parsed,
        Err(error) => panic!("stdout must be valid JSON for eval {args:?}: {error}\n{stdout}"),
    };
    (parsed, code, stdout)
}

/// Path to the `emath` binary for this workspace, built once into the
/// Cargo target dir: prefer the explicit `CARGO_TARGET_DIR` the test
/// batch used, then the workspace default `target/debug`. Building here
/// happens exactly once per test process via `cargo build -p emath-cli`.
fn eval_binary() -> PathBuf {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let candidate = |target: &Path| target.join("debug/emath");
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        let bin = candidate(Path::new(&dir));
        if bin.is_file() {
            return bin;
        }
    }
    let default = candidate(&workspace.join("target"));
    if !default.is_file() {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "emath-cli"])
            .current_dir(&workspace)
            .status()
            .expect("build emath");
        assert!(status.success(), "cargo build -p emath-cli must succeed");
    }
    default
}

fn error_codes(parsed: &emath_artifact::JsonValue) -> Vec<String> {
    match parsed.field("diagnostics") {
        Ok(emath_artifact::JsonValue::Arr(items)) => items
            .iter()
            .map(|item| item.string_field("code").expect("code"))
            .collect(),
        Ok(other) => panic!("diagnostics must be array, got {other:?}"),
        Err(_) => Vec::new(),
    }
}

/// Read a string entry from a JSON object value (`Obj` is a
/// `Vec<(String, JsonValue)>` here, insertion-ordered).
fn obj_string(parsed: &emath_artifact::JsonValue, object_key: &str, entry: &str) -> Option<String> {
    match parsed.field(object_key) {
        Ok(emath_artifact::JsonValue::Obj(fields)) => fields
            .iter()
            .find(|(key, _)| key == entry)
            .map(|(_, value)| match value {
                emath_artifact::JsonValue::Str(text) => text.clone(),
                other => panic!("{object_key}.{entry} must be a string, got {other:?}"),
            }),
        Ok(other) => panic!("{object_key} must be an object, got {other:?}"),
        Err(_) => None,
    }
}

/// PASS 1 RED / PASS 2+ GREEN — plain `emath eval` (no `--set`) on a
/// standard function spec runs the spec's own worked example as the
/// input oracle: nothing is invented, so `hello-square`'s
/// `example <three_squared>: given x = 3` binds `x=3` and the
/// `expect y == 9` must hold. This is what makes the old
/// `eval_json_refuses_invalid_function_file` pin (genesis-only refusal)
/// flip to EXIT_OK.
#[test]
fn eval_plain_single_example_uses_spec_oracle() {
    emath_syntax::install_source_parser();
    let (parsed, code, stdout) = eval_json(&[&hello_square(), "--json"]);
    assert_eq!(code, EXIT_OK, "plain eval runs the spec oracle; {stdout}");
    assert_eq!(
        parsed.string_field("schema").expect("schema id"),
        "emath.eval-function"
    );
    assert_eq!(parsed.string_field("function").expect("function"), "Square");
    assert_eq!(
        obj_string(&parsed, "inputs", "x").as_deref(),
        Some("3.0"),
        "oracle binds x from the spec's own example: {stdout}"
    );
    assert_eq!(
        obj_string(&parsed, "outputs", "y").as_deref(),
        Some("9.0"),
        "oracle run evaluates the body: {stdout}"
    );
}

/// PASS 1 RED / PASS 2+ GREEN — plain `emath eval` on a spec whose
/// single worked example FAILS its expect is a typed refusal
/// (E-EVAL-007), never a silent numeric answer: the example is the
/// oracle, and a failing oracle means the spec's own claim is false.
#[test]
fn eval_failing_example_refuses_typed() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("failing-example");
    let path = write_fixture(&dir, "wrong.emath", FAILING_EXAMPLE_SPEC);
    let path = path.to_string_lossy().into_owned();
    let (parsed, code, stdout) = eval_json(&[&path, "--json"]);
    assert_eq!(code, EXIT_REFUSED, "failing example must refuse; {stdout}");
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-007"),
        "failing example refuses E-EVAL-007, got {:?}",
        error_codes(&parsed)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// METAMORPHIC — determinism: the same evaluation repeated yields
/// byte-identical receipts, and `--set` arrangement order is a
/// permutation that must not change the emitted document (all bindings
/// render in sorted name order).
#[test]
fn eval_receipt_deterministic_repeats() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("determinism");
    let add = write_fixture(&dir, "add.emath", TWO_INPUT_SPEC);
    let add = add.to_string_lossy().into_owned();
    let (_, code_1, stdout_1) = eval_json(&[&add, "--set", "a=2", "--set", "b=3", "--json"]);
    let (_, code_2, stdout_2) = eval_json(&[&add, "--set", "a=2", "--set", "b=3", "--json"]);
    assert_eq!(code_1, EXIT_OK);
    assert_eq!(code_2, EXIT_OK);
    assert_eq!(
        stdout_1, stdout_2,
        "repeat evaluation must be byte-identical"
    );
    // Permutation invariance: reversed --set order changes only the
    // command line, never the receipt.
    let (parsed, code_3, stdout_3) = eval_json(&[&add, "--set", "b=3", "--set", "a=2", "--json"]);
    assert_eq!(code_3, EXIT_OK);
    assert_eq!(stdout_3, stdout_1, "--set order must not change the receipt");
    assert_eq!(
        obj_string(&parsed, "outputs", "r").as_deref(),
        Some("5.0"),
        "Add(2,3) == 5.0: {stdout_3}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// METAMORPHIC — the malformed `--set` refusal closure: duplicate names
/// and non-numeric payloads are typed E-EVAL-005, and a scalar bound to
/// a vector slot (or the reverse) is E-EVAL-006; the genesis lane is
/// untouched (`--world` + function flags is E-EVAL-008).
#[test]
fn eval_set_closure_refuses_typed() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("set-closure");
    let add = write_fixture(&dir, "add.emath", TWO_INPUT_SPEC);
    let add = add.to_string_lossy().into_owned();
    let (parsed, code, _) = eval_json(&[&add, "--set", "a=2", "--set", "a=3", "--json"]);
    assert_eq!(code, EXIT_REFUSED);
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-005"),
        "duplicate --set refuses E-EVAL-005, got {:?}",
        error_codes(&parsed)
    );
    let (parsed, code, _) = eval_json(&[&add, "--set", "a=banana", "--json"]);
    assert_eq!(code, EXIT_REFUSED);
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-005"),
        "non-numeric --set refuses E-EVAL-005, got {:?}",
        error_codes(&parsed)
    );
    let (parsed, code, _) = eval_json(&[&add, "--set", "a=[1,2]", "--set", "b=3", "--json"]);
    assert_eq!(code, EXIT_REFUSED);
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-006"),
        "vector bound to a Float64 slot refuses E-EVAL-006, got {:?}",
        error_codes(&parsed)
    );
    let (parsed, code, _) = eval_json(&[&hello_square(), "--world", "free_symbolic", "--set", "x=3", "--json"]);
    assert_eq!(code, EXIT_REFUSED);
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-008"),
        "--world with function flags refuses E-EVAL-008, got {:?}",
        error_codes(&parsed)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// PASS 1 RED / PASS 2+ GREEN — the standard function spec evaluates
/// with the ACTUAL numeric result of its body: `Square(3) == 9`.
/// Before the path existed this file refused as genesis-only
/// (`EXIT_REFUSED`, E-GEN-080), so this test ran red with the right
/// discrimination.
#[test]
fn eval_standard_function_spec_returns_numeric_result() {
    emath_syntax::install_source_parser();
    let (parsed, code, stdout) = eval_json(&[&hello_square(), "--set", "x=3", "--json"]);
    assert_eq!(code, EXIT_OK, "Square(3) must eval; stdout:\n{stdout}");
    assert_eq!(
        parsed.string_field("schema").expect("schema id"),
        "emath.eval-function"
    );
    assert_eq!(parsed.string_field("function").expect("function"), "Square");
    assert_eq!(
        obj_string(&parsed, "outputs", "y").as_deref(),
        Some("9.0"),
        "Square(3) == 9.0 (rendered exact): {stdout}"
    );
    // Receipt carries provenance: the meaning id of the admitted
    // function spec, deterministic (see determinism MR).
    assert!(
        parsed.field("meaning_id").is_ok(),
        "receipt carries meaning_id provenance"
    );
    // MUTATION-KILL PAIR: an implementation that substitutes a constant
    // for execution must die. With x=3 the constant 9.0 is
    // indistinguishable from the true square, so the oracle must ALSO
    // evaluate a second distinct input: a constant-9 mutant gives
    // 9.0 != 16.0 here and fails; an echo mutant (y = x) gives 4.0 and
    // fails both runs.
    let (parsed, code, stdout) = eval_json(&[&hello_square(), "--set", "x=4", "--json"]);
    assert_eq!(code, EXIT_OK, "Square(4) must eval; stdout:\n{stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "y").as_deref(),
        Some("16.0"),
        "Square(4) == 16.0 (constant/echo mutants must fail): {stdout}"
    );
}

/// PASS 1 RED / PASS 2+ GREEN — binding only one of two inputs is a
/// typed missing-input refusal (E-EVAL-004), never a partial eval.
#[test]
fn eval_missing_input_refuses_typed() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("missing-input");
    let path = write_fixture(&dir, "add.emath", TWO_INPUT_SPEC);
    let path = path.to_string_lossy().into_owned();
    let (parsed, code, stdout) = eval_json(&[&path, "--set", "a=2", "--json"]);
    assert_eq!(code, EXIT_REFUSED, "missing input must refuse; {stdout}");
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-004"),
        "missing input refuses E-EVAL-004, got {:?}",
        error_codes(&parsed)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// PASS 1 RED / PASS 2+ GREEN — two functions without `--function` is
/// an ambiguous-entrypoint refusal (E-EVAL-003), never a silent pick.
#[test]
fn eval_ambiguous_entrypoint_refuses_typed() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("ambiguous");
    let path = write_fixture(&dir, "two.emath", TWO_FUNCTION_SPEC);
    let path = path.to_string_lossy().into_owned();
    let (parsed, code, stdout) = eval_json(&[&path, "--set", "x=2", "--json"]);
    assert_eq!(code, EXIT_REFUSED, "ambiguous entrypoint must refuse; {stdout}");
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-003"),
        "ambiguity refuses E-EVAL-003, got {:?}",
        error_codes(&parsed)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// PASS 1 RED / PASS 2+ GREEN — `--function` naming nothing is a typed
/// refusal (E-EVAL-002), and naming the OTHER function selects it.
#[test]
fn eval_unknown_named_entrypoint_refuses_typed() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("unknown-fn");
    let path = write_fixture(&dir, "two.emath", TWO_FUNCTION_SPEC);
    let path = path.to_string_lossy().into_owned();
    let (parsed, code, stdout) = eval_json(&[&path, "--function", "Nope", "--set", "x=2", "--json"]);
    assert_eq!(code, EXIT_REFUSED, "unknown --function must refuse; {stdout}");
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-002"),
        "unknown named entrypoint refuses E-EVAL-002, got {:?}",
        error_codes(&parsed)
    );
    // Selecting a real function by name is the disambiguation path.
    let (parsed, code, stdout) =
        eval_json(&[&path, "--function", "B", "--set", "x=2", "--json"]);
    assert_eq!(code, EXIT_OK, "named entrypoint evals; {stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "r").as_deref(),
        Some("4.0"),
        "B(2) == 4.0: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// PASS 1 RED / PASS 2+ GREEN — a file whose declarations are not
/// function specs (model) refuses E-EVAL-001 (unsupported entrypoint).
#[test]
fn eval_unsupported_entrypoint_refuses_typed() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("model-only");
    let path = write_fixture(&dir, "decay.emath", MODEL_ONLY_SPEC);
    let path = path.to_string_lossy().into_owned();
    let (parsed, code, stdout) = eval_json(&[&path, "--set", "k=1", "--json"]);
    assert_eq!(code, EXIT_REFUSED, "model entrypoint must refuse; {stdout}");
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-001"),
        "unsupported entrypoint refuses E-EVAL-001, got {:?}",
        error_codes(&parsed)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// PASS 1 RED / PASS 2+ GREEN — in-process exit-code contract through
/// `run`: a function spec eval succeeds, and the typed refusals exit
/// `EXIT_REFUSED`, never a silent zero.
#[test]
fn eval_in_process_exit_codes() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("exit-codes");
    let square = write_fixture(&dir, "square.emath", SQUARE_SPEC);
    let add = write_fixture(&dir, "add.emath", TWO_INPUT_SPEC);
    let two = write_fixture(&dir, "two.emath", TWO_FUNCTION_SPEC);
    let square = square.to_string_lossy().into_owned();
    let add = add.to_string_lossy().into_owned();
    let two = two.to_string_lossy().into_owned();

    assert_eq!(
        run(&["eval".into(), square.clone(), "--set".into(), "x=3".into()]),
        EXIT_OK,
        "in-process eval of a function spec succeeds"
    );
    assert_eq!(
        run(&["eval".into(), square.clone(), "--set".into(), "x=3".into(), "--json".into()]),
        EXIT_OK
    );
    assert_eq!(
        run(&["eval".into(), add, "--set".into(), "a=2".into()]),
        EXIT_REFUSED,
        "missing input refuses"
    );
    assert_eq!(
        run(&["eval".into(), two.clone(), "--set".into(), "x=2".into()]),
        EXIT_REFUSED,
        "ambiguous entrypoint refuses"
    );
    assert_eq!(
        run(&["eval".into(), two, "--function".into(), "Nope".into(), "--set".into(), "x=2".into()]),
        EXIT_REFUSED,
        "unknown entrypoint refuses"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
