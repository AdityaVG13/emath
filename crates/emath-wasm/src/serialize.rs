//! Run-report serialization and JSON value helpers.

use super::*;

pub(super) fn op_inputs(source: &str) -> String {
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

pub(super) fn serialize_run_report(report: &RunReport, desugared: Option<&str>) -> String {
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

pub(super) fn serialize_declaration_run(declaration: &DeclarationRun) -> String {
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

pub(super) fn serialize_test_run(test: &TestRun) -> String {
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

pub(super) fn value_map_value(map: &BTreeMap<String, Value>) -> String {
    let mut object = JsonWriter::object();
    for (name, value) in map {
        object.field(name, &value_json(value));
    }
    object.finish().trim_end().to_string()
}

pub(super) fn json_string(value: &str) -> String {
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

pub(super) fn value_json(value: &Value) -> String {
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

pub(super) fn json_f64_list(values: &[f64]) -> String {
    let body = values
        .iter()
        .map(|value| json_f64(*value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{body}]")
}

pub(super) fn json_matrix(rows: usize, cols: usize, data: &[f64]) -> String {
    let mut rows_json = Vec::with_capacity(rows);
    for row in 0..rows {
        let start = row * cols;
        rows_json.push(json_f64_list(&data[start..start + cols]));
    }
    format!("[{}]", rows_json.join(", "))
}

pub(super) fn json_tensor(shape: &[usize], data: &[f64]) -> String {
    let mut object = JsonWriter::object();
    object.field("shape", &json_usize_list(shape));
    object.field("data", &json_f64_list(data));
    object.finish().trim_end().to_string()
}

pub(super) fn json_usize_list(values: &[usize]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub(super) fn json_f64(value: f64) -> String {
    if value.is_finite() {
        format_f64(value)
    } else {
        format!("\"{}\"", format_f64(value))
    }
}

pub(super) fn op_format(source: &str) -> String {
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
