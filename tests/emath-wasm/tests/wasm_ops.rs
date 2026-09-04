//! End-to-end tests for the emath-wasm op surface.
//!
//! Migrated from the in-crate `#[cfg(test)]` module: the op entry point
//! (`run_op`) and every fixture it serves are public crate surface, so
//! these exercise the API exactly as a wasm host would.

use std::collections::BTreeMap;

use emath_artifact::{JsonValue, JsonWriter, parse_json_document};
use emath_core::Severity;
use emath_exec_ir::interp::{Value, format_f64};
use emath_exec_ir::runner::run_package_with_given;
use emath_wasm::*;

fn field_contains(json: &str, name: &str, needle: &str) -> bool {
    let key = format!("\"{name}\":");
    json.contains(&key) && json.contains(needle)
}

#[test]
fn version_op_shape() {
    let json = run_op("version", "");
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(
        field_contains(&json, "version", env!("CARGO_PKG_VERSION")),
        "{json}"
    );
    assert!(json.contains("\"abi\": 1"), "{json}");
}

#[test]
fn check_hello_square_admits() {
    let json = run_op("check", HELLO_SQUARE);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"admitted\": true"), "{json}");
    assert!(json.contains("\"diagnostics\": []"), "{json}");
    assert!(json.contains("\"Square\""), "{json}");
}

#[test]
fn run_vector_given_computes() {
    let json = run_op("run", VECTOR_GIVEN);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"first\": 1.0"), "{json}");
    assert!(json.contains("\"mag_sq\": 14.0"), "{json}");
    assert!(json.contains("\"scaled\": [2.0, 4.0, 6.0]"), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
}

#[test]
fn run_envelope_vector_given_computes() {
    let source = "\nemath function VecPane:\n    inputs:\n        v: Vector[3]\n\n    outputs:\n        first: Float64\n        mag_sq: Float64\n\n    definitions:\n        first = v[0]\n        mag_sq = dot(v, v)\n";
    let json = run_envelope(source, Some(&[("v", "[1.0, 2.0, 3.0]")]));
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"_pane\""), "{json}");
    assert!(json.contains("\"computed\": true"), "{json}");
    assert!(json.contains("\"first\": 1.0"), "{json}");
    assert!(json.contains("\"mag_sq\": 14.0"), "{json}");
}

#[test]
fn run_factorial_inclusive_computes() {
    let json = run_op("run", FACTORIAL);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"fac\": 120.0"), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
}

#[test]
fn run_range_sum_computes() {
    let json = run_op("run", RANGE_SUM);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"s\": 6.0"), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
}

#[test]
fn run_forall_exists_computes() {
    let json = run_op("run", FORALL_EXISTS);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"all_positive\": false"), "{json}");
    assert!(json.contains("\"has_zero\": true"), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
}

#[test]
fn run_integral_computes() {
    let json = run_op("run", INTEGRAL);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"area\":"), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
}

#[test]
fn run_autodiff_computes() {
    let json = run_op("run", AUTODIFF);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"dy\": 6.0"), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
}

#[test]
fn run_solve_computes() {
    let json = run_op("run", SOLVE);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"root\":"), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
}

#[test]
fn run_constrained_opt_computes() {
    let json = run_op("run", CONSTRAINED_OPT);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"opt_x\":"), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
}

#[test]
fn run_optimize_computes() {
    let json = run_op("run", OPTIMIZE);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"min_x\":"), "{json}");
    assert!(json.contains("\"max_x\":"), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
}

#[test]
fn curated_non_demo_examples_admit() {
    for (name, source) in curated_examples() {
        if name.contains("Diagnostics") || *name == "diagnostics demo" {
            continue;
        }
        let json = run_op("check", source);
        assert!(json.contains("\"ok\": true"), "{name}: {json}");
        assert!(json.contains("\"admitted\": true"), "{name}: {json}");
        // Advisory diagnostics (E-SEC-133's visible-default note on
        // constant computations) are by design; a curated example
        // must not carry ERROR-severity diagnostics.
        assert!(!json.contains("\"severity\": \"error\""), "{name}: {json}");
        let run_json = run_op("run", source);
        assert!(
            run_json.contains("\"ok\": true"),
            "{name} run failed: {run_json}"
        );
    }
}

#[test]
fn empty_and_comment_only_pane_are_not_admitted() {
    for source in ["", "   \n", "# comment only\n", "// still comment only\n"] {
        let json = run_op("check", source);
        assert!(json.contains("\"ok\": true"), "{source:?}: {json}");
        assert!(json.contains("\"admitted\": false"), "{source:?}: {json}");
        assert!(json.contains("E-PKG-081"), "{source:?}: {json}");
    }
}

#[test]
fn check_bad_source_surfaces_code() {
    let json = run_op("check", "this is not emath\n");
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"admitted\": false"), "{json}");
    assert!(json.contains("\"severity\": \"error\""), "{json}");
    assert!(
        json.contains("E-SYN") || json.contains("E-NAME") || json.contains("E-"),
        "{json}"
    );
}

#[test]
fn mig_canonical_contains_goal_and_is_stable() {
    let first = run_op("mig", HELLO_SQUARE);
    let second = run_op("mig", HELLO_SQUARE);
    assert_eq!(first, second);
    assert!(first.contains("\"ok\": true"), "{first}");
    assert!(first.contains("goal"), "{first}");
}

#[test]
fn generate_hello_square_files() {
    let json = run_op("generate", HELLO_SQUARE);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"path\":"), "{json}");
    assert!(
        json.contains("struct Square") || json.contains("Square") && json.contains("fn "),
        "{json}"
    );
    assert!(
        json.contains("src/lib.rs") || json.contains("Cargo.toml"),
        "{json}"
    );
}

#[test]
fn run_finite_sum_is_fifteen() {
    let json = run_op("run", SUM_ONE_TO_FIVE);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"total\": 15.0"), "{json}");
    assert!(json.contains("\"folded\": 15.0"), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
}

#[test]
fn run_tensor_face_serializes_matrix() {
    let json = run_op("run", TENSOR_FACE);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(
        json.contains("\"face\": [[1.0, 2.0], [3.0, 4.0]]"),
        "{json}"
    );
    assert!(json.contains("\"expect_passed\": true"), "{json}");
}

#[test]
fn bare_sum_wrap_computes() {
    let json = run_op("run", "sum i in 1..6: i\n");
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"desugared_source\""), "{json}");
    assert!(
        json.contains("\"result\": 15.0"),
        "bare sum must compute 15, got {json}"
    );
    let folded = run_op("run", "sum([1, 2, 3, 4, 5])\n");
    assert!(
        folded.contains("\"result\": 15.0"),
        "bare vector sum must compute 15, got {folded}"
    );
}

#[test]
fn run_hello_square_passes() {
    let json = run_op("run", HELLO_SQUARE);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(
        json.contains("\"tier\": \"interpreted-strict-f64\""),
        "{json}"
    );
    assert!(json.contains("\"expect_passed\": true"), "{json}");
    assert!(json.contains("\"y\": 9.0"), "{json}");
    assert!(json.contains("\"passed\": 1"), "{json}");
    assert!(json.contains("\"failed\": 0"), "{json}");
}

#[test]
fn run_affine_scorer_constructor_state() {
    let source = "\
emath policy AffineScorer:
    inputs:
        x: Float64

    outputs:
        score: Float64

    state:
        scale: Float64
        bias: Float64

    constructors:
        public fn new(scale: Float64, bias: Float64) -> Result<Self, ConfigError>:
            require scale >= 0
            require is_finite(scale)
            require is_finite(bias)

            Self:
                scale = scale
                bias = bias

    definitions:
        score = state.scale * x + state.bias

    goals:
        evaluate <score>:
            produce rust.library

    tests:
        example <unit_plus_one>:
            given scale = 2
            given bias = 1
            given x = 3
            expect score == 7

    compile:
        target rust
        profile library
        numeric strict-f64
";
    let json = run_op("run", source);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
    assert!(json.contains("\"score\": 7.0"), "{json}");
    assert!(json.contains("\"scale\": 2.0"), "{json}");
    assert!(json.contains("\"bias\": 1.0"), "{json}");
}

fn worked_square_source() -> String {
    HELLO_SQUARE.replace("given x = 3\n            expect y == 9", "given x = 4")
}

fn twenty_one_source() -> &'static str {
    "\
emath function TwentyOne:
    definitions:
        y = 3 * 7

    tests:
        example <worked>:
            expect y == 21
"
}

fn head_args_square_source() -> &'static str {
    "\
emath function square(x: Float64) -> Float64:
    definitions:
        square = x * x

    tests:
        example <four>:
            given x = 4
"
}

#[test]
fn run_head_args_square_computes_sixteen() {
    let json = run_op("run", head_args_square_source());
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"computed\": true"), "{json}");
    assert!(json.contains("\"computed\": 1"), "{json}");
    assert!(
        json.contains("\"square\": 16.0"),
        "head-args square(x=4) must compute 16, got {json}"
    );
    assert!(
        !json.contains("\"expect_passed\""),
        "worked examples omit expect_passed: {json}"
    );
    assert!(json.contains("\"passed\": 0"), "{json}");
    assert!(json.contains("\"failed\": 0"), "{json}");
}

#[test]
fn generate_head_args_square_emits_free_function() {
    let json = run_op("generate", head_args_square_source());
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(
        json.contains("pub fn square") && json.contains("x: f64"),
        "stateless head-args must generate a free function: {json}"
    );
    assert!(
        !json.contains("struct square") && !json.contains("impl square"),
        "stateless head-args must not generate a unit struct + method: {json}"
    );
}

#[test]
fn run_worked_example_computes_without_expect() {
    let json = run_op("run", &worked_square_source());
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"computed\": true"), "{json}");
    assert!(json.contains("\"computed\": 1"), "{json}");
    assert!(json.contains("\"y\": 16.0"), "{json}");
    assert!(
        !json.contains("\"expect_passed\""),
        "worked examples omit expect_passed: {json}"
    );
    assert!(json.contains("\"passed\": 0"), "{json}");
    assert!(json.contains("\"failed\": 0"), "{json}");
}

#[test]
fn generate_worked_example_computes_without_assert() {
    let json = run_op("generate", &worked_square_source());
    assert!(json.contains("\"ok\": true"), "{json}");
    // Intent: a worked example (no `expect:`) must generate a test that
    // computes values but makes no pass/fail claim. Scope the check to the
    // generated test fn: the embedded `emath_rt` module may legitimately
    // contain `assert!` (e.g. Simpson's even-steps guard), which is a
    // runtime precondition, not a claim about this example.
    let at = json
        .find("fn square_three_squared")
        .expect("generated crate must contain the worked-example test fn");
    let test_tail = &json[at..];
    assert!(!test_tail.contains("assert!"), "{json}");
    assert!(test_tail.contains("let _ ="), "{json}");
    assert!(test_tail.contains("actual"), "{json}");
}

#[test]
fn run_twenty_one_constant_only() {
    let json = run_op("run", twenty_one_source());
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(
        json.contains("\"tier\": \"interpreted-strict-f64\""),
        "{json}"
    );
    assert!(json.contains("\"expect_passed\": true"), "{json}");
    assert!(json.contains("\"y\": 21.0"), "{json}");
    assert!(json.contains("\"passed\": 1"), "{json}");
    assert!(json.contains("\"failed\": 0"), "{json}");
    assert!(json.contains("\"TwentyOne\""), "{json}");
}

#[test]
fn run_failing_expect_counts_failed() {
    let source = HELLO_SQUARE.replace("y == 9", "y == 8");
    let json = run_op("run", &source);
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"expect_passed\": false"), "{json}");
    assert!(json.contains("\"failed\": 1"), "{json}");
    assert!(json.contains("\"passed\": 0"), "{json}");
}

#[test]
fn run_error_source_surfaces_diagnostics() {
    let json = run_op("run", "this is not emath\n");
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"admitted\": false"), "{json}");
    assert!(json.contains("\"severity\": \"error\""), "{json}");
    assert!(
        json.contains("E-SYN") || json.contains("E-NAME") || json.contains("E-"),
        "{json}"
    );
    assert!(!json.contains("\"tier\""), "{json}");
}

#[test]
fn unknown_op_refuses() {
    let json = run_op("not-an-op", "");
    assert!(json.contains("\"ok\": false"), "{json}");
    assert!(json.contains("unknown op `not-an-op`"), "{json}");
}

#[test]
fn json_escaping_survives_quotes_backslashes_newlines() {
    let source = "emath function \"Quote\\Path\"\n";
    let json = run_op("examples", source);
    assert!(json.contains("\"ok\": true"), "{json}");
    // The curated hello-square source contains a newline; the writer
    // must escape it rather than break the JSON object.
    assert!(json.contains("\\n"), "{json}");
    let quoted = run_op(
        "check",
        "emath function Q:\n    about:\n        summary: \"a \\\"quoted\\\" line\"\n",
    );
    assert!(
        quoted.contains("\\\"") || quoted.contains("E-") || quoted.contains("\"ok\": true"),
        "{quoted}"
    );
    let escaped = run_op("check", "line with \"quotes\" and \\back and \nnewline");
    assert!(escaped.contains("\"ok\": true"), "{escaped}");
    assert!(
        escaped.contains("\\\"") || escaped.contains("\\\\") || escaped.contains("\\n"),
        "{escaped}"
    );
}

fn run_envelope(source: &str, given: Option<&[(&str, &str)]>) -> String {
    let mut object = JsonWriter::object();
    object.string("source", source);
    if let Some(pairs) = given {
        let mut map = JsonWriter::object();
        for (name, value) in pairs {
            map.field(name, value);
        }
        object.object_field("given", &map.finish().trim_end());
    }
    run_op("run", &object.finish())
}

#[test]
fn check_bare_square_desugars_and_admits() {
    let json = run_op("check", "y = x * x\n");
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("N-TYPE-001"), "{json}");
    assert!(json.contains("\"desugared_source\""), "{json}");
    assert!(json.contains("emath function Scratch"), "{json}");
    assert!(json.contains("y = x * x"), "{json}");
    assert!(!json.contains("\"severity\": \"error\""), "{json}");
}

#[test]
fn run_bare_constants_computes_without_tests_section() {
    let json = run_op("run", "a = 2\nb = a * a\n");
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(
        json.contains("\"tier\": \"interpreted-strict-f64\""),
        "{json}"
    );
    assert!(json.contains("\"b\": 4.0"), "{json}");
    assert!(json.contains("\"computed\": true"), "{json}");
    assert!(json.contains("\"_pane\""), "{json}");
    assert!(json.contains("\"desugared_source\""), "{json}");
    assert!(
        json.contains("a = 2") && json.contains("b = a * a"),
        "{json}"
    );
    assert!(
        !json.contains("tests:\\n") && !json.contains("tests:\\n    "),
        "desugared source must not invent a tests section: {json}"
    );
}

#[test]
fn run_envelope_given_square_computes() {
    let json = run_envelope(HELLO_SQUARE, Some(&[("x", "5.0")]));
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"y\": 25.0"), "{json}");
    assert!(json.contains("\"computed\": true"), "{json}");
    assert!(json.contains("\"_pane\""), "{json}");
    assert!(json.contains("\"expect_passed\": true"), "{json}");
    assert!(json.contains("\"y\": 9.0"), "{json}");
}

#[test]
fn run_envelope_missing_binding_refuses() {
    let json = run_envelope(HELLO_SQUARE, Some(&[]));
    assert!(json.contains("\"ok\": true"), "{json}");
    assert!(json.contains("\"refusal\""), "{json}");
    assert!(json.contains("missing input `x`"), "{json}");
    assert!(json.contains("\"_pane\""), "{json}");
}

#[test]
fn run_envelope_malformed_given_number_refuses() {
    let json = run_envelope(HELLO_SQUARE, Some(&[("x", "\"abc\"")]));
    assert!(json.contains("\"ok\": false"), "{json}");
    assert!(json.contains("given `x`"), "{json}");
    let nan = run_envelope(HELLO_SQUARE, Some(&[("x", "\"NaN\"")]));
    assert!(nan.contains("\"ok\": false"), "{nan}");
    let inf = run_envelope(HELLO_SQUARE, Some(&[("x", "\"Infinity\"")]));
    assert!(inf.contains("\"ok\": false"), "{inf}");
}

#[test]
fn run_envelope_duplicate_given_key_refuses() {
    let mut object = JsonWriter::object();
    object.string("source", HELLO_SQUARE);
    object.field("given", "{\"x\": 1.0, \"x\": 2.0}");
    let json = run_op("run", &object.finish());
    assert!(json.contains("\"ok\": false"), "{json}");
    assert!(json.contains("given `x` is duplicated"), "{json}");
}

#[test]
fn run_envelope_duplicate_source_key_refuses() {
    let mut object = JsonWriter::object();
    object.string("source", HELLO_SQUARE);
    object.string("source", HELLO_SQUARE);
    let json = run_op("run", &object.finish());
    assert!(json.contains("\"ok\": false"), "{json}");
    assert!(json.contains("run envelope duplicates `source`"), "{json}");
}

fn assert_native_wasm_parity(source: &str, given: &[(&str, f64)]) {
    let mut given_map = BTreeMap::new();
    let mut given_pairs = Vec::new();
    for (k, v) in given {
        given_map.insert(k.to_string(), Value::F64(*v));
        given_pairs.push((*k, format_f64(*v)));
    }
    let prepared = prepare_source(source);
    let (mut session, file) = session_from_source(&prepared.source);
    let result = session.check(file);
    assert!(
        !result.diagnostics.has_errors(),
        "check errors: {:?}",
        result.diagnostics.items()
    );
    let native_report = run_package_with_given(&result.package, Some(&given_map));

    let given_str_refs: Vec<(&str, &str)> =
        given_pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let wasm_json = run_envelope(source, Some(&given_str_refs));
    assert!(
        wasm_json.contains("\"ok\": true"),
        "wasm failed: {wasm_json}"
    );

    let doc = parse_json_document(&wasm_json).expect("valid wasm json");
    let decls = match doc.field("declarations").expect("declarations") {
        JsonValue::Arr(list) => list,
        _ => panic!("declarations must be array"),
    };

    assert_eq!(decls.len(), native_report.declarations.len());
    for (decl_json, decl_native) in decls.iter().zip(&native_report.declarations) {
        let tests_json = match decl_json.field("tests").expect("tests") {
            JsonValue::Arr(list) => list,
            _ => panic!("tests must be array"),
        };
        assert_eq!(tests_json.len(), decl_native.tests.len());
        for (test_json, test_native) in tests_json.iter().zip(&decl_native.tests) {
            let defs_json = match test_json.field("definitions").expect("definitions") {
                JsonValue::Obj(map) => map,
                _ => panic!("definitions must be object"),
            };
            for (key, native_val) in &test_native.definitions {
                let json_val = defs_json
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v)
                    .expect("definition key present");
                match native_val {
                    Value::F64(expected) => {
                        let parsed: f64 = match json_val {
                            JsonValue::Num(num_str) => num_str.parse().expect("valid f64"),
                            JsonValue::Str(s) => s.parse().expect("valid non-finite f64 string"),
                            _ => panic!("unexpected json value for f64"),
                        };
                        if expected.is_nan() {
                            assert!(parsed.is_nan(), "expected NaN for `{key}`");
                        } else {
                            assert_eq!(
                                parsed.to_bits(),
                                expected.to_bits(),
                                "bit mismatch for `{key}`: wasm={parsed} ({:#x}) vs native={expected} ({:#x})",
                                parsed.to_bits(),
                                expected.to_bits()
                            );
                        }
                    }
                    Value::I64(expected) => {
                        let parsed: f64 = match json_val {
                            JsonValue::Num(num_str) => num_str.parse().expect("valid f64"),
                            JsonValue::Str(s) => s.parse().expect("valid non-finite f64 string"),
                            _ => panic!("unexpected json value for i64"),
                        };
                        assert!(
                            (parsed - *expected as f64).abs() < 1e-9,
                            "mismatch for `{key}`: wasm={parsed} vs native={expected}"
                        );
                    }
                    Value::Bool(expected) => {
                        let parsed = match json_val {
                            JsonValue::Bool(b) => *b,
                            _ => panic!("unexpected json value for bool"),
                        };
                        assert_eq!(parsed, *expected, "bool mismatch for `{key}`");
                    }
                    Value::Vector(expected) => {
                        let JsonValue::Arr(list) = json_val else {
                            panic!("unexpected json value for vector `{key}`");
                        };
                        assert_eq!(
                            list.len(),
                            expected.len(),
                            "vector length mismatch for `{key}`"
                        );
                        for (entry, want) in list.iter().zip(expected) {
                            let got: f64 = match entry {
                                JsonValue::Num(text) => text.parse().expect("valid f64"),
                                JsonValue::Str(text) => text.parse().expect("valid f64"),
                                _ => panic!("unexpected vector element for `{key}`"),
                            };
                            assert_eq!(
                                got.to_bits(),
                                want.to_bits(),
                                "vector mismatch for `{key}`"
                            );
                        }
                    }
                    Value::Matrix { rows, cols, data } => {
                        let JsonValue::Arr(outer) = json_val else {
                            panic!("unexpected json value for matrix `{key}`");
                        };
                        assert_eq!(outer.len(), *rows, "matrix row mismatch for `{key}`");
                        for (row_index, row) in outer.iter().enumerate() {
                            let JsonValue::Arr(cells) = row else {
                                panic!("unexpected matrix row for `{key}`");
                            };
                            assert_eq!(cells.len(), *cols, "matrix col mismatch for `{key}`");
                            for (col_index, cell) in cells.iter().enumerate() {
                                let got: f64 = match cell {
                                    JsonValue::Num(text) => text.parse().expect("valid f64"),
                                    JsonValue::Str(text) => text.parse().expect("valid f64"),
                                    _ => panic!("unexpected matrix cell for `{key}`"),
                                };
                                let want = data[row_index * cols + col_index];
                                assert_eq!(
                                    got.to_bits(),
                                    want.to_bits(),
                                    "matrix mismatch for `{key}`"
                                );
                            }
                        }
                    }
                    Value::Tensor { shape, data } => {
                        let JsonValue::Obj(map) = json_val else {
                            panic!("unexpected json value for tensor `{key}`");
                        };
                        let shape_json = map
                            .iter()
                            .find(|(name, _)| name == "shape")
                            .map(|(_, value)| value)
                            .expect("tensor shape");
                        let data_json = map
                            .iter()
                            .find(|(name, _)| name == "data")
                            .map(|(_, value)| value)
                            .expect("tensor data");
                        let JsonValue::Arr(shape_list) = shape_json else {
                            panic!("tensor shape must be an array for `{key}`");
                        };
                        let JsonValue::Arr(data_list) = data_json else {
                            panic!("tensor data must be an array for `{key}`");
                        };
                        assert_eq!(
                            shape_list.len(),
                            shape.len(),
                            "tensor rank mismatch for `{key}`"
                        );
                        assert_eq!(
                            data_list.len(),
                            data.len(),
                            "tensor data mismatch for `{key}`"
                        );
                    }
                    Value::Complex { re, im } => {
                        let JsonValue::Obj(map) = json_val else {
                            panic!("unexpected json value for complex `{key}`");
                        };
                        let got_re: f64 = match map.iter().find(|(k, _)| k == "re").map(|(_, v)| v)
                        {
                            Some(JsonValue::Num(t)) => t.parse().expect("valid f64"),
                            Some(JsonValue::Str(t)) => t.parse().expect("valid f64"),
                            _ => panic!("missing re for complex `{key}`"),
                        };
                        let got_im: f64 = match map.iter().find(|(k, _)| k == "im").map(|(_, v)| v)
                        {
                            Some(JsonValue::Num(t)) => t.parse().expect("valid f64"),
                            Some(JsonValue::Str(t)) => t.parse().expect("valid f64"),
                            _ => panic!("missing im for complex `{key}`"),
                        };
                        assert_eq!(
                            got_re.to_bits(),
                            re.to_bits(),
                            "complex re mismatch for `{key}`"
                        );
                        assert_eq!(
                            got_im.to_bits(),
                            im.to_bits(),
                            "complex im mismatch for `{key}`"
                        );
                    }
                    other => {
                        panic!(
                            "parity fixtures cover numeric/structural values only; got {other:?} for `{key}`"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn parity_transcendentals_bit_exact() {
    let source = "\
emath function Transcendentals:
    inputs:
        x: Float64

    outputs:
        s: Float64
        c: Float64
        e: Float64
        sq: Float64
        l: Float64
        t: Float64
        th: Float64
        composite: Float64

    definitions:
        s = sin(x)
        c = cos(x)
        e = exp(x)
        sq = sqrt(x)
        l = ln(x)
        t = tan(x)
        th = tanh(x)
        composite = exp(-0.1 * x) * sin(x) + sqrt(cos(x) * cos(x) + sin(x) * sin(x)) + ln(x + 1.0)
";
    for &x in &[
        0.123456789,
        0.25,
        0.5,
        1.0,
        2.0,
        std::f64::consts::PI / 3.0,
        std::f64::consts::E,
        10.0,
    ] {
        assert_native_wasm_parity(source, &[("x", x)]);
    }
}

#[test]
fn parity_polynomials_bit_exact() {
    let source = "\
emath function Polynomials:
    inputs:
        x: Float64

    outputs:
        quad: Float64
        cubic: Float64
        poly: Float64

    definitions:
        quad = 3.0 * (x ^ 2.0) + 5.0 * x - 2.0
        cubic = x ^ 3.0 - 4.0 * (x ^ 2.0) + 7.0 * x - 15.0
        poly = 2.0 * (x * x * x) - 3.0 * (x * x) + 4.0 * x - 5.0
";
    for &x in &[-10.5, -2.0, -0.5, 0.0, 1.0, 2.5, 3.5, 100.25] {
        assert_native_wasm_parity(source, &[("x", x)]);
    }
}

#[test]
fn parity_rational_functions_bit_exact() {
    let source = "\
emath function Rational:
    inputs:
        x: Float64

    outputs:
        r1: Float64
        r2: Float64

    definitions:
        r1 = (2.0 * x + 1.0) / (x * x + 4.0)
        r2 = (x ^ 3.0 - 2.0 * x + 1.0) / (x ^ 2.0 + 1.0)
";
    for &x in &[-5.0, -2.0, -1.0, 0.0, 0.5, 1.0, 2.0, 10.0] {
        assert_native_wasm_parity(source, &[("x", x)]);
    }
}

#[test]
fn parity_conditionals_bit_exact() {
    let source = "\
emath function Conditionals:
    inputs:
        x: Float64

    outputs:
        c1: Float64
        c2: Float64
        c3: Float64

    definitions:
        c1 = if x > 0.0: x * 2.0 else: -x * 3.0
        c2 = if x >= 1.0: sqrt(x) else: x * x
        c3 = if sin(x) > 0.0: cos(x) else: exp(x)
";
    for &x in &[-3.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, 4.0] {
        assert_native_wasm_parity(source, &[("x", x)]);
    }
}

#[test]
fn parity_stateful_affine_transforms_bit_exact() {
    let source = "\
emath policy AffineTransform:
    inputs:
        x: Float64

    outputs:
        y: Float64

    state:
        scale: Float64
        bias: Float64

    constructors:
        public fn new(scale: Float64, bias: Float64) -> Result<Self, ConfigError>:
            require scale >= 0.0
            require is_finite(scale)
            require is_finite(bias)

            Self:
                scale = scale
                bias = bias

    definitions:
        y = state.scale * x + state.bias
";
    let test_cases = &[
        (2.5, 1.25, 3.0),
        (0.0, -5.0, 10.0),
        (10.0, 100.0, -2.5),
        (1.0, 0.0, 42.0),
        (0.5, 0.25, -1.5),
    ];
    for &(scale, bias, x) in test_cases {
        assert_native_wasm_parity(source, &[("scale", scale), ("bias", bias), ("x", x)]);
    }
}

#[test]
fn parity_plan_and_mig_determinism_and_hashes() {
    let models = &[
        HELLO_SQUARE,
        AFFINE_SCORER,
        TUTORIAL_01_QUICKSTART,
        TUTORIAL_02_PLOTTER,
        TUTORIAL_03_MATH_INTENT,
    ];

    for &source in models {
        let initial_plan = run_op("plan", source);
        let initial_mig = run_op("mig", source);

        assert!(initial_plan.contains("\"ok\": true"), "{initial_plan}");
        assert!(initial_mig.contains("\"ok\": true"), "{initial_mig}");

        let mig_doc = parse_json_document(&initial_mig).expect("valid mig json");
        let canonical_str = mig_doc
            .string_field("canonical")
            .expect("canonical string field");
        let identity_str = mig_doc
            .string_field("identity")
            .expect("identity string field");
        assert!(!canonical_str.is_empty());
        assert!(!identity_str.is_empty());

        // Verify idempotence and exact string match across multiple runs
        for _ in 0..10 {
            let plan = run_op("plan", source);
            let mig = run_op("mig", source);
            assert_eq!(plan, initial_plan, "plan json must be deterministic");
            assert_eq!(mig, initial_mig, "mig json must be deterministic");
        }
    }
}

#[test]
fn parity_diagnostic_codes_and_structures() {
    let cases = &[
        // Syntax error (unclosed parens)
        (
            "emath function BadSyntax:\n    definitions:\n        y = (3.0 * x\n",
            "E-SYN-102",
        ),
        // Undefined variable name error
        (
            "emath function BadName:\n    inputs:\n        x: Float64\n    definitions:\n        y = nonexistent_variable\n",
            "E-TYPE-002",
        ),
        // Duplicate declaration error
        (
            "emath function Dup:\n    definitions:\n        y = 1.0\nemath function Dup:\n    definitions:\n        y = 2.0\n",
            "E-NAME-022",
        ),
        // Reserved identifier error
        (
            "emath function _:\n    definitions:\n        y = 1.0\n",
            "E-NAME-023",
        ),
        // Type error (incompatible argument to unary/binary op)
        (
            "emath function BadType:\n    inputs:\n        x: Float64\n    definitions:\n        y = sin(x > 0.0)\n",
            "E-TYPE-012",
        ),
        // Dimension/Unit compatibility error
        (
            "emath function BadUnit:\n    inputs:\n        x: Float64\n    definitions:\n        y = 1.0 m + 2.0 s\n",
            "E-UNIT-101",
        ),
        // Bare source type default note
        ("y = x * x\n", "N-TYPE-001"),
    ];

    for (source, expected_code_prefix) in cases {
        let prepared = prepare_source(source);
        let (mut session, file) = session_from_source(&prepared.source);
        let native_result = session.check(file);

        let wasm_json = run_op("check", source);
        assert!(wasm_json.contains("\"ok\": true"), "{wasm_json}");

        let wasm_doc = parse_json_document(&wasm_json).expect("valid wasm json");
        let diags = match wasm_doc.field("diagnostics").expect("diagnostics field") {
            JsonValue::Arr(list) => list,
            _ => panic!("diagnostics must be array"),
        };

        assert_eq!(
            diags.len(),
            native_result.diagnostics.items().len(),
            "diagnostic count mismatch for source: {source}"
        );

        for (wasm_diag, native_diag) in diags.iter().zip(native_result.diagnostics.items()) {
            let code = wasm_diag.string_field("code").expect("code string");
            let message = wasm_diag.string_field("message").expect("message string");
            let severity = wasm_diag.string_field("severity").expect("severity string");

            assert_eq!(code, native_diag.code);
            assert_eq!(message, native_diag.message);
            let native_sev_str = match native_diag.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Note => "note",
            };
            assert_eq!(severity, native_sev_str);
        }

        assert!(
            wasm_json.contains(expected_code_prefix),
            "expected prefix `{expected_code_prefix}` in wasm json: {wasm_json}"
        );
    }
}
