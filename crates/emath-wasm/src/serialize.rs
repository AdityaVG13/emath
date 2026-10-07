//! Run-report serialization and JSON value helpers.

use super::{prepare_source, session_from_source, JsonWriter, put_pipeline_status, diagnostic_objects, refuse_kind_gone, parse_lossless, FileId, Limits, format_lossless};

pub(super) fn op_inputs(source: &str) -> String {
    let prepared = prepare_source(source);
    let (mut session, file) = session_from_source(&prepared.source);
    let checked = session.check(file);
    if checked.diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        put_pipeline_status(&mut object, &checked.diagnostics);
        object.objects("diagnostics", &diagnostic_objects(&checked.diagnostics));
        return object.finish();
    }
    let Some(module) = checked.prepared else {
        return refuse_kind_gone("inputs");
    };
    let mut declarations = Vec::new();
    for item in &module.tree().items {
        let emath_core::tree::Item::Declaration(decl) = item else {
            continue;
        };
        if !matches!(decl.as_kind.as_str(), "object" | "function" | "query") {
            return refuse_kind_gone("inputs");
        }
        let mut inputs = Vec::new();
        for section in decl.sections().filter(|section| section.name == "inputs") {
            for stmt in &section.suite.statements {
                if let emath_core::tree::StmtKind::FieldDecl { name, ty, .. } = &stmt.kind {
                    let mut entry = JsonWriter::object();
                    entry.string("name", name);
                    let type_name = match &ty.kind {
                        emath_core::tree::TypeKind::Path { segments, .. } => segments.join("::"),
                        _ => "Unknown".to_string(),
                    };
                    entry.string("type", &type_name);
                    entry.bool("defaulted", false);
                    inputs.push(entry.finish().trim_end().to_string());
                }
            }
        }
        let mut object = JsonWriter::object();
        object.string("declaration", &decl.name);
        object.objects("inputs", &inputs);
        declarations.push(object.finish().trim_end().to_string());
    }
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.bool("admitted", true);
    object.string("schema_version", "emath.constructor.v1");
    object.objects("declarations", &declarations);
    object.finish()
}

pub(super) fn serialize_constructor_report(
    report: &emath_exec_ir::constructor_layer::ModuleReport,
) -> String {
    let mut tests = Vec::new();
    for test in &report.tests {
        let mut entry = JsonWriter::object();
        entry.string("label", &test.label);
        entry.bool("passed", test.passed);
        entry.string("detail", &test.detail);
        tests.push(entry.finish().trim_end().to_string());
    }
    let mut bindings = JsonWriter::object();
    for (name, value) in &report.bindings {
        bindings.string(name, &value.to_string());
    }
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.bool("admitted", true);
    object.string("schema_version", "emath.constructor.v1");
    object.object_field("bindings", bindings.finish().trim_end());
    object.objects("tests", &tests);
    object.strings("uses", &report.uses);
    object.finish()
}

pub(super) fn op_format(source: &str) -> String {
    let parsed = parse_lossless(source, FileId(0), &Limits::default());
    if parsed.diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        put_pipeline_status(&mut object, &parsed.diagnostics);
        object.objects("diagnostics", &diagnostic_objects(&parsed.diagnostics));
        return object.finish();
    }
    let (mut session, file) = session_from_source(source);
    let checked = session.check(file);
    let mut object = JsonWriter::object();
    put_pipeline_status(&mut object, &checked.diagnostics);
    object.objects("diagnostics", &diagnostic_objects(&checked.diagnostics));
    object.string("formatted", &format_lossless(&parsed));
    object.finish()
}
