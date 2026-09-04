//! Run-payload parsing, session setup, and JSON value conversion.

use super::*;

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

pub(super) fn op_version() -> String {
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.string("version", env!("CARGO_PKG_VERSION"));
    object.int("abi", u64::from(ABI_VERSION));
    object.finish()
}

pub(super) fn op_examples() -> String {
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

pub(super) fn maybe_desugared(object: &mut emath_artifact::JsonObject, desugared: Option<&str>) {
    if let Some(source) = desugared {
        object.string("desugared_source", source);
    }
}

/// Pipeline ran (`ok`) is not admission. Untrusted pane text stays
/// `admitted: false` until diagnostics are clean.
pub(super) fn put_pipeline_status(
    object: &mut emath_artifact::JsonObject,
    diagnostics: &Diagnostics,
) {
    object.bool("ok", true);
    object.bool("admitted", !diagnostics.has_errors());
}

pub(super) struct RunPayload<'a> {
    pub(super) source: Cow<'a, str>,
    pub(super) given: Option<BTreeMap<String, Value>>,
}

pub(super) fn parse_run_payload<'a>(payload: &'a str) -> Result<RunPayload<'a>, String> {
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

pub(super) fn first_duplicate_key(entries: &[(String, JsonValue)]) -> Option<&str> {
    let mut seen = BTreeMap::new();
    for (key, _) in entries {
        if seen.insert(key.as_str(), ()).is_some() {
            return Some(key.as_str());
        }
    }
    None
}

pub(super) fn parse_given_field(
    value: &JsonValue,
) -> Result<Option<BTreeMap<String, Value>>, String> {
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

pub(super) fn parse_finite_f64(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// Parse a pane `given` entry into a typed [`Value`]: scalars, vectors,
/// matrices (array-of-arrays), and tensors (`{ shape, data }`).
pub(super) fn parse_json_value(entry: &JsonValue) -> Option<Value> {
    match entry {
        JsonValue::Num(text) | JsonValue::Str(text) => parse_finite_f64(text).map(Value::F64),
        JsonValue::Bool(flag) => Some(Value::Bool(*flag)),
        JsonValue::Arr(list) => parse_json_array(list),
        JsonValue::Obj(entries) => parse_json_tensor(entries),
        JsonValue::Null => None,
    }
}

pub(super) fn parse_json_array(list: &[JsonValue]) -> Option<Value> {
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

pub(super) fn parse_json_tensor(entries: &[(String, JsonValue)]) -> Option<Value> {
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

pub(super) fn diagnostic_objects(diagnostics: &Diagnostics) -> Vec<String> {
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

pub(super) fn declaration_names(package: &emath_ir::SemanticPackage) -> Vec<String> {
    let mut names = Vec::with_capacity(package.declarations.len());
    for declaration in &package.declarations {
        names.push(declaration.name.leaf().to_string());
    }
    names
}

pub(super) fn crate_name_of(package: &emath_ir::SemanticPackage) -> String {
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
