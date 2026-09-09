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
use emath_test_harness::{Probe, boot};
use emath_wasm::*;

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
fn assert_native_wasm_parity(p: &mut Probe, source: &str, given: &[(&str, f64)]) {
    let mut given_map = BTreeMap::new();
    let mut given_pairs = Vec::new();
    for (k, v) in given {
        given_map.insert(k.to_string(), Value::F64(*v));
        given_pairs.push((*k, format_f64(*v)));
    }
    let prepared = prepare_source(source);
    let (mut session, file) = session_from_source(&prepared.source);
    let result = session.check(file);
    p.demand("result.diagnostics.items()", !result.diagnostics.has_errors(), format!("check errors: {:?}", result.diagnostics.items()));
    let native_report = run_package_with_given(&result.package, Some(&given_map));

    let given_str_refs: Vec<(&str, &str)> =
        given_pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let wasm_json = run_envelope(source, Some(&given_str_refs));
    p.demand("\"wasm failed: {wasm_json}\"", wasm_json.contains("\"ok\": true"), format!("wasm failed: {wasm_json}"));

    let doc = parse_json_document(&wasm_json).expect("valid wasm json");
    let decls = match doc.field("declarations").expect("declarations") {
        JsonValue::Arr(list) => list,
        _ => panic!("declarations must be array"),
    };

    p.eq("decls.len()", &(decls.len()), &(native_report.declarations.len()));
    for (decl_json, decl_native) in decls.iter().zip(&native_report.declarations) {
        let tests_json = match decl_json.field("tests").expect("tests") {
            JsonValue::Arr(list) => list,
            _ => panic!("tests must be array"),
        };
        p.eq("tests_json.len()", &(tests_json.len()), &(decl_native.tests.len()));
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
                            p.demand("\"expected NaN for `{key}`\"", parsed.is_nan(), format!("expected NaN for `{key}`"));
                        } else {
                            p.eq("parsed.to_bits()", &(parsed.to_bits()), &(expected.to_bits()));
                        }
                    }
                    Value::I64(expected) => {
                        let parsed: f64 = match json_val {
                            JsonValue::Num(num_str) => num_str.parse().expect("valid f64"),
                            JsonValue::Str(s) => s.parse().expect("valid non-finite f64 string"),
                            _ => panic!("unexpected json value for i64"),
                        };
                        p.demand("\"mismatch for `{key}`: wasm={parsed} vs native={expected}\"", (parsed - *expected as f64).abs() < 1e-9, format!("mismatch for `{key}`: wasm={parsed} vs native={expected}"));
                    }
                    Value::Bool(expected) => {
                        let parsed = match json_val {
                            JsonValue::Bool(b) => *b,
                            _ => panic!("unexpected json value for bool"),
                        };
                        p.eq("parsed", &(parsed), &(*expected));
                    }
                    Value::Vector(expected) => {
                        let JsonValue::Arr(list) = json_val else {
                            panic!("unexpected json value for vector `{key}`");
                        };
                        p.eq("list.len()", &(list.len()), &(expected.len()));
                        for (entry, want) in list.iter().zip(expected) {
                            let got: f64 = match entry {
                                JsonValue::Num(text) => text.parse().expect("valid f64"),
                                JsonValue::Str(text) => text.parse().expect("valid f64"),
                                _ => panic!("unexpected vector element for `{key}`"),
                            };
                            p.eq("got.to_bits()", &(got.to_bits()), &(want.to_bits()));
                        }
                    }
                    Value::Matrix { rows, cols, data } => {
                        let JsonValue::Arr(outer) = json_val else {
                            panic!("unexpected json value for matrix `{key}`");
                        };
                        p.eq("outer.len()", &(outer.len()), &(*rows));
                        for (row_index, row) in outer.iter().enumerate() {
                            let JsonValue::Arr(cells) = row else {
                                panic!("unexpected matrix row for `{key}`");
                            };
                            p.eq("cells.len()", &(cells.len()), &(*cols));
                            for (col_index, cell) in cells.iter().enumerate() {
                                let got: f64 = match cell {
                                    JsonValue::Num(text) => text.parse().expect("valid f64"),
                                    JsonValue::Str(text) => text.parse().expect("valid f64"),
                                    _ => panic!("unexpected matrix cell for `{key}`"),
                                };
                                let want = data[row_index * cols + col_index];
                                p.eq("got.to_bits()", &(got.to_bits()), &(want.to_bits()));
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
                        p.eq("shape_list.len()", &(shape_list.len()), &(shape.len()));
                        p.eq("data_list.len()", &(data_list.len()), &(data.len()));
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
                        p.eq("got_re.to_bits()", &(got_re.to_bits()), &(re.to_bits()));
                        p.eq("got_im.to_bits()", &(got_im.to_bits()), &(im.to_bits()));
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
fn wasm_ops() {
    boot();
    let mut p = Probe::new("wasm ops admit, compute, refuse, and match native bit-exact");
    p.case("version", |p| {
        let json = run_op("version", "");
        p.contains("ok", &json, "\"ok\": true");
        p.contains("version", &json, env!("CARGO_PKG_VERSION"));
        p.contains("abi", &json, "\"abi\": 1");
        p.ne("nonempty", json, String::new());
    });
    let runs: &[(&str, &str, &[&str])] = &[
        ("hello-check", HELLO_SQUARE, &["\"admitted\": true", "\"diagnostics\": []", "\"Square\""]),
        ("vector", VECTOR_GIVEN, &["\"first\": 1.0", "\"mag_sq\": 14.0", "\"scaled\": [2.0, 4.0, 6.0]", "\"expect_passed\": true"]),
        ("factorial", FACTORIAL, &["\"fac\": 120.0", "\"expect_passed\": true"]),
        ("range-sum", RANGE_SUM, &["\"s\": 6.0", "\"expect_passed\": true"]),
        ("forall", FORALL_EXISTS, &["\"all_positive\": false", "\"has_zero\": true", "\"expect_passed\": true"]),
        ("integral", INTEGRAL, &["\"area\":", "\"expect_passed\": true"]),
        ("autodiff", AUTODIFF, &["\"dy\": 6.0", "\"expect_passed\": true"]),
        ("solve", SOLVE, &["\"root\":", "\"expect_passed\": true"]),
        ("constrained", CONSTRAINED_OPT, &["\"opt_x\":", "\"expect_passed\": true"]),
        ("optimize", OPTIMIZE, &["\"min_x\":", "\"max_x\":", "\"expect_passed\": true"]),
        ("sum-five", SUM_ONE_TO_FIVE, &["\"total\": 15.0", "\"folded\": 15.0", "\"expect_passed\": true"]),
        ("tensor", TENSOR_FACE, &["\"face\": [[1.0, 2.0], [3.0, 4.0]]", "\"expect_passed\": true"]),
        ("hello-run", HELLO_SQUARE, &["\"tier\": \"interpreted-strict-f64\"", "\"y\": 9.0", "\"passed\": 1", "\"failed\": 0", "\"expect_passed\": true"]),
        ("twenty-one", "emath function TwentyOne:\n    definitions:\n        y = 3 * 7\n\n    tests:\n        example <worked>:\n            expect y == 21\n", &["\"tier\": \"interpreted-strict-f64\"", "\"y\": 21.0", "\"passed\": 1", "\"TwentyOne\""]),
    ];
    for &(name, source, needles) in runs {
        p.case(name, |p| {
            let op = if *name == "hello-check" { "check" } else { "run" };
            let json = run_op(op, source);
            p.contains(format!("{name}/ok"), &json, "\"ok\": true");
            for &needle in needles {
                p.contains(format!("{name}/{needle}"), &json, needle);
            }
            p.ne(format!("{name}/nonempty"), json, String::new());
        });
    }
    p.case("bare-sums", |p| {
        let json = run_op("run", "sum i in 1..6: i\n");
        p.contains("ok", &json, "\"ok\": true");
        p.contains("desugar", &json, "\"desugared_source\"");
        p.contains("result", &json, "\"result\": 15.0");
        p.contains("vec-sum", &run_op("run", "sum([1, 2, 3, 4, 5])\n"), "\"result\": 15.0");
    });
    p.case("affine", |p| {
        let source = "emath policy AffineScorer:\n    inputs:\n        x: Float64\n\n    outputs:\n        score: Float64\n\n    state:\n        scale: Float64\n        bias: Float64\n\n    constructors:\n        public fn new(scale: Float64, bias: Float64) -> Result<Self, ConfigError>:\n            require scale >= 0\n            require is_finite(scale)\n            require is_finite(bias)\n\n            Self:\n                scale = scale\n                bias = bias\n\n    definitions:\n        score = state.scale * x + state.bias\n\n    goals:\n        evaluate <score>:\n            produce rust.library\n\n    tests:\n        example <unit_plus_one>:\n            given scale = 2\n            given bias = 1\n            given x = 3\n            expect score == 7\n\n    compile:\n        target rust\n        profile library\n        numeric strict-f64\n";
        let json = run_op("run", source);
        p.contains("ok", &json, "\"ok\": true");
        p.contains("score", &json, "\"score\": 7.0");
        p.contains("scale", &json, "\"scale\": 2.0");
        p.contains("bias", &json, "\"bias\": 1.0");
    });
    p.case("head-args", |p| {
        let source = "emath function square(x: Float64) -> Float64:\n    definitions:\n        square = x * x\n\n    tests:\n        example <four>:\n            given x = 4\n";
        let json = run_op("run", source);
        p.contains("ok", &json, "\"ok\": true");
        p.contains("computed", &json, "\"computed\": true");
        p.contains("square", &json, "\"square\": 16.0");
        p.demand("no-expect", !json.contains("\"expect_passed\""), "worked omits expect_passed");
        let gen = run_op("generate", source);
        p.contains("free-fn", &gen, "pub fn square");
        p.demand("no-struct", !gen.contains("struct square") && !gen.contains("impl square"), "stateless stays free");
    });
    p.case("worked", |p| {
        let source = HELLO_SQUARE.replace("given x = 3\n            expect y == 9", "given x = 4");
        let json = run_op("run", &source);
        p.contains("ok", &json, "\"ok\": true");
        p.contains("y", &json, "\"y\": 16.0");
        p.demand("no-expect", !json.contains("\"expect_passed\""), "worked omits expect_passed");
        let gen = run_op("generate", &source);
        let at = gen.find("fn square_three_squared").expect("worked test fn");
        let tail = &gen[at..];
        p.demand("no-assert", !tail.contains("assert!"), "worked generates no claim");
        p.contains("bind", tail, "let _ =");
    });
    p.case("expect-counts", |p| {
        let json = run_op("run", &HELLO_SQUARE.replace("y == 9", "y == 8"));
        p.contains("ok", &json, "\"ok\": true");
        p.contains("failed", &json, "\"expect_passed\": false");
        p.contains("count", &json, "\"failed\": 1");
    });
    p.case("refusals", |p| {
        for source in ["", "   \n", "# comment only\n", "// still comment only\n"] {
            let json = run_op("check", source);
            p.contains(format!("empty/{source:?}/admitted"), &json, "\"admitted\": false");
            p.contains(format!("empty/{source:?}/code"), &json, "E-PKG-081");
        }
        for (name, source) in [("check-bad", "this is not emath\n"), ("run-bad", "this is not emath\n")] {
            let json = run_op(if name == "check-bad" { "check" } else { "run" }, source);
            p.contains(format!("{name}/ok"), &json, "\"ok\": true");
            p.contains(format!("{name}/admitted"), &json, "\"admitted\": false");
            p.contains(format!("{name}/severity"), &json, "\"severity\": \"error\"");
            p.contains(format!("{name}/code"), &json, "E-");
        }
        p.demand("run-no-tier", !run_op("run", "this is not emath\n").contains("\"tier\""), "refused run has no tier");
        let unknown = run_op("not-an-op", "");
        p.contains("unknown-ok", &unknown, "\"ok\": false");
        p.contains("unknown-op", &unknown, "unknown op `not-an-op`");
    });
    p.case("mig-generate", |p| {
        let (first, second) = (run_op("mig", HELLO_SQUARE), run_op("mig", HELLO_SQUARE));
        p.eq("stable", first.clone(), second);
        p.contains("goal", &first, "goal");
        let gen = run_op("generate", HELLO_SQUARE);
        p.contains("path", &gen, "\"path\":");
        p.contains("square", &gen, "Square");
        p.demand("files", gen.contains("src/lib.rs") || gen.contains("Cargo.toml"), "generated files");
    });
    p.case("curated", |p| {
        for (name, source) in curated_examples() {
            if name.contains("Diagnostics") || *name == "diagnostics demo" {
                continue;
            }
            let json = run_op("check", source);
            p.contains(format!("{name}/admitted"), &json, "\"admitted\": true");
            p.demand(format!("{name}/no-error"), !json.contains("\"severity\": \"error\""), "no error diagnostics");
            p.contains(format!("{name}/run"), &run_op("run", source), "\"ok\": true");
        }
    });
    p.case("escaping", |p| {
        p.contains("newline", &run_op("examples", "emath function \"Quote\\Path\"\n"), "\\n");
        p.contains("bare-square", &run_op("check", "y = x * x\n"), "N-TYPE-001");
        p.contains("desugar", &run_op("check", "y = x * x\n"), "\"desugared_source\"");
        let bare = run_op("run", "a = 2\nb = a * a\n");
        p.contains("pane", &bare, "\"_pane\"");
        p.contains("computed", &bare, "\"b\": 4.0");
    });
    p.case("envelope", |p| {
        let source = "\nemath function VecPane:\n    inputs:\n        v: Vector[3]\n\n    outputs:\n        first: Float64\n        mag_sq: Float64\n\n    definitions:\n        first = v[0]\n        mag_sq = dot(v, v)\n";
        let json = run_envelope(source, Some(&[("v", "[1.0, 2.0, 3.0]")]));
        p.contains("ok", &json, "\"ok\": true");
        p.contains("first", &json, "\"first\": 1.0");
        let given = run_envelope(HELLO_SQUARE, Some(&[("x", "5.0")]));
        p.contains("given-y", &given, "\"y\": 25.0");
        p.contains("pane", &given, "\"_pane\"");
        let missing = run_envelope(HELLO_SQUARE, Some(&[]));
        p.contains("missing", &missing, "missing input `x`");
        let malformed = run_envelope(HELLO_SQUARE, Some(&[("x", "\"abc\"")]));
        p.contains("malformed", &malformed, "\"ok\": false");
        p.contains("nan", &run_envelope(HELLO_SQUARE, Some(&[("x", "\"NaN\"")])), "\"ok\": false");
        let mut dup = JsonWriter::object();
        dup.string("source", HELLO_SQUARE);
        dup.field("given", "{\"x\": 1.0, \"x\": 2.0}");
        p.contains("dup-given", &run_op("run", &dup.finish()), "given `x` is duplicated");
        let mut dup_src = JsonWriter::object();
        dup_src.string("source", HELLO_SQUARE);
        dup_src.string("source", HELLO_SQUARE);
        p.contains("dup-src", &run_op("run", &dup_src.finish()), "run envelope duplicates `source`");
    });
    p.case("parity-families", |p| {
        let transcendental = "emath function Transcendentals:\n    inputs:\n        x: Float64\n\n    outputs:\n        s: Float64\n        c: Float64\n        e: Float64\n        sq: Float64\n        l: Float64\n        t: Float64\n        th: Float64\n        composite: Float64\n\n    definitions:\n        s = sin(x)\n        c = cos(x)\n        e = exp(x)\n        sq = sqrt(x)\n        l = ln(x)\n        t = tan(x)\n        th = tanh(x)\n        composite = exp(-0.1 * x) * sin(x) + sqrt(cos(x) * cos(x) + sin(x) * sin(x)) + ln(x + 1.0)\n";
        for x in [0.123456789, 0.25, 0.5, 1.0, 2.0, std::f64::consts::PI / 3.0, std::f64::consts::E, 10.0] {
            assert_native_wasm_parity(p, transcendental, &[("x", x)]);
        }
        let polynomial = "emath function Polynomials:\n    inputs:\n        x: Float64\n\n    outputs:\n        quad: Float64\n        cubic: Float64\n        poly: Float64\n\n    definitions:\n        quad = 3.0 * (x ^ 2.0) + 5.0 * x - 2.0\n        cubic = x ^ 3.0 - 4.0 * (x ^ 2.0) + 7.0 * x - 15.0\n        poly = 2.0 * (x * x * x) - 3.0 * (x * x) + 4.0 * x - 5.0\n";
        for x in [-10.5, -2.0, -0.5, 0.0, 1.0, 2.5, 3.5, 100.25] {
            assert_native_wasm_parity(p, polynomial, &[("x", x)]);
        }
        let rational = "emath function Rational:\n    inputs:\n        x: Float64\n\n    outputs:\n        r1: Float64\n        r2: Float64\n\n    definitions:\n        r1 = (2.0 * x + 1.0) / (x * x + 4.0)\n        r2 = (x ^ 3.0 - 2.0 * x + 1.0) / (x ^ 2.0 + 1.0)\n";
        for x in [-5.0, -2.0, -1.0, 0.0, 0.5, 1.0, 2.0, 10.0] {
            assert_native_wasm_parity(p, rational, &[("x", x)]);
        }
        let conditional = "emath function Conditionals:\n    inputs:\n        x: Float64\n\n    outputs:\n        c1: Float64\n        c2: Float64\n        c3: Float64\n\n    definitions:\n        c1 = if x > 0.0: x * 2.0 else: -x * 3.0\n        c2 = if x >= 1.0: sqrt(x) else: x * x\n        c3 = if sin(x) > 0.0: cos(x) else: exp(x)\n";
        for x in [-3.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, 4.0] {
            assert_native_wasm_parity(p, conditional, &[("x", x)]);
        }
        p.ne("parity-nonempty", run_op("run", VECTOR_GIVEN), String::new());
    });
    p.case("plan-mig-diag", |p| {
        for source in [HELLO_SQUARE, AFFINE_SCORER, TUTORIAL_01_QUICKSTART, TUTORIAL_02_PLOTTER, TUTORIAL_03_MATH_INTENT] {
            let (plan, mig) = (run_op("plan", source), run_op("mig", source));
            p.contains("plan-ok", &plan, "\"ok\": true");
            let doc = parse_json_document(&mig).unwrap();
            p.demand("canonical", !doc.string_field("canonical").unwrap().is_empty(), "canonical present");
            p.demand("identity", !doc.string_field("identity").unwrap().is_empty(), "identity present");
            p.eq("plan-stable", run_op("plan", source), plan);
            p.eq("mig-stable", run_op("mig", source), mig);
        }
        for (source, prefix) in [
            ("emath function BadSyntax:\n    definitions:\n        y = (3.0 * x\n", "E-SYN-102"),
            ("emath function BadName:\n    inputs:\n        x: Float64\n    definitions:\n        y = nonexistent_variable\n", "E-TYPE-002"),
            ("emath function Dup:\n    definitions:\n        y = 1.0\nemath function Dup:\n    definitions:\n        y = 2.0\n", "E-NAME-022"),
            ("emath function _:\n    definitions:\n        y = 1.0\n", "E-NAME-023"),
            ("emath function BadType:\n    inputs:\n        x: Float64\n    definitions:\n        y = sin(x > 0.0)\n", "E-TYPE-012"),
            ("emath function BadUnit:\n    inputs:\n        x: Float64\n    definitions:\n        y = 1.0 m + 2.0 s\n", "E-UNIT-101"),
            ("y = x * x\n", "N-TYPE-001"),
        ] {
            let prepared = prepare_source(source);
            let (mut session, file) = session_from_source(&prepared.source);
            let native = session.check(file);
            let wasm_json = run_op("check", source);
            p.contains(format!("diag/{prefix}"), &wasm_json, prefix);
            let doc = parse_json_document(&wasm_json).unwrap();
            let diags = match doc.field("diagnostics").unwrap() {
                JsonValue::Arr(list) => list,
                _ => panic!("diagnostics array"),
            };
            p.eq(format!("count/{prefix}"), diags.len(), native.diagnostics.items().len());
            for (w, n) in diags.iter().zip(native.diagnostics.items()) {
                p.eq(format!("code/{prefix}"), w.string_field("code").unwrap(), n.code.clone());
                let sev = match n.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Note => "note",
                };
                p.eq(format!("sev/{prefix}"), w.string_field("severity").unwrap(), sev.to_string());
            }
        }
    });
    p.finish();
}
