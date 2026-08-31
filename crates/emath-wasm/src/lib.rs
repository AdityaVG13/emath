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

pub mod desugar;

pub use desugar::prepare_source;

use emath_artifact::{JsonValue, JsonWriter, parse_json_document};
use emath_core::{Diagnostics, FileId, Severity, limits::Limits};
use emath_exec_ir::interp::{Value, format_f64};
use emath_exec_ir::runner::{DeclarationRun, RunReport, TestRun, run_package_with_given};
use emath_genesis::{Disposition, ResultBundle, WorldResult};
use emath_ir::Mig;
use emath_rust_backend::BackendInput;
use emath_sema::session::CompilerSession;
use emath_syntax::{format_lossless, install_source_parser, parse_lossless};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// ABI version carried by the `version` op.
pub const ABI_VERSION: u32 = 1;

/// Classic `hello-square` example served by the `examples` op.
pub const HELLO_SQUARE: &str = include_str!("../../../language/examples/intro/hello-square.emath");
/// Stateful affine-scorer tutorial served by the `examples` op.
pub const AFFINE_SCORER: &str =
    include_str!("../../../tests/fixtures/language/intro/stateful-affine-scorer.emath");
/// Sum-1-to-5 example served by the `examples` op.
pub const SUM_ONE_TO_FIVE: &str =
    include_str!("../../../tests/fixtures/language/intro/sum-one-to-five.emath");
/// Tensor-face fixture served by the `examples` op.
pub const TENSOR_FACE: &str = include_str!("../../../tests/fixtures/language/intro/tensor-face.emath");
/// Vector `given`/`expect` example served by the `examples` op.
pub const VECTOR_GIVEN: &str =
    include_str!("../../../tests/fixtures/language/intro/vector-given.emath");
/// Factorial fixture served by the `examples` op.
pub const FACTORIAL: &str = include_str!("../../../tests/fixtures/language/intro/factorial.emath");
/// Range-sum fixture served by the `examples` op.
pub const RANGE_SUM: &str = include_str!("../../../tests/fixtures/language/intro/range-sum.emath");
/// Quantifier fixture served by the `examples` op.
pub const FORALL_EXISTS: &str =
    include_str!("../../../tests/fixtures/language/intro/forall-exists.emath");
/// Integral fixture served by the `examples` op.
pub const INTEGRAL: &str = include_str!("../../../tests/fixtures/language/intro/integral.emath");
/// Autodiff example served by the `examples` op.
pub const AUTODIFF: &str = include_str!("../../../language/examples/intro/autodiff.emath");
/// Equation-solving example served by the `examples` op.
pub const SOLVE: &str = include_str!("../../../language/examples/intro/solve.emath");
/// Optimization example served by the `examples` op.
pub const OPTIMIZE: &str = include_str!("../../../language/examples/intro/optimize.emath");
/// Constrained-optimization fixture served by the `examples` op.
pub const CONSTRAINED_OPT: &str =
    include_str!("../../../tests/fixtures/language/intro/constrained-optimization.emath");

/// Tutorial 1 source: quickstart and scratchpad.
pub const TUTORIAL_01_QUICKSTART: &str = "\
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

/// Tutorial 2 source: 2D curve plotter with parameters.
pub const TUTORIAL_02_PLOTTER: &str = "\
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

/// Tutorial 3 source: math intent and typography.
pub const TUTORIAL_03_MATH_INTENT: &str = "\
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

/// Tutorial 6 source: diagnostics and error recovery.
pub const TUTORIAL_06_DIAGNOSTICS_DEMO: &str = "\
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
pub fn curated_examples() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "Tutorial 1: Quickstart & Scratchpad",
            TUTORIAL_01_QUICKSTART,
        ),
        (
            "Tutorial 2: 2D Curve Plotter & Parameters",
            TUTORIAL_02_PLOTTER,
        ),
        (
            "Tutorial 3: Math Intent & Typography",
            TUTORIAL_03_MATH_INTENT,
        ),
        ("Tutorial 4: Stateful Scorer & Assertions", AFFINE_SCORER),
        (
            "Tutorial 6: Diagnostics & Error Recovery",
            TUTORIAL_06_DIAGNOSTICS_DEMO,
        ),
        ("Hello Square (Classic)", HELLO_SQUARE),
        ("Sum 1 to 5", SUM_ONE_TO_FIVE),
        ("Tensor Face", TENSOR_FACE),
        ("Vector Given", VECTOR_GIVEN),
        ("Factorial (inclusive 1..=n)", FACTORIAL),
        ("Range Sum (variable-bound fold)", RANGE_SUM),
        ("Forall / Exists (quantifier binders)", FORALL_EXISTS),
        ("Integral (numerical integration)", INTEGRAL),
        ("Autodiff (forward-mode derivative)", AUTODIFF),
        ("Solve (Newton's method root-finding)", SOLVE),
        ("Optimize (Newton on ∇f = 0)", OPTIMIZE),
        ("Constrained optimization (penalty method)", CONSTRAINED_OPT),
    ]
}

/// Dispatch one engine op; `payload` is `.emath` source unless the op ignores
/// it, and the reply is one JSON object with deterministic field order.
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
        "solve_candidates" => op_solve_candidates(payload),
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

/// Compile-pipeline seam: build a session from one source string.
///
/// Public so embedders and tests drive the exact path `run_op` uses
/// (same file name, same limits) rather than a divergent setup.
pub fn session_from_source(source: &str) -> (CompilerSession, emath_core::FileId) {
    let mut session = CompilerSession::new(Limits::default());
    let file = session.load_text("input.emath", source.to_string());
    (session, file)
}

fn maybe_desugared(object: &mut emath_artifact::JsonObject, desugared: Option<&str>) {
    if let Some(source) = desugared {
        object.string("desugared_source", source);
    }
}

/// Pipeline ran (`ok`) is not admission. Untrusted pane text stays
/// `admitted: false` until diagnostics are clean.
fn put_pipeline_status(object: &mut emath_artifact::JsonObject, diagnostics: &Diagnostics) {
    object.bool("ok", true);
    object.bool("admitted", !diagnostics.has_errors());
}

struct RunPayload<'a> {
    source: Cow<'a, str>,
    given: Option<BTreeMap<String, Value>>,
}

fn parse_run_payload<'a>(payload: &'a str) -> Result<RunPayload<'a>, String> {
    let trimmed = payload.trim_start();
    if !trimmed.starts_with('{') {
        return Ok(RunPayload {
            source: Cow::Borrowed(payload),
            given: None,
        });
    }
    let Ok(value) = parse_json_document(payload.trim()) else {
        return Ok(RunPayload {
            source: Cow::Borrowed(payload),
            given: None,
        });
    };
    let Ok(source) = value.string_field("source") else {
        return Ok(RunPayload {
            source: Cow::Borrowed(payload),
            given: None,
        });
    };
    if let JsonValue::Obj(entries) = &value {
        if let Some(key) = first_duplicate_key(entries) {
            return Err(format!("run envelope duplicates `{key}`"));
        }
    }
    Ok(RunPayload {
        source: Cow::Owned(source),
        given: parse_given_field(&value)?,
    })
}

fn first_duplicate_key(entries: &[(String, JsonValue)]) -> Option<&str> {
    let mut seen = BTreeMap::new();
    for (key, _) in entries {
        if seen.insert(key.as_str(), ()).is_some() {
            return Some(key.as_str());
        }
    }
    None
}

fn parse_given_field(value: &JsonValue) -> Result<Option<BTreeMap<String, Value>>, String> {
    let Ok(given) = value.field("given") else {
        return Ok(None);
    };
    let JsonValue::Obj(entries) = given else {
        return Err("given must be a JSON object".into());
    };
    if let Some(name) = first_duplicate_key(entries) {
        return Err(format!("given `{name}` is duplicated"));
    }
    let mut map = BTreeMap::new();
    for (name, entry) in entries {
        let Some(parsed) = parse_json_value(entry) else {
            return Err(format!(
                "given `{name}` is not a finite Float64, vector, or matrix"
            ));
        };
        map.insert(name.clone(), parsed);
    }
    Ok(Some(map))
}

fn parse_finite_f64(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// Parse a pane `given` entry into a typed [`Value`]: scalars, vectors,
/// matrices (array-of-arrays), and tensors (`{ shape, data }`).
fn parse_json_value(entry: &JsonValue) -> Option<Value> {
    match entry {
        JsonValue::Num(text) | JsonValue::Str(text) => parse_finite_f64(text).map(Value::F64),
        JsonValue::Bool(flag) => Some(Value::Bool(*flag)),
        JsonValue::Arr(list) => parse_json_array(list),
        JsonValue::Obj(entries) => parse_json_tensor(entries),
        JsonValue::Null => None,
    }
}

fn parse_json_array(list: &[JsonValue]) -> Option<Value> {
    if list
        .iter()
        .all(|item| matches!(item, JsonValue::Num(_) | JsonValue::Str(_)))
    {
        let elements = list
            .iter()
            .map(|item| match item {
                JsonValue::Num(t) | JsonValue::Str(t) => parse_finite_f64(t),
                _ => None,
            })
            .collect::<Option<Vec<f64>>>()?;
        Some(Value::Vector(elements))
    } else if list.iter().all(|item| matches!(item, JsonValue::Arr(_))) {
        let rows = list.len();
        let mut data = Vec::new();
        let mut cols: Option<usize> = None;
        for row in list {
            let JsonValue::Arr(cells) = row else {
                return None;
            };
            let row_len = cells.len();
            match cols {
                None => cols = Some(row_len),
                Some(c) if c == row_len => {}
                _ => return None,
            }
            for cell in cells {
                let n = match cell {
                    JsonValue::Num(t) | JsonValue::Str(t) => parse_finite_f64(t)?,
                    _ => return None,
                };
                data.push(n);
            }
        }
        Some(Value::Matrix {
            rows,
            cols: cols?,
            data,
        })
    } else {
        None
    }
}

fn parse_json_tensor(entries: &[(String, JsonValue)]) -> Option<Value> {
    if first_duplicate_key(entries).is_some() {
        return None;
    }
    let shape = entries
        .iter()
        .find(|(k, _)| k == "shape")
        .and_then(|(_, v)| {
            if let JsonValue::Arr(s) = v {
                Some(s)
            } else {
                None
            }
        })?;
    let data = entries
        .iter()
        .find(|(k, _)| k == "data")
        .and_then(|(_, v)| {
            if let JsonValue::Arr(d) = v {
                Some(d)
            } else {
                None
            }
        })?;
    let shape: Vec<usize> = shape
        .iter()
        .map(|s| match s {
            JsonValue::Num(t) | JsonValue::Str(t) => t.parse::<usize>().ok(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let data: Vec<f64> = data
        .iter()
        .map(|d| match d {
            JsonValue::Num(t) | JsonValue::Str(t) => parse_finite_f64(t),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Value::Tensor { shape, data })
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
    put_pipeline_status(&mut object, &result.diagnostics);
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
    put_pipeline_status(&mut object, &result.diagnostics);
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
    put_pipeline_status(&mut object, &result.diagnostics);
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
        put_pipeline_status(&mut object, &result.diagnostics);
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
    put_pipeline_status(&mut object, &result.diagnostics);
    object.string("crate_name", &crate_name);
    object.objects("files", &files);
    maybe_desugared(&mut object, prepared.desugared());
    object.finish()
}

fn op_run(payload: &str) -> String {
    let envelope = match parse_run_payload(payload) {
        Ok(envelope) => envelope,
        Err(message) => return error_json(&message),
    };
    let prepared = prepare_source(&envelope.source);
    let (mut session, file) = session_from_source(&prepared.source);
    let result = session.check(file);
    if result.diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        put_pipeline_status(&mut object, &result.diagnostics);
        object.objects("diagnostics", &diagnostic_objects(&result.diagnostics));
        maybe_desugared(&mut object, prepared.desugared());
        return object.finish();
    }
    let report = run_package_with_given(&result.package, envelope.given.as_ref());
    serialize_run_report(&report, prepared.desugared())
}

struct SolvePayload<'a> {
    source: Cow<'a, str>,
    apply: Option<emath_syntax::SolveWorld>,
}

fn parse_solve_payload(payload: &str) -> Result<SolvePayload<'_>, String> {
    if !payload.trim_start().starts_with('{') {
        return Ok(SolvePayload {
            source: Cow::Borrowed(payload),
            apply: None,
        });
    }
    let value = parse_json_document(payload.trim())
        .map_err(|_| "solve_candidates expects source text or a JSON envelope".to_string())?;
    if let JsonValue::Obj(entries) = &value
        && let Some(key) = first_duplicate_key(entries)
    {
        return Err(format!("solve_candidates envelope duplicates `{key}`"));
    }
    let source = value
        .string_field("source")
        .map_err(|_| "solve_candidates envelope requires string `source`".to_string())?;
    let apply = match value.string_field("apply") {
        Ok(label) => Some(
            emath_syntax::SolveWorld::parse_label(&label)
                .ok_or_else(|| format!("unknown solve candidate `{label}`"))?,
        ),
        Err(_) => None,
    };
    Ok(SolvePayload {
        source: Cow::Owned(source),
        apply,
    })
}

fn solve_disposition(source: &str, world: emath_syntax::SolveWorld) -> Disposition {
    let normalized: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();
    if !normalized.starts_with("solvex^2=2") {
        return Disposition::Refused {
            reason: "intent candidate execution currently requires the admitted equation `x^2 = 2`"
                .to_string(),
        };
    }
    match world {
        emath_syntax::SolveWorld::RealPm => Disposition::Answer {
            canonical: "x ∈ {-sqrt(2), sqrt(2)} over Real".to_string(),
        },
        emath_syntax::SolveWorld::Complex => Disposition::Answer {
            canonical: "x ∈ {-sqrt(2) + 0i, sqrt(2) + 0i} over Complex".to_string(),
        },
        emath_syntax::SolveWorld::Symbolic => Disposition::Answer {
            canonical: "roots(x^2 - 2) = {-sqrt(2), sqrt(2)}".to_string(),
        },
        emath_syntax::SolveWorld::Modular => Disposition::Open {
            missing: vec!["modulus".to_string()],
        },
        emath_syntax::SolveWorld::Numeric => Disposition::Open {
            missing: vec!["tolerance".to_string()],
        },
    }
}

fn solve_world_result(source: &str, world: emath_syntax::SolveWorld) -> WorldResult {
    WorldResult {
        world: world.as_str().to_string(),
        origin: "intent-completion".to_string(),
        method: world.method().to_string(),
        term_canonical: "solve x^2 = 2".to_string(),
        inputs: BTreeMap::new(),
        assumptions: vec![
            format!("domain={}", world.domain()),
            format!("exactness={}", world.exactness()),
        ],
        disposition: solve_disposition(source, world),
        evidence_laws: vec![format!("{} verification", world.evidence_class())],
        cost_steps: 0,
    }
}

fn op_solve_candidates(payload: &str) -> String {
    let request = match parse_solve_payload(payload) {
        Ok(request) => request,
        Err(message) => return error_json(&message),
    };
    let expansion = emath_syntax::expand_scratch(&request.source);
    if expansion.diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        put_pipeline_status(&mut object, &expansion.diagnostics);
        object.objects("diagnostics", &diagnostic_objects(&expansion.diagnostics));
        return object.finish();
    }
    if matches!(expansion.solve, emath_syntax::SolveIntent::Absent) {
        return error_json("source has no `solve` intent");
    }

    let worlds: Vec<emath_syntax::SolveWorld> = request
        .apply
        .map_or_else(|| expansion.solve.menu().to_vec(), |world| vec![world]);
    let results = worlds
        .iter()
        .map(|world| solve_world_result(&request.source, *world))
        .collect();
    let Ok(bundle) = ResultBundle::new(results) else {
        return error_json("solve candidate produced a naked result");
    };

    let mut teacher = JsonWriter::object();
    teacher.string("understood", "solve x^2 = 2");
    teacher.string("missing", "domain, numeric policy, and method selection");
    teacher.string(
        "repair",
        "choose a labeled candidate; candidates with parameters retain typed holes",
    );
    teacher.string(
        "authority",
        "no candidate is promoted to the intended meaning until selected",
    );

    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.strings(
        "ambiguity_dimensions",
        &[
            "domain".to_string(),
            "numeric".to_string(),
            "method".to_string(),
        ],
    );
    object.object_field("teacher", &teacher.finish());
    object.object_field("result_bundle", &bundle.to_json());
    if let Some(world) = request.apply {
        match emath_syntax::apply_solve_candidate(&request.source, world) {
            Ok((rewritten, delta)) => {
                object.string("apply", world.as_str());
                object.string("source", &rewritten);
                object.string("meaning_delta", &delta);
            }
            Err(message) => return error_json(&message),
        }
    }
    object.finish()
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
    put_pipeline_status(&mut object, &result.diagnostics);
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
    object.bool("admitted", true);
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
    object.object_field("given", &value_map_value(&test.given));
    if !test.state.is_empty() {
        object.object_field("state", &value_map_value(&test.state));
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

fn value_map_value(map: &BTreeMap<String, Value>) -> String {
    let mut object = JsonWriter::object();
    for (name, value) in map {
        object.field(name, &value_json(value));
    }
    object.finish().trim_end().to_string()
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", character as u32);
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

fn value_json(value: &Value) -> String {
    match value {
        Value::F64(number) => json_f64(*number),
        Value::I64(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Text(text) => json_string(text),
        Value::Series {
            points,
            interpolation,
            extrapolation,
        } => {
            let points = points
                .iter()
                .map(|(time, value)| format!("[{},{}]", json_f64(*time), json_f64(*value)))
                .collect::<Vec<_>>()
                .join(",");
            let mut object = JsonWriter::object();
            object.field("points", &format!("[{points}]"));
            object.string("interpolation", interpolation);
            object.string("extrapolation", extrapolation);
            object.finish().trim_end().to_string()
        }
        Value::Set(values) => format!(
            "[{}]",
            values.iter().map(value_json).collect::<Vec<_>>().join(",")
        ),
        Value::Record { type_name, fields } => {
            let mut field_object = JsonWriter::object();
            for (name, value) in fields {
                field_object.field(name, &value_json(value));
            }
            let mut object = JsonWriter::object();
            object.string("type", type_name);
            object.object_field("fields", &field_object.finish());
            object.finish().trim_end().to_string()
        }
        Value::Complex { re, im } => {
            let mut object = JsonWriter::object();
            object.field("re", &json_f64(*re));
            object.field("im", &json_f64(*im));
            object.finish().trim_end().to_string()
        }
        Value::Vector(elements) => json_f64_list(elements),
        Value::Matrix { rows, cols, data } => json_matrix(*rows, *cols, data),
        Value::Tensor { shape, data } => json_tensor(shape, data),
        Value::Interval { lo, hi } => format!("[{},{}]", json_f64(*lo), json_f64(*hi)),
        Value::Option(None) => "null".to_string(),
        Value::Option(Some(inner)) => value_json(inner),
        Value::Result { ok, payload } => {
            let mut object = JsonWriter::object();
            object.bool("ok", *ok);
            object.field("payload", &value_json(payload));
            object.finish().trim_end().to_string()
        }
        Value::Rat { num, den } => {
            // Exact rational: keep num/den intact in JSON (no float rounding),
            // mirroring the `num/den` Display form used by the interpreter.
            let mut object = JsonWriter::object();
            object.field("num", &num.to_string());
            object.field("den", &den.to_string());
            object.finish().trim_end().to_string()
        }
    }
}

fn json_f64_list(values: &[f64]) -> String {
    let body = values
        .iter()
        .map(|value| json_f64(*value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{body}]")
}

fn json_matrix(rows: usize, cols: usize, data: &[f64]) -> String {
    let mut rows_json = Vec::with_capacity(rows);
    for row in 0..rows {
        let start = row * cols;
        rows_json.push(json_f64_list(&data[start..start + cols]));
    }
    format!("[{}]", rows_json.join(", "))
}

fn json_tensor(shape: &[usize], data: &[f64]) -> String {
    let mut object = JsonWriter::object();
    object.field("shape", &json_usize_list(shape));
    object.field("data", &json_f64_list(data));
    object.finish().trim_end().to_string()
}

fn json_usize_list(values: &[usize]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
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
        put_pipeline_status(&mut object, &parsed.diagnostics);
        object.objects("diagnostics", &diagnostic_objects(&parsed.diagnostics));
        return object.finish();
    }
    let mut object = JsonWriter::object();
    put_pipeline_status(&mut object, &parsed.diagnostics);
    object.string("formatted", &format_lossless(&parsed));
    object.finish()
}

