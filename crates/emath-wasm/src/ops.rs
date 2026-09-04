//! Individual wasm operations: check, plan, mig, generate, run.

use super::*;

pub(super) fn op_check(source: &str) -> String {
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

pub(super) fn op_plan(source: &str) -> String {
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

pub(super) fn op_mig(source: &str) -> String {
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

pub(super) fn op_generate(source: &str) -> String {
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

pub(super) fn op_run(payload: &str) -> String {
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

pub(super) struct SolvePayload<'a> {
    pub(super) source: Cow<'a, str>,
    pub(super) apply: Option<emath_syntax::SolveWorld>,
}

pub(super) fn parse_solve_payload(payload: &str) -> Result<SolvePayload<'_>, String> {
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
