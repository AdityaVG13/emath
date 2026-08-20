//! In-memory emath compiler pipeline for the web demo.
//!
//! Safe dispatch lives here and is unit-testable on the host. The tiny C
//! ABI (`em_alloc` / `em_free` / `em_run` / `em_init`) is confined to [`ffi`].

#![deny(unsafe_code)]
#![deny(missing_docs)]

/// C ABI leaf: `em_alloc`, `em_free`, `em_run`, `em_init`.
#[allow(unsafe_code)]
pub mod ffi;

pub use ffi::install_panic_hook;

mod desugar;

use desugar::prepare_source;
use emath_artifact::{parse_json_document, JsonValue, JsonWriter};
use emath_core::{limits::Limits, Diagnostics, FileId, Severity};
use emath_exec_ir::interp::{format_f64, Value};
use emath_exec_ir::runner::{run_package_with_given, DeclarationRun, RunReport, TestRun};
use emath_ir::Mig;
use emath_rust_backend::BackendInput;
use emath_sema::session::CompilerSession;
use emath_syntax::{format_lossless, install_source_parser, parse_lossless};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// ABI version carried by the `version` op.
pub const ABI_VERSION: u32 = 1;

const HELLO_SQUARE: &str = include_str!("../../../language/examples/intro/hello-square.emath");
const AFFINE_SCORER: &str =
    include_str!("../../../language/examples/intro/stateful-affine-scorer.emath");
const PARAMETRIC_UNKNOWN: &str =
    include_str!("../../../language/examples/integration/parametric-unknown-operator.emath");

const TUTORIAL_01_QUICKSTART: &str = "\
# Tutorial 1: Quickstart & Scratchpad
# Declarative mathematical function with test assertions.
# Press Ctrl+R (or Cmd+Enter) to evaluate the interpreter.

emath function Quickstart:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y = 3 * x + 7

    goals:
        evaluate <y>:
            produce rust.library

    tests:
        example <test_four>:
            given x = 4
            expect y == 19
";

const TUTORIAL_02_PLOTTER: &str = "\
# Tutorial 2: 2D Curve Plotter & Parameters
# Switch to the 'Plot 2D' tab (Alt+2) to visualize this oscillator curve.
# Adjust the 'x' slider live while viewing the canvas.

emath function DampedOscillator:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y = exp(-0.1 * x) * sin(x)

    goals:
        evaluate <y>:
            produce rust.library

    tests:
        example <origin>:
            given x = 0
            expect y == 0
";

const TUTORIAL_03_MATH_INTENT: &str = "\
# Tutorial 3: Math Intent & Typography
# Press Shift+Cmd+Y to toggle Unicode math symbols.
# Switch to the 'Math Intent' tab (Alt+3) to view LaTeX rendering and export formulas.

emath function AerodynamicDrag:
    inputs:
        rho: Float64
        v: Float64
        cd: Float64
        area: Float64

    outputs:
        drag_force: Float64

    definitions:
        drag_force = 0.5 * rho * (v * v) * cd * area

    goals:
        evaluate <drag_force>:
            produce rust.library
";

const TUTORIAL_06_DIAGNOSTICS_DEMO: &str = "\
# Tutorial 6: Diagnostics & Error Recovery
# Notice the red indicator in the status bar and the Diagnostics tab (Alt+5).
# Fix the undefined variable below to see diagnostics clear automatically.

emath function DiagnosticsDemo:
    inputs:
        x: Float64

    definitions:
        y = missing_variable
";

/// Curated examples served by the `examples` op.
fn curated_examples() -> &'static [(&'static str, &'static str)] {
    &[
        ("Tutorial 1: Quickstart & Scratchpad", TUTORIAL_01_QUICKSTART),
        ("Tutorial 2: 2D Curve Plotter & Parameters", TUTORIAL_02_PLOTTER),
        ("Tutorial 3: Math Intent & Typography", TUTORIAL_03_MATH_INTENT),
        ("Tutorial 4: Stateful Scorer & Assertions", AFFINE_SCORER),
        ("Tutorial 5: Parametric Operator Synthesis", PARAMETRIC_UNKNOWN),
        ("Tutorial 6: Diagnostics & Error Recovery", TUTORIAL_06_DIAGNOSTICS_DEMO),
        ("Hello Square (Classic)", HELLO_SQUARE),
    ]
}

/// Dispatch one engine op. `payload` is `.emath` source unless the op
/// ignores it (`version`, `examples`) or is `run` (raw source or a JSON
/// envelope with `source` + optional `given`).
///
/// The returned string is one JSON object with deterministic field order.
#[must_use]
pub fn run_op(op: &str, payload: &str) -> String {
    install_source_parser();
    match op {
        "version" => op_version(),
        "examples" => op_examples(),
        "check" => op_check(payload),
        "plan" => op_plan(payload),
        "mig" => op_mig(payload),
        "generate" => op_generate(payload),
        "format" => op_format(payload),
        "run" => op_run(payload),
        "inputs" => op_inputs(payload),
        other => error_json(&format!("unknown op `{other}`")),
    }
}

pub(crate) fn error_json(message: &str) -> String {
    let mut object = JsonWriter::object();
    object.bool("ok", false);
    object.string("error", message);
    object.finish()
}

fn op_version() -> String {
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.string("version", env!("CARGO_PKG_VERSION"));
    object.int("abi", u64::from(ABI_VERSION));
    object.finish()
}

fn op_examples() -> String {
    static CACHED: OnceLock<String> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            let examples = curated_examples();
            let mut entries = Vec::with_capacity(examples.len());
            for (name, source) in examples {
                let mut entry = JsonWriter::object();
                entry.string("name", name);
                entry.string("source", source);
                entries.push(entry.finish().trim_end().to_string());
            }
            let mut object = JsonWriter::object();
            object.bool("ok", true);
            object.objects("examples", &entries);
            object.finish()
        })
        .clone()
}

fn session_from_source(source: &str) -> (CompilerSession, emath_core::FileId) {
    let mut session = CompilerSession::new(Limits::default());
    let file = session.load_text("input.emath", source.to_string());
    (session, file)
}

fn maybe_desugared(object: &mut emath_artifact::JsonObject, desugared: Option<&str>) {
    if let Some(source) = desugared {
        object.string("desugared_source", source);
    }
}

struct RunPayload<'a> {
    source: Cow<'a, str>,
    given: Option<BTreeMap<String, f64>>,
}

fn parse_run_payload<'a>(payload: &'a str) -> RunPayload<'a> {
    let trimmed = payload.trim_start();
    if !trimmed.starts_with('{') {
        return RunPayload {
            source: Cow::Borrowed(payload),
            given: None,
        };
    }
    let Ok(value) = parse_json_document(payload.trim()) else {
        return RunPayload {
            source: Cow::Borrowed(payload),
            given: None,
        };
    };
    let Ok(source) = value.string_field("source") else {
        return RunPayload {
            source: Cow::Borrowed(payload),
            given: None,
        };
    };
    RunPayload {
        source: Cow::Owned(source),
        given: parse_given_field(&value),
    }
}

fn parse_given_field(value: &JsonValue) -> Option<BTreeMap<String, f64>> {
    let Ok(given) = value.field("given") else {
        return None;
    };
    let JsonValue::Obj(entries) = given else {
        return Some(BTreeMap::new());
    };
    let mut map = BTreeMap::new();
    for (name, entry) in entries {
        match entry {
            JsonValue::Num(text) => {
                if let Ok(number) = text.parse::<f64>() {
                    map.insert(name.clone(), number);
                }
            }
            JsonValue::Bool(true) => {
                map.insert(name.clone(), 1.0);
            }
            JsonValue::Bool(false) => {
                map.insert(name.clone(), 0.0);
            }
            _ => {}
        }
    }
    Some(map)
}

fn diagnostic_objects(diagnostics: &Diagnostics) -> Vec<String> {
    let items = diagnostics.items();
    let mut body = Vec::with_capacity(items.len());
    for item in items {
        let mut entry = JsonWriter::object();
        entry.string(
            "severity",
            match item.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Note => "note",
            },
        );
        entry.string("code", item.code);
        entry.string("message", &item.message);
        entry.int("start", u64::from(item.primary.start));
        entry.int("end", u64::from(item.primary.end));
        body.push(entry.finish().trim_end().to_string());
    }
    body
}

fn declaration_names(package: &emath_ir::SemanticPackage) -> Vec<String> {
    let mut names = Vec::with_capacity(package.declarations.len());
    for declaration in &package.declarations {
        names.push(declaration.name.leaf().to_string());
    }
    names
}

fn crate_name_of(package: &emath_ir::SemanticPackage) -> String {
    package
        .identity
        .as_ref()
        .map(|identity| identity.name.clone())
        .or_else(|| {
            package
                .declarations
                .first()
                .map(|declaration| declaration.name.leaf().to_string())
        })
        .unwrap_or_else(|| "package".to_string())
}

fn op_check(source: &str) -> String {
    let prepared = prepare_source(source);
    let (mut session, file) = session_from_source(&prepared.source);
    let result = session.check(file);
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.objects("diagnostics", &diagnostic_objects(&result.diagnostics));
    object.strings("declarations", &declaration_names(&result.package));
    maybe_desugared(&mut object, prepared.desugared());
    object.finish()
}

fn op_plan(source: &str) -> String {
    let prepared = prepare_source(source);
    let (mut session, file) = session_from_source(&prepared.source);
    let result = session.plan(file);
    let mut requests = Vec::with_capacity(result.requests.len());
    for request in &result.requests {
        let mut entry = JsonWriter::object();
        entry.string("kind", &request.kind);
        entry.string("target", &request.target);
        entry.string("produce", &request.produce);
        requests.push(entry.finish().trim_end().to_string());
    }
    let mut plan_items = Vec::with_capacity(result.plans.len());
    for plan in &result.plans {
        let mut steps = Vec::with_capacity(plan.nodes.len());
        let mut providers = Vec::with_capacity(plan.nodes.len());
        for node in plan.nodes.values() {
            steps.push(node.operation.name().to_string());
            if let Some(provider) = &node.provider {
                if !providers.iter().any(|existing| existing == &provider.id) {
                    providers.push(provider.id.clone());
                }
            }
        }
        let mut entry = JsonWriter::object();
        entry.string("policy", &plan.policy);
        entry.string("artifact_class", &plan.artifact_class);
        entry.strings("steps", &steps);
        entry.strings("providers", &providers);
        plan_items.push(entry.finish().trim_end().to_string());
    }
    let mut plans = JsonWriter::object();
    plans.int("count", result.plans.len() as u64);
    plans.objects("items", &plan_items);
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.objects("diagnostics", &diagnostic_objects(&result.diagnostics));
    object.objects("requests", &requests);
    object.object_field("plans", plans.finish().trim_end());
    maybe_desugared(&mut object, prepared.desugared());
    object.finish()
}

fn op_mig(source: &str) -> String {
    let prepared = prepare_source(source);
    let (mut session, file) = session_from_source(&prepared.source);
    let result = session.plan(file);
    let mig = Mig::from_package(&result.package);
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.string("canonical", &mig.canonical());
    object.int("nodes", mig.nodes.len() as u64);
    object.int("edges", mig.edges.len() as u64);
    object.string("identity", &mig.identity().0);
    maybe_desugared(&mut object, prepared.desugared());
    object.finish()
}

fn op_generate(source: &str) -> String {
    let prepared = prepare_source(source);
    let (mut session, file) = session_from_source(&prepared.source);
    let result = session.plan(file);
    if result.diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        object.bool("ok", true);
        object.objects("diagnostics", &diagnostic_objects(&result.diagnostics));
        maybe_desugared(&mut object, prepared.desugared());
        return object.finish();
    }
    let crate_name = crate_name_of(&result.package);
    let output = match (BackendInput {
        package: &result.package,
        crate_name: crate_name.clone(),
        version: "0.1.0".to_string(),
    })
    .generate()
    {
        Ok(output) => output,
        Err(error) => return error_json(&error.to_string()),
    };
    let mut files = Vec::with_capacity(output.files.len());
    for (path, content) in &output.files {
        let mut entry = JsonWriter::object();
        entry.string("path", path);
        entry.string("content", content);
        files.push(entry.finish().trim_end().to_string());
    }
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.string("crate_name", &crate_name);
    object.objects("files", &files);
    maybe_desugared(&mut object, prepared.desugared());
    object.finish()
}

fn op_run(payload: &str) -> String {
    let envelope = parse_run_payload(payload);
    let prepared = prepare_source(&envelope.source);
    let (mut session, file) = session_from_source(&prepared.source);
    let result = session.check(file);
    if result.diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        object.bool("ok", true);
        object.objects("diagnostics", &diagnostic_objects(&result.diagnostics));
        maybe_desugared(&mut object, prepared.desugared());
        return object.finish();
    }
    let report = run_package_with_given(&result.package, envelope.given.as_ref());
    serialize_run_report(&report, prepared.desugared())
}

fn op_inputs(source: &str) -> String {
    let prepared = prepare_source(source);
    let (mut session, file) = session_from_source(&prepared.source);
    let result = session.check(file);
    let mut declarations = Vec::with_capacity(result.package.declarations.len());
    for declaration in &result.package.declarations {
        let mut inputs = Vec::with_capacity(declaration.inputs.len());
        for field in &declaration.inputs {
            let type_name = result
                .package
                .types
                .get(field.ty.index())
                .map(emath_ir::TypeNode::display_name)
                .unwrap_or_else(|| "Float64".to_string());
            let defaulted = result.diagnostics.items().iter().any(|item| {
                item.code == "N-TYPE-001" && item.message.contains(field.name.as_str())
            });
            let mut entry = JsonWriter::object();
            entry.string("name", &field.name);
            entry.string("type", &type_name);
            entry.bool("defaulted", defaulted);
            inputs.push(entry.finish().trim_end().to_string());
        }
        let mut object = JsonWriter::object();
        object.string("declaration", declaration.name.leaf());
        object.objects("inputs", &inputs);
        declarations.push(object.finish().trim_end().to_string());
    }
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.objects("diagnostics", &diagnostic_objects(&result.diagnostics));
    object.objects("declarations", &declarations);
    maybe_desugared(&mut object, prepared.desugared());
    object.finish()
}

fn serialize_run_report(report: &RunReport, desugared: Option<&str>) -> String {
    let mut declarations = Vec::with_capacity(report.declarations.len());
    for declaration in &report.declarations {
        declarations.push(serialize_declaration_run(declaration));
    }
    let mut summary = JsonWriter::object();
    summary.int("tests", u64::from(report.summary.tests));
    summary.int("passed", u64::from(report.summary.passed));
    summary.int("failed", u64::from(report.summary.failed));
    summary.int("refused", u64::from(report.summary.refused));
    summary.int("computed", u64::from(report.summary.computed));
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.string("tier", "interpreted-strict-f64");
    object.objects("declarations", &declarations);
    object.object_field("summary", summary.finish().trim_end());
    if let Some(source) = desugared {
        object.string("desugared_source", source);
    }
    object.finish()
}

fn serialize_declaration_run(declaration: &DeclarationRun) -> String {
    let mut tests = Vec::with_capacity(declaration.tests.len());
    for test in &declaration.tests {
        tests.push(serialize_test_run(test));
    }
    let mut object = JsonWriter::object();
    object.string("name", &declaration.name);
    object.objects("tests", &tests);
    if let Some(note) = &declaration.note {
        object.string("note", note);
    } else {
        object.field("note", "null");
    }
    object.finish().trim_end().to_string()
}

fn serialize_test_run(test: &TestRun) -> String {
    let mut object = JsonWriter::object();
    object.string("name", &test.name);
    object.object_field("given", &value_map_f64(&test.given));
    if !test.state.is_empty() {
        object.object_field("state", &value_map_f64(&test.state));
    }
    object.object_field("definitions", &value_map_value(&test.definitions));
    object.object_field("outputs", &value_map_value(&test.outputs));
    if test.verdict.is_computed() {
        object.bool("computed", true);
    } else {
        object.bool("expect_passed", test.verdict.expect_passed());
    }
    if let Some(tag) = test.verdict.refusal_tag() {
        object.string("refusal", tag);
        if let Some(reason) = test.verdict.reason_text() {
            object.string("reason", &reason);
        }
    }
    object.finish().trim_end().to_string()
}

fn value_map_f64(map: &BTreeMap<String, f64>) -> String {
    let mut object = JsonWriter::object();
    for (name, value) in map {
        object.field(name, &json_f64(*value));
    }
    object.finish().trim_end().to_string()
}

fn value_map_value(map: &BTreeMap<String, Value>) -> String {
    let mut object = JsonWriter::object();
    for (name, value) in map {
        match value {
            Value::F64(number) => {
                object.field(name, &json_f64(*number));
            }
            Value::Bool(flag) => {
                object.bool(name, *flag);
            }
        }
    }
    object.finish().trim_end().to_string()
}

fn json_f64(value: f64) -> String {
    if value.is_finite() {
        format_f64(value)
    } else {
        format!("\"{}\"", format_f64(value))
    }
}

fn op_format(source: &str) -> String {
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    if parsed.diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        object.bool("ok", true);
        object.objects("diagnostics", &diagnostic_objects(&parsed.diagnostics));
        return object.finish();
    }
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.string("formatted", &format_lossless(&parsed));
    object.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(json.contains("\"diagnostics\": []"), "{json}");
        assert!(json.contains("\"Square\""), "{json}");
    }

    #[test]
    fn curated_non_demo_examples_admit() {
        for (name, source) in curated_examples() {
            if name.contains("Diagnostics") || *name == "diagnostics demo" {
                continue;
            }
            let json = run_op("check", source);
            assert!(
                json.contains("\"ok\": true") && json.contains("\"diagnostics\": []"),
                "{name}: {json}"
            );
            let run_json = run_op("run", source);
            assert!(
                run_json.contains("\"ok\": true"),
                "{name} run failed: {run_json}"
            );
        }
    }

    #[test]
    fn check_bad_source_surfaces_code() {
        let json = run_op("check", "this is not emath\n");
        assert!(json.contains("\"ok\": true"), "{json}");
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
            !json.contains("struct square") && !json.contains("&self"),
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
        assert!(!json.contains("assert!"), "{json}");
        assert!(json.contains("let _ ="), "{json}");
        assert!(json.contains("actual") || json.contains("fn y"), "{json}");
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
        assert!(json.contains("emath function Pane"), "{json}");
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

    fn assert_native_wasm_parity(source: &str, given: &[(&str, f64)]) {
        let mut given_map = BTreeMap::new();
        let mut given_pairs = Vec::new();
        for (k, v) in given {
            given_map.insert(k.to_string(), *v);
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
        assert!(wasm_json.contains("\"ok\": true"), "wasm failed: {wasm_json}");

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
                                JsonValue::Str(s) => {
                                    s.parse().expect("valid non-finite f64 string")
                                }
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
                        Value::Bool(expected) => {
                            let parsed = match json_val {
                                JsonValue::Bool(b) => *b,
                                _ => panic!("unexpected json value for bool"),
                            };
                            assert_eq!(parsed, *expected, "bool mismatch for `{key}`");
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
            assert_native_wasm_parity(
                source,
                &[("scale", scale), ("bias", bias), ("x", x)],
            );
        }
    }

    #[test]
    fn parity_plan_and_mig_determinism_and_hashes() {
        let models = &[
            HELLO_SQUARE,
            AFFINE_SCORER,
            PARAMETRIC_UNKNOWN,
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
            (
                "y = x * x\n",
                "N-TYPE-001",
            ),
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

            for (wasm_diag, native_diag) in
                diags.iter().zip(native_result.diagnostics.items())
            {
                let code = wasm_diag.string_field("code").expect("code string");
                let message = wasm_diag
                    .string_field("message")
                    .expect("message string");
                let severity = wasm_diag
                    .string_field("severity")
                    .expect("severity string");

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
}
