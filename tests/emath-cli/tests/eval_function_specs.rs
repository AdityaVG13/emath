//! — conformance harness for
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

use std::path::PathBuf;

mod common;
use std::process::Command;

use emath_cli::{CliExit, EXIT_OK, EXIT_REFUSED, run};

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
    let output = Command::new(common::emath_bin())
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

/// Plain `emath eval` (no `--set`) on a
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

/// Plain `emath eval` on a spec whose
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
    assert_eq!(
        stdout_3, stdout_1,
        "--set order must not change the receipt"
    );
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
    let (parsed, code, _) = eval_json(&[
        &hello_square(),
        "--world",
        "free_symbolic",
        "--set",
        "x=3",
        "--json",
    ]);
    assert_eq!(code, EXIT_REFUSED);
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-008"),
        "--world with function flags refuses E-EVAL-008, got {:?}",
        error_codes(&parsed)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The standard function spec evaluates
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

/// Binding only one of two inputs is a
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

/// Two functions without `--function` is
/// an ambiguous-entrypoint refusal (E-EVAL-003), never a silent pick.
#[test]
fn eval_ambiguous_entrypoint_refuses_typed() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("ambiguous");
    let path = write_fixture(&dir, "two.emath", TWO_FUNCTION_SPEC);
    let path = path.to_string_lossy().into_owned();
    let (parsed, code, stdout) = eval_json(&[&path, "--set", "x=2", "--json"]);
    assert_eq!(
        code, EXIT_REFUSED,
        "ambiguous entrypoint must refuse; {stdout}"
    );
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-003"),
        "ambiguity refuses E-EVAL-003, got {:?}",
        error_codes(&parsed)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Function-file auto-detect (emath-tmd95): a multi-function file
/// without `--function` emits ONE actionable line (count + names), not
/// the E-GEN-080/E-SYN-20x cascade; a sole-function file with `--set`
/// auto-selects; genesis-header files are untouched by the hint path.
#[test]
fn eval_function_file_hint_contract() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("function-hint");

    // Multi-function file: the refusal names the count and the candidates.
    let two = write_fixture(&dir, "two.emath", TWO_FUNCTION_SPEC);
    let two = two.to_string_lossy().into_owned();
    let (parsed, code, stdout) = eval_json(&[&two, "--set", "x=2", "--json"]);
    assert_eq!(code, EXIT_REFUSED);
    let message = match parsed.field("diagnostics") {
        Ok(emath_artifact::JsonValue::Arr(items)) => items
            .iter()
            .filter_map(|item| match item {
                emath_artifact::JsonValue::Obj(fields) => fields
                    .iter()
                    .find(|(key, _)| key == "message")
                    .map(|(_, value)| match value {
                        emath_artifact::JsonValue::Str(text) => text.clone(),
                        other => panic!("message must be a string, got {other:?}"),
                    }),
                other => panic!("diagnostic must be an object, got {other:?}"),
            })
            .collect::<Vec<_>>(),
        other => panic!("diagnostics must be an array, got {other:?}"),
    };
    let hint = message.join("\n");
    assert!(
        hint.contains("2 function declarations share this file"),
        "hint must name the count; got: {hint}"
    );
    assert!(
        hint.contains("--function") && hint.contains("A") && hint.contains("B"),
        "hint must point at --function and name the candidates; got: {hint}"
    );
    assert!(
        !stdout.contains("E-GEN-080") && !stdout.contains("E-SYN-2"),
        "no genesis/parse cascade on the hint path; got: {stdout}"
    );

    // Sole-function file with --set: auto-selects, no --function needed.
    let sole = write_fixture(&dir, "one.emath", SQUARE_SPEC);
    let sole = sole.to_string_lossy().into_owned();
    let (_, code, stdout) = eval_json(&[&sole, "--set", "x=3", "--json"]);
    assert_eq!(
        code, EXIT_OK,
        "sole function auto-selects with --set; {stdout}"
    );

    // Genesis-header file: the hint path is dormant (plain probe evals).
    let genesis = write_fixture(&dir, "gen.emath", "probe = 1 + 1\n");
    let genesis = genesis.to_string_lossy().into_owned();
    let (_, code, stdout) = eval_json(&[&genesis, "--json"]);
    assert_eq!(
        code, EXIT_OK,
        "genesis-header file untouched by the hint; {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--function` naming nothing is a typed
/// refusal (E-EVAL-002), and naming the OTHER function selects it.
#[test]
fn eval_unknown_named_entrypoint_refuses_typed() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("unknown-fn");
    let path = write_fixture(&dir, "two.emath", TWO_FUNCTION_SPEC);
    let path = path.to_string_lossy().into_owned();
    let (parsed, code, stdout) =
        eval_json(&[&path, "--function", "Nope", "--set", "x=2", "--json"]);
    assert_eq!(
        code, EXIT_REFUSED,
        "unknown --function must refuse; {stdout}"
    );
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-002"),
        "unknown named entrypoint refuses E-EVAL-002, got {:?}",
        error_codes(&parsed)
    );
    // Selecting a real function by name is the disambiguation path.
    let (parsed, code, stdout) = eval_json(&[&path, "--function", "B", "--set", "x=2", "--json"]);
    assert_eq!(code, EXIT_OK, "named entrypoint evals; {stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "r").as_deref(),
        Some("4.0"),
        "B(2) == 4.0: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file whose declarations are not
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

/// In-process exit-code contract through
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
        run(&[
            "eval".into(),
            square.clone(),
            "--set".into(),
            "x=3".into(),
            "--json".into()
        ]),
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
        run(&[
            "eval".into(),
            two,
            "--function".into(),
            "Nope".into(),
            "--set".into(),
            "x=2".into()
        ]),
        EXIT_REFUSED,
        "unknown entrypoint refuses"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A sibling-function call whose callee body contains a binder that
/// references a parameter must resolve that parameter through the
/// caller-bound argument: the renamed parameter (`y#count_below`) is
/// bound as a definition over the argument subtree, and the inlining
/// pass must reach INTO binder bodies to substitute it. Regression
/// for emath-87ls0: the renamed parameter survived inside the fold
/// guard and the runner refused with E-EVAL-007 "unknown input
/// `y#count_below`". The companion callee (parameter in the binder
/// DOMAIN) already resolved and guards the domain path.
const SIBLING_BINDER_SPEC: &str = "\
emath function count_to:
    inputs:
        y: Int
    outputs:
        r: Nat
    definitions:
        r = sum j in 0..y: 1

emath function count_below:
    inputs:
        y: Int
    outputs:
        r: Nat
    definitions:
        r = sum j in 0..6 if j < y: 1

emath function caller:
    inputs:
        x: Int
    outputs:
        a: Int
        b: Int
    definitions:
        a = count_to(x)
        b = count_below(x)
";

#[test]
fn eval_sibling_call_with_binder_body_resolves_arguments() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("sibling-binder-body");
    let path = write_fixture(&dir, "caller_binder.emath", SIBLING_BINDER_SPEC);
    let path = path.to_string_lossy().into_owned();
    // count_to(4) = |{0,1,2,3}| = 4 (binder domain path);
    // count_below(4) = |{0,1,2,3}| = 4 (binder body path, emath-87ls0).
    let (parsed, code, stdout) =
        eval_json(&[&path, "--function", "caller", "--set", "x=4", "--json"]);
    assert_eq!(code, EXIT_OK, "caller(4) must eval; stdout:\n{stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "a").as_deref(),
        Some("4"),
        "count_to(4) == 4 (binder domain path): {stdout}"
    );
    assert_eq!(
        obj_string(&parsed, "outputs", "b").as_deref(),
        Some("4"),
        "count_below(4) == 4 (binder body path, emath-87ls0): {stdout}"
    );
    // MUTATION-KILL PAIR: a callee that ignores its argument (constant
    // fold) must fail on a second, distinct input.
    let (parsed, code, stdout) =
        eval_json(&[&path, "--function", "caller", "--set", "x=2", "--json"]);
    assert_eq!(code, EXIT_OK, "caller(2) must eval; stdout:\n{stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "b").as_deref(),
        Some("2"),
        "count_below(2) == 2 (constant mutant must fail): {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Multi-binder folds (emath-6kk1b) ────────────────────────────────────
//
// `sum i in 0..n, j in 0..m: body` desugars to nested single-binder
// folds, leftmost outermost / rightmost innermost; the guard binds to
// the innermost binder. Failure-first: written before the desugar and
// ran red on the E-TYPE-010 single-binder refusal.

const MULTI_BINDER_SPEC: &str = "\
emath function grid:
    inputs:
        n: Int
        m: Int
    outputs:
        cells: Int
    definitions:
        cells = sum i in 0..n, j in 0..m: 1

emath function weighted:
    inputs:
        n: Int
    outputs:
        s: Int
    definitions:
        s = sum i in 0..n, j in 0..n: i * j

emath function filtered:
    inputs:
        n: Int
    outputs:
        evens: Int
    definitions:
        evens = sum i in 0..n, j in 0..n if int_rem(i + j, 2) == 0: 1

emath function exists_pair:
    inputs:
        n: Int
    outputs:
        found: Bool
    definitions:
        found = exists i in 0..n, j in 0..n: i * j == 6

emath function triangle:
    inputs:
        n: Int
    outputs:
        t: Int
    definitions:
        t = sum i in 0..n, j in 0..i: 1
";

#[test]
fn eval_multi_binder_folds_desugar_to_nested() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("multi-binder");
    let path = write_fixture(&dir, "multi_binder.emath", MULTI_BINDER_SPEC);
    let path = path.to_string_lossy().into_owned();

    // grid(3, 4) = 3·4 = 12: the product of extents, not 3 + 4.
    let (parsed, code, stdout) = eval_json(&[
        &path,
        "--function",
        "grid",
        "--set",
        "n=3",
        "--set",
        "m=4",
        "--json",
    ]);
    assert_eq!(code, EXIT_OK, "grid(3,4) must eval; stdout:\n{stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "cells").as_deref(),
        Some("12"),
        "double sum of 1 over 3×4 == 12: {stdout}"
    );

    // weighted(4) = (Σi)(Σj) with i·j over the grid = 6·6 = 36; nesting
    // ORDER is observable: swapping binders gives the same value for a
    // symmetric body, so a second, asymmetric body pins the convention.
    let (parsed, code, stdout) =
        eval_json(&[&path, "--function", "weighted", "--set", "n=4", "--json"]);
    assert_eq!(code, EXIT_OK, "weighted(4) must eval; stdout:\n{stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "s").as_deref(),
        Some("36"),
        "sum i,j of i·j over 0..4 == 36: {stdout}"
    );

    // Guard binds to the INNERMOST binder: even-parity cells of a 3×3
    // grid = 5 ((0,0),(0,2),(1,1),(2,0),(2,2)).
    let (parsed, code, stdout) =
        eval_json(&[&path, "--function", "filtered", "--set", "n=3", "--json"]);
    assert_eq!(code, EXIT_OK, "filtered(3) must eval; stdout:\n{stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "evens").as_deref(),
        Some("5"),
        "guarded double sum == 5 (guard on innermost j): {stdout}"
    );

    // exists composes associatively: 2·3 = 6 IS reachable in 0..4².
    let (parsed, code, stdout) =
        eval_json(&[&path, "--function", "exists_pair", "--set", "n=4", "--json"]);
    assert_eq!(code, EXIT_OK, "exists_pair(4) must eval; stdout:\n{stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "found").as_deref(),
        Some("true"),
        "exists i,j: i·j == 6 must be true for n=4: {stdout}"
    );

    // ORDER PIN (mutation kill): the inner domain references the outer
    // binder. Rightmost-innermost gives the triangle sum Σ_{i<4} i = 6
    // (0+1+2+3 over the half-open domains); a reversed desugar cannot
    // lower this shape.
    let (parsed, code, stdout) =
        eval_json(&[&path, "--function", "triangle", "--set", "n=4", "--json"]);
    assert_eq!(code, EXIT_OK, "triangle(4) must eval; stdout:\n{stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "t").as_deref(),
        Some("6"),
        "triangle(4) == 6 pins rightmost-innermost nesting: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Mutation kill for the desugar: if the synthesized nesting order were
/// reversed (leftmost innermost), a domain whose INNER bound references
/// the OUTER binder would still work, but a guard referencing the
/// outermost binder would resolve to the wrong scope. `outer_ref` uses
/// the outer binder inside the guard — legal under innermost-binding
/// only when the outer variable is still in scope (it is; outer refs
/// stay legal), and the count differs from an innermost-only reading.
const OUTER_REF_SPEC: &str = "\
emath function outer_ref:
    inputs:
        n: Int
    outputs:
        r: Int
    definitions:
        r = sum i in 0..n, j in 0..n if j == i: 1
";

#[test]
fn eval_multi_binder_guard_outer_reference_scopes_correctly() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("multi-binder-outer-ref");
    let path = write_fixture(&dir, "outer_ref.emath", OUTER_REF_SPEC);
    let path = path.to_string_lossy().into_owned();
    // sum over the diagonal: exactly n matches (j == i), so outer_ref(4) = 4.
    let (parsed, code, stdout) =
        eval_json(&[&path, "--function", "outer_ref", "--set", "n=4", "--json"]);
    assert_eq!(code, EXIT_OK, "outer_ref(4) must eval; stdout:\n{stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "r").as_deref(),
        Some("4"),
        "diagonal count outer_ref(4) == 4: {stdout}"
    );
    // Second distinct input kills constant mutants.
    let (parsed, code, stdout) =
        eval_json(&[&path, "--function", "outer_ref", "--set", "n=2", "--json"]);
    assert_eq!(code, EXIT_OK, "outer_ref(2) must eval; stdout:\n{stdout}");
    assert_eq!(
        obj_string(&parsed, "outputs", "r").as_deref(),
        Some("2"),
        "diagonal count outer_ref(2) == 2 (constant mutant must fail): {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Vector[Int]/Vector[Nat] --set bindings (emath-5rmwr) ────────────────
//
// Integer-vector inputs are the natural shape for probe witnesses
// (codewords, coefficient vectors, domain points). Failure-first:
// written before the widened binding and ran red on E-EVAL-006.

const INT_VECTOR_SPEC: &str = "\
emath function witness_sum:
    inputs:
        v: Vector[Int]
    outputs:
        s: Int
    definitions:
        s = v[0] + v[1] + v[2]

emath function nat_code:
    inputs:
        c: Vector[Nat]
    outputs:
        m: Int
    definitions:
        m = c[0] * 10 + c[1]
";

#[test]
fn eval_set_binds_integer_vectors() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("int-vector-set");
    let path = write_fixture(&dir, "int_vector.emath", INT_VECTOR_SPEC);
    let path = path.to_string_lossy().into_owned();

    // Vector[Int] binds and computes exactly.
    let (parsed, code, stdout) = eval_json(&[
        &path,
        "--function",
        "witness_sum",
        "--set",
        "v=[3, 4, 5]",
        "--json",
    ]);
    assert_eq!(
        code, EXIT_OK,
        "witness_sum([3,4,5]) must eval; stdout:\n{stdout}"
    );
    assert_eq!(
        obj_string(&parsed, "outputs", "s").as_deref(),
        Some("12"),
        "v[0]+v[1]+v[2] == 12: {stdout}"
    );

    // Vector[Nat] binds and computes exactly.
    let (parsed, code, stdout) = eval_json(&[
        &path,
        "--function",
        "nat_code",
        "--set",
        "c=[4, 2]",
        "--json",
    ]);
    assert_eq!(
        code, EXIT_OK,
        "nat_code([4,2]) must eval; stdout:\n{stdout}"
    );
    assert_eq!(
        obj_string(&parsed, "outputs", "m").as_deref(),
        Some("42"),
        "c[0]*10 + c[1] == 42: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn eval_set_integer_vector_strictness() {
    emath_syntax::install_source_parser();
    let dir = fixture_dir("int-vector-strict");
    let path = write_fixture(&dir, "int_vector_strict.emath", INT_VECTOR_SPEC);
    let path = path.to_string_lossy().into_owned();

    // Strict parsing: a fractional element refuses typed (E-EVAL-006
    // shape mismatch — the value parses as a vector but not for THIS
    // declared element type).
    let (parsed, code, _) = eval_json(&[
        &path,
        "--function",
        "witness_sum",
        "--set",
        "v=[1, 2.5, 3]",
        "--json",
    ]);
    assert_eq!(code, EXIT_REFUSED);
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-006"),
        "fractional element in Vector[Int] refuses E-EVAL-006, got {:?}",
        error_codes(&parsed)
    );

    // A negative element refuses for Vector[Nat].
    let (parsed, code, _) = eval_json(&[
        &path,
        "--function",
        "nat_code",
        "--set",
        "c=[-1, 2]",
        "--json",
    ]);
    assert_eq!(code, EXIT_REFUSED);
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-006"),
        "negative element in Vector[Nat] refuses E-EVAL-006, got {:?}",
        error_codes(&parsed)
    );

    // A Vector[Int] slot still refuses a Vector[Float64] payload (1.5).
    let (parsed, code, _) = eval_json(&[
        &path,
        "--function",
        "witness_sum",
        "--set",
        "v=[1.5, 2, 3]",
        "--json",
    ]);
    assert_eq!(code, EXIT_REFUSED);
    assert!(
        error_codes(&parsed).iter().any(|code| code == "E-EVAL-006"),
        "float vector in Vector[Int] slot refuses E-EVAL-006, got {:?}",
        error_codes(&parsed)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
