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

#[allow(unreachable_code, unused_variables)]
pub(super) fn op_plan(source: &str) -> String {
    let _ = source;
    return refuse_kind_gone("plan");
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

#[allow(unreachable_code, unused_variables)]
pub(super) fn op_mig(source: &str) -> String {
    let _ = source;
    return refuse_kind_gone("mig");
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
    let (tree, diagnostics) = emath_syntax::parse_str(&prepared.source);
    if diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        put_pipeline_status(&mut object, &diagnostics);
        object.objects("diagnostics", &diagnostic_objects(&diagnostics));
        return object.finish();
    }
    let constructor_only = tree.items.iter().all(|item| match item {
        emath_core::tree::Item::Use { .. } => true,
        emath_core::tree::Item::Declaration(decl) => {
            matches!(decl.as_kind.as_str(), "object" | "function" | "query")
        }
        _ => true,
    });
    if !constructor_only {
        return refuse_kind_gone("generate");
    }
    let functions: Vec<String> = tree
        .items
        .iter()
        .filter_map(|item| match item {
            emath_core::tree::Item::Declaration(decl) if decl.as_kind == "function" => {
                Some(decl.name.clone())
            }
            _ => None,
        })
        .collect();
    if functions.is_empty() {
        return refuse_kind_gone("generate");
    }
    let mut rust = String::from("#![forbid(unsafe_code)]\n\n");
    for name in &functions {
        match emath_exec_ir::constructor_emir::lower_constructor_function(&tree, name) {
            Ok(lowered) if lowered.runnable => {
                match emath_rust_backend::emit_constructor_program(&lowered.program, &lowered.inputs)
                {
                    Ok(body) => {
                        rust.push_str(&format!("// function `{name}`\n"));
                        rust.push_str(&body);
                        rust.push('\n');
                    }
                    Err(err) => return error_json(&err.to_string()),
                }
            }
            Ok(lowered) => {
                rust.push_str(&format!(
                    "// `{name}` is not marked runnable: {}\n",
                    lowered.unresolved.join(", ")
                ));
            }
            Err(err) => return error_json(&err),
        }
    }
    let mut file = JsonWriter::object();
    file.string("path", "src/lib.rs");
    file.string("content", &rust);
    let mut object = JsonWriter::object();
    object.bool("ok", true);
    object.bool("admitted", true);
    object.string("schema_version", "emath.constructor.v1");
    object.string("crate_name", functions.first().map(String::as_str).unwrap_or("package"));
    object.objects("files", &[file.finish().trim_end().to_string()]);
    object.finish()
}

pub(super) fn op_run(payload: &str) -> String {
    let envelope = match parse_run_payload(payload) {
        Ok(envelope) => envelope,
        Err(message) => return error_json(&message),
    };
    let prepared = prepare_source(&envelope.source);
    let (tree, diagnostics) = emath_syntax::parse_str(&prepared.source);
    if diagnostics.has_errors() {
        let mut object = JsonWriter::object();
        put_pipeline_status(&mut object, &diagnostics);
        object.objects("diagnostics", &diagnostic_objects(&diagnostics));
        return object.finish();
    }
    if let Err(err) = emath_exec_ir::constructor_layer::admit_tree(&tree) {
        return constructor_error_json(&err);
    }
    if let Some(given) = &envelope.given
        && let Some(name) = first_function_name(&tree)
    {
        for input in function_input_names(&tree, &name) {
            if !given.contains_key(&input) {
                return error_json(&format!("missing input `{input}`"));
            }
        }
        let declared = function_input_types(&tree, &name);
        let mut inputs = BTreeMap::new();
        for (key, value) in given {
            match value_to_cvalue(declared.get(key).map(String::as_str), value) {
                Ok(converted) => {
                    inputs.insert(key.clone(), converted);
                }
                Err(message) => return error_json(&message),
            }
        }
        match emath_exec_ir::constructor_layer::evaluate_function(&tree, &name, &inputs) {
            Ok(value) => {
                let mut report = match emath_exec_ir::constructor_layer::evaluate_tree(&tree) {
                    Ok(report) => report,
                    Err(err) => return constructor_error_json(&err),
                };
                report.bindings.insert(name, value);
                return serialize_constructor_report(&report);
            }
            Err(err) => return constructor_error_json(&err),
        }
    }
    match emath_exec_ir::constructor_layer::evaluate_tree(&tree) {
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

fn function_input_types(
    tree: &emath_core::tree::SyntaxTree,
    name: &str,
) -> BTreeMap<String, String> {
    tree.items
        .iter()
        .find_map(|item| match item {
            emath_core::tree::Item::Declaration(decl)
                if decl.as_kind == "function" && decl.name == name =>
            {
                let mut types = BTreeMap::new();
                for section in decl.sections().filter(|section| section.name == "inputs") {
                    for stmt in &section.suite.statements {
                        if let emath_core::tree::StmtKind::FieldDecl { name, ty, .. } = &stmt.kind {
                            let label = match &ty.kind {
                                emath_core::tree::TypeKind::Path { segments, .. } => {
                                    segments.last().cloned().unwrap_or_default()
                                }
                                _ => String::new(),
                            };
                            types.insert(name.clone(), label);
                        }
                    }
                }
                Some(types)
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn function_input_names(tree: &emath_core::tree::SyntaxTree, name: &str) -> Vec<String> {
    tree.items
        .iter()
        .find_map(|item| match item {
            emath_core::tree::Item::Declaration(decl)
                if decl.as_kind == "function" && decl.name == name =>
            {
                Some(
                    decl.sections()
                        .filter(|section| section.name == "inputs")
                        .flat_map(|section| {
                            section.suite.statements.iter().filter_map(|stmt| {
                                if let emath_core::tree::StmtKind::FieldDecl { name, .. } =
                                    &stmt.kind
                                {
                                    Some(name.clone())
                                } else {
                                    None
                                }
                            })
                        })
                        .collect(),
                )
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn value_to_cvalue(
    declared: Option<&str>,
    value: &Value,
) -> Result<emath_exec_ir::constructor_layer::CValue, String> {
    use emath_exec_ir::constructor_layer::CValue;
    match (declared, value) {
        (Some("Int"), Value::F64(n))
            if n.is_finite() && n.fract() == 0.0 && *n >= i128::MIN as f64 && *n <= i128::MAX as f64 =>
        {
            Ok(CValue::Int(*n as i128))
        }
        (Some("Int"), Value::I64(n)) => Ok(CValue::Int(i128::from(*n))),
        (Some("Float64"), Value::F64(n)) => Ok(CValue::Float64(*n)),
        (_, Value::F64(n)) => Ok(CValue::Float64(*n)),
        (_, Value::I64(n)) => Ok(CValue::Int(i128::from(*n))),
        (_, Value::Bool(flag)) => Ok(CValue::Bool(*flag)),
        (_, Value::Rat { num, den }) => Ok(CValue::Rat {
            num: *num,
            den: *den,
        }),
        (_, Value::Vector(items)) => Ok(CValue::Sequence(
            items.iter().copied().map(CValue::Float64).collect(),
        )),
        (_, other) => Err(format!(
            "given value `{other:?}` is not a constructor scalar, bool, or vector"
        )),
    }
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
