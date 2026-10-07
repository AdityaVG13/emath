//! Individual wasm operations: check, generate, run.

use super::{session_from_source, JsonWriter, put_pipeline_status, diagnostic_objects, declaration_names, error_json, parse_run_payload, BTreeMap, serialize_constructor_report, Value};

pub(super) fn op_check(source: &str) -> String {
    let (mut session, file) = session_from_source(source);
    let result = session.check(file);
    let mut object = JsonWriter::object();
    put_pipeline_status(&mut object, &result.diagnostics);
    object.objects("diagnostics", &diagnostic_objects(&result.diagnostics));
    object.strings("declarations", &declaration_names(&result.package));
    object.finish()
}

pub(super) fn refuse_kind_gone(op: &str) -> String {
    let mut diagnostics = JsonWriter::object();
    diagnostics.string("severity", "error");
    diagnostics.string("code", "E-KIND-GONE");
    let message = format!(
        "`{op}` is not a constructor command. Write an ordinary `emath function` or `emath query`."
    );
    diagnostics.string("message", &message);
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.bool("admitted", false);
    object.string("schema_version", "emath.constructor.v1");
    object.string("admission", "refused");
    object.objects("diagnostics", &[diagnostics.finish().trim_end().to_string()]);
    object.finish()
}

pub(super) fn op_generate(source: &str) -> String {
    let (mut session, file) = session_from_source(source);
    let checked = session.check(file);
    if checked.diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        put_pipeline_status(&mut object, &checked.diagnostics);
        object.objects("diagnostics", &diagnostic_objects(&checked.diagnostics));
        return object.finish();
    }
    let Some(module) = checked.prepared else {
        return refuse_kind_gone("generate");
    };
    let emission = match emath_rust_backend::constructor_crate::emit_constructor_crate(
        &module, std::path::Path::new("input.emath"), source,
    ) {
        Ok(emission) => emission,
        Err(emath_rust_backend::constructor_crate::ConstructorEmitRefusal::NotConstructor(_)) => return refuse_kind_gone("generate"),
        Err(emath_rust_backend::constructor_crate::ConstructorEmitRefusal::ImportsRefused { e_code, detail }) => return error_json(&format!("{e_code}: {detail}")),
    };
    let files: Vec<String> = emission.files().into_iter().map(|(path, contents)| {
        let mut file = JsonWriter::object();
        file.string("path", &path);
        file.string("content", &contents);
        file.finish().trim_end().to_string()
    }).collect();
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.bool("admitted", true);
    object.string("schema_version", "emath.constructor.v1");
    object.string("crate_name", &emission.package_name);
    object.bool("runnable", emission.runnable);
    object.strings("unresolved", &emission.unresolved);
    object.objects("files", &files);
    object.finish()
}

pub(super) fn op_run(payload: &str) -> String {
    let envelope = match parse_run_payload(payload) {
        Ok(envelope) => envelope,
        Err(message) => return error_json(&message),
    };
    let (mut session, file) = session_from_source(&envelope.source);
    let checked = session.check(file);
    if checked.diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        put_pipeline_status(&mut object, &checked.diagnostics);
        object.objects("diagnostics", &diagnostic_objects(&checked.diagnostics));
        return object.finish();
    }
    let Some(module) = checked.prepared else {
        return refuse_kind_gone("run");
    };
    if let Some(given) = &envelope.given
        && let Some(name) = first_function_name(module.tree())
    {
        let signature = module.input_types(&name).unwrap_or_default();
        for input in signature.keys() {
            if !given.contains_key(input) {
                return error_json(&format!("missing input `{input}`"));
            }
        }
        let declared: BTreeMap<String, String> = signature.iter().map(|(name, ty)| {
            let label = match &ty.kind {
                emath_core::tree::TypeKind::Path { segments, .. } => segments.last().cloned().unwrap_or_default(),
                _ => String::new(),
            };
            (name.clone(), label)
        }).collect();
        let mut inputs = BTreeMap::new();
        for (key, value) in given {
            match value_to_cvalue(declared.get(key).map(String::as_str), value) {
                Ok(converted) => {
                    inputs.insert(key.clone(), converted);
                }
                Err(message) => return error_json(&message),
            }
        }
        match module.evaluate_function(&name, &inputs, emath_exec_ir::constructor_layer::DEFAULT_WORK) {
            Ok(value) => {
                let mut report = match module.evaluate_tests(emath_exec_ir::constructor_layer::DEFAULT_WORK) {
                    Ok(report) => report,
                    Err(err) => return constructor_error_json(&err),
                };
                report.bindings.insert(name, value);
                return serialize_constructor_report(&report);
            }
            Err(err) => return constructor_error_json(&err),
        }
    }
    match module.evaluate_tests(emath_exec_ir::constructor_layer::DEFAULT_WORK) {
        Ok(report) => serialize_constructor_report(&report),
        Err(err) => constructor_error_json(&err),
    }
}

fn constructor_error_json(err: &emath_exec_ir::constructor_layer::ConstructorError) -> String {
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.bool("admitted", false);
    object.string("schema_version", "emath.constructor.v1");
    object.string("admission", "refused");
    object.string("code", &err.code);
    object.string("error", &err.message);
    object.finish()
}

fn first_function_name(tree: &emath_core::tree::SyntaxTree) -> Option<String> {
    tree.items.iter().find_map(|item| match item {
        emath_core::tree::Item::Declaration(decl) if decl.as_kind == "function" => {
            Some(decl.name.clone())
        }
        _ => None,
    })
}

fn value_to_cvalue(
    declared: Option<&str>,
    value: &Value,
) -> Result<emath_exec_ir::constructor_layer::CValue, String> {
    use emath_exec_ir::constructor_layer::CValue;
    match (declared, value) {
        (Some("Int"), Value::F64(n))
            if n.is_finite()
                && n.fract() == 0.0
                && n.abs() <= 9_007_199_254_740_992.0 =>
        {
            Ok(CValue::Int(emath_exec_ir::exact_int::ExactInt::from(
                *n as i64,
            )))
        }
        (Some("Int"), Value::I64(n)) => Ok(CValue::Int(emath_exec_ir::exact_int::ExactInt::from(*n))),
        (Some("Float64"), Value::F64(n)) => Ok(CValue::Float64(*n)),
        (_, Value::F64(n)) => Ok(CValue::Float64(*n)),
        (_, Value::I64(n)) => Ok(CValue::Int(emath_exec_ir::exact_int::ExactInt::from(*n))),
        (_, Value::Bool(flag)) => Ok(CValue::Bool(*flag)),
        (_, Value::Rat { num, den }) => emath_exec_ir::constructor_layer::exact_rat(
            emath_exec_ir::exact_int::ExactInt::from(*num),
            emath_exec_ir::exact_int::ExactInt::from(*den),
        )
        .map_err(|err| err.message),
        (_, Value::Vector(items)) => Ok(CValue::Sequence(std::sync::Arc::new(
            items.iter().copied().map(CValue::Float64).collect(),
        ))),
        (_, other) => Err(format!(
            "given value `{other:?}` is not a constructor scalar, bool, or vector"
        )),
    }
}
