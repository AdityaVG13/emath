use super::*;

pub(super) fn run_constructor_layer(request: &RunRequest, source: &str) -> Option<CliExit> {
    let (tree, diagnostics) = emath_syntax::parse_str(source);
    if diagnostics.has_errors() {
        crate::print_diagnostics(&diagnostics);
        return Some(EXIT_ADMISSION);
    }
    let constructor_only = tree.items.iter().all(|item| match item {
        emath_core::tree::Item::Use { .. } => true,
        emath_core::tree::Item::Declaration(decl) => {
            matches!(decl.as_kind.as_str(), "object" | "function" | "query")
        }
        _ => true,
    });
    let has_constructor = tree.items.iter().any(|item| matches!(
        item,
        emath_core::tree::Item::Declaration(decl)
            if matches!(decl.as_kind.as_str(), "object" | "function" | "query")
    ));
    if !constructor_only || !has_constructor {
        return Some(diagnostic(
            request.json,
            EXIT_ADMISSION,
            "E-KIND-GONE",
            "`emath run` evaluates `emath object`, `emath function`, and `emath query`. Other declaration kinds are not constructors.",
        ));
    }
    let inputs = match constructor_inputs(request) {
        Ok(inputs) => inputs,
        Err(exit) => return Some(exit),
    };
    // `--function` runs one entry; the module's authored tests are not
    // an input to that evaluation. Gating first would evaluate every
    // test under DEFAULT_WORK and refuse modules whose suites are
    // larger than that default, regardless of --work — the budget the
    // flag exists to raise. The report is only consumed by the
    // no-function lane below.
    let report = if request.function.is_some() {
        None
    } else {
        let evaluated = if request.work_set {
            emath_exec_ir::constructor_layer::evaluate_tree_budgeted_at(
                &tree,
                Some(&request.path),
                request.work as u64,
            )
        } else {
            emath_exec_ir::constructor_layer::evaluate_tree_at(&tree, Some(&request.path))
        };
        match evaluated {
            Ok(report) => Some(report),
            Err(err) => {
                return Some(diagnostic(
                    request.json,
                    if err.code == "E-KIND-GONE" {
                        EXIT_ADMISSION
                    } else if err.code == "incompatible_checkpoint" {
                        crate::EXIT_CHECKPOINT
                    } else if err.code == "budget_exhausted" {
                        EXIT_PARTIAL
                    } else {
                        EXIT_FAULT
                    },
                    &err.code,
                    &err.message,
                ));
            }
        }
    };
    if let Some(name) = &request.function {
        if request.work_set {
            return Some(run_constructor_budgeted(request, source, &tree, name, &inputs));
        }
        match emath_exec_ir::constructor_layer::evaluate_function_at(
            &tree,
            name,
            &inputs,
            Some(&request.path),
        ) {
            Ok(value) => {
                return Some(print_constructor_json(
                    request.json,
                    "returned",
                    "satisfied",
                    constructor_representation(&value),
                    &value.to_string(),
                    None,
                    EXIT_OK,
                ));
            }
            // `unbound` is the unknown-entry code: fall through to the query
            // lookup and the E-RUN-ENTRY refusal. Any other error means the
            // entry EXISTS and its evaluation faulted — surface the fault
            // instead of masking it as a missing entry.
            Err(err) if err.code == "unbound" => {}
            Err(err) => {
                return Some(diagnostic(
                    request.json,
                    EXIT_FAULT,
                    &err.code,
                    &err.message,
                ));
            }
        }
        if let Ok(receipt) = emath_exec_ir::constructor_layer::evaluate_query_at(
            &tree,
            name,
            &inputs,
            Some(&request.path),
        )
        {
            let exit = match receipt.fulfillment.as_str() {
                "satisfied" => EXIT_OK,
                "partial" | "unmet" => EXIT_PARTIAL,
                _ => EXIT_FAULT,
            };
            return Some(print_constructor_receipt(request.json, &receipt, exit));
        }
        return Some(diagnostic(
            request.json,
            EXIT_ADMISSION,
            "E-RUN-ENTRY",
            &format!("no constructor entry `{name}`"),
        ));
    }
    // Every path above returns; the only way here is the no-function
    // lane, which always evaluated the tree (report is Some).
    let report = report.expect("no-function lane evaluated the tree above");
    let failed = report.tests.iter().any(|test| !test.passed);
    let partial = report.tests.iter().any(|test| {
        test.receipt
            .as_ref()
            .is_some_and(|receipt| matches!(receipt.fulfillment.as_str(), "partial" | "unmet"))
    });
    let exit = if failed || partial {
        EXIT_PARTIAL
    } else {
        EXIT_OK
    };
    let mut out = JsonWriter::object();
    out.string("schema_version", "emath.constructor.v1");
    out.string("command", "run");
    out.string("admission", "ok");
    println!("{}", out.finish());
    let _ = request;
    Some(exit)
}

pub(super) fn run_constructor_budgeted(
    request: &RunRequest,
    source: &str,
    tree: &emath_core::tree::SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, emath_exec_ir::constructor_layer::CValue>,
) -> CliExit {
    let source_id = content_id_of_str(source).0;
    match emath_exec_ir::constructor_layer::evaluate_function_budgeted_reported(
        tree,
        name,
        inputs,
        request.work as u64,
        None,
        &source_id,
        Some(&request.path),
        source,
    ) {
        Ok((value, work)) => print_constructor_json(
            request.json,
            "returned",
            "satisfied",
            constructor_representation(&value),
            &value.to_string(),
            Some(work),
            EXIT_OK,
        ),
        Err((err, mut checkpoint)) => {
            checkpoint.function = name.into();
            checkpoint.source = source.into();
            checkpoint.inputs = inputs.clone();
            checkpoint.source_id = source_id;
            if err.code == "budget_exhausted" {
                if let Err(write_err) = write_constructor_checkpoint(request, &checkpoint) {
                    return diagnostic(request.json, EXIT_FAULT, "E-IO", &write_err);
                }
                return print_constructor_json(
                    request.json,
                    "suspended",
                    "partial",
                    "absent",
                    // Self-describing in both modes: a bare `work=N` on
                    // the human lane would not say the run suspended.
                    &format!(
                        "suspended: work={} (checkpoint written; resume with `emath step <checkpoint> --work N`)",
                        checkpoint.work
                    ),
                    None,
                    EXIT_PARTIAL,
                );
            }
            if err.code == "incompatible_checkpoint" {
                return diagnostic(request.json, crate::EXIT_CHECKPOINT, &err.code, &err.message);
            }
            diagnostic(request.json, EXIT_FAULT, &err.code, &err.message)
        }
    }
}

pub(super) fn write_constructor_checkpoint(
    request: &RunRequest,
    checkpoint: &emath_exec_ir::constructor_layer::Checkpoint,
) -> Result<(), String> {
    std::fs::create_dir_all(&request.out).map_err(|err| err.to_string())?;
    let path = request.out.join("constructor-checkpoint.json");
    std::fs::write(path, checkpoint.encode()).map_err(|err| err.to_string())
}

pub(super) fn constructor_inspect(path: &Path, json: bool) -> Option<CliExit> {
    let text = std::fs::read_to_string(path).ok()?;
    if !emath_exec_ir::constructor_layer::is_constructor_checkpoint(&text) {
        return None;
    }
    match emath_exec_ir::constructor_layer::Checkpoint::decode(&text) {
        Ok(checkpoint) => {
            if json {
                let mut out = JsonWriter::object();
                out.string("schema_version", "emath.constructor.v1");
                out.string("command", "inspect");
                out.string("admission", "ok");
                out.string("execution", "suspended");
                out.string("fulfillment", "partial");
                out.string("function", &checkpoint.function);
                out.string("source_id", &checkpoint.source_id);
                out.string("image", &checkpoint.image);
                out.int("work", checkpoint.work);
                out.int("remaining", checkpoint.remaining);
                out.string("accounting", &checkpoint.accounting);
                out.objects("evidence", &[]);
                println!("{}", out.finish());
            } else {
                println!(
                    "constructor checkpoint function={} work={} source_id={}",
                    checkpoint.function, checkpoint.work, checkpoint.source_id
                );
            }
            Some(EXIT_OK)
        }
        Err(err) => Some(diagnostic(
            json,
            crate::EXIT_CHECKPOINT,
            &err.code,
            &err.message,
        )),
    }
}

pub(super) fn constructor_step(request: &RunRequest) -> Option<CliExit> {
    let text = std::fs::read_to_string(&request.path).ok()?;
    if !emath_exec_ir::constructor_layer::is_constructor_checkpoint(&text) {
        return None;
    }
    let checkpoint = match emath_exec_ir::constructor_layer::Checkpoint::decode(&text) {
        Ok(checkpoint) => checkpoint,
        Err(err) => {
            return Some(diagnostic(
                request.json,
                crate::EXIT_CHECKPOINT,
                &err.code,
                &err.message,
            ));
        }
    };
    let (tree, diagnostics) = emath_syntax::parse_str(&checkpoint.source);
    if diagnostics.has_errors() {
        return Some(diagnostic(
            request.json,
            crate::EXIT_CHECKPOINT,
            "incompatible_checkpoint",
            "checkpoint source no longer parses",
        ));
    }
    let extra = if request.work_set {
        request.work as u64
    } else {
        64
    };
    match emath_exec_ir::constructor_layer::evaluate_function_budgeted_reported(
        &tree,
        &checkpoint.function,
        &checkpoint.inputs,
        checkpoint.work + extra,
        Some(&checkpoint),
        &checkpoint.source_id,
        None,
        &checkpoint.source,
    ) {
        Ok((value, work)) => Some(print_constructor_json(
            request.json,
            "returned",
            "satisfied",
            constructor_representation(&value),
            &value.to_string(),
            Some(work),
            EXIT_OK,
        )),
        Err((err, next)) => {
            if err.code == "budget_exhausted" {
                if let Err(write_err) = write_constructor_checkpoint(request, &next) {
                    return Some(diagnostic(request.json, EXIT_FAULT, "E-IO", &write_err));
                }
                return Some(print_constructor_json(
                    request.json,
                    "suspended",
                    "partial",
                    "absent",
                    &format!("work={}", next.work),
                    None,
                    EXIT_PARTIAL,
                ));
            }
            Some(diagnostic(
                request.json,
                if err.code == "incompatible_checkpoint" {
                    crate::EXIT_CHECKPOINT
                } else {
                    EXIT_FAULT
                },
                &err.code,
                &err.message,
            ))
        }
    }
}

pub(super) fn parse_constructor_literal(raw: &str) -> emath_exec_ir::constructor_layer::CValue {
    emath_exec_ir::constructor_layer::parse_constructor_scalar(raw)
}

pub(super) fn constructor_inputs(
    request: &RunRequest,
) -> Result<BTreeMap<String, emath_exec_ir::constructor_layer::CValue>, CliExit> {
    let mut inputs = BTreeMap::new();
    if let Some(path) = &request.set_file {
        match load_set_file(path) {
            Ok(file_inputs) => inputs = file_inputs,
            Err((code, message)) => {
                return Err(diagnostic(request.json, EXIT_USAGE, code, &message));
            }
        }
    }
    for (name, raw) in &request.given {
        inputs.insert(name.clone(), parse_constructor_literal(raw));
    }
    Ok(inputs)
}

pub(super) fn load_set_file(
    path: &Path,
) -> Result<BTreeMap<String, emath_exec_ir::constructor_layer::CValue>, (&'static str, String)> {
    let text = std::fs::read_to_string(path).map_err(|err| {
        (
            "E-RUN-SET-FILE",
            format!("cannot read --set-file {}: {err}", path.display()),
        )
    })?;
    let json = parse_json_document(&text).map_err(|err| {
        (
            "E-RUN-SET-FILE",
            format!("--set-file {}: {err}", path.display()),
        )
    })?;
    let JsonValue::Obj(entries) = json else {
        return Err((
            "E-RUN-SET-FILE",
            "--set-file must be a JSON object of name to scalar or array".into(),
        ));
    };
    let mut inputs = BTreeMap::new();
    for (name, value) in entries {
        if name.is_empty() {
            return Err(("E-RUN-SET-FILE", "--set-file keys must be non-empty".into()));
        }
        let raw = json_to_set_raw(&value).map_err(|err| ("E-RUN-SET-FILE", err))?;
        inputs.insert(name, parse_constructor_literal(&raw));
    }
    Ok(inputs)
}

pub(super) fn json_to_set_raw(value: &JsonValue) -> Result<String, String> {
    match value {
        JsonValue::Str(text) => Ok(text.clone()),
        JsonValue::Num(text) => Ok(text.clone()),
        JsonValue::Bool(true) => Ok("true".into()),
        JsonValue::Bool(false) => Ok("false".into()),
        JsonValue::Arr(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(json_to_set_raw(item)?);
            }
            Ok(format!("[{}]", parts.join(", ")))
        }
        JsonValue::Null => Err("--set-file null is not a constructor value".into()),
        JsonValue::Obj(_) => Err("--set-file nested objects are not constructor values".into()),
    }
}

pub(super) fn print_constructor_receipt(
    json: bool,
    receipt: &emath_exec_ir::constructor_layer::Receipt,
    exit: CliExit,
) -> CliExit {
    if json {
        let mut out = JsonWriter::object();
        out.string("schema_version", "emath.constructor.v1");
        out.string("command", "run");
        out.string("admission", "ok");
        out.string("execution", &receipt.execution);
        out.string("fulfillment", &receipt.fulfillment);
        out.string("representation", &receipt.representation);
        if let Some(payload) = &receipt.payload {
            out.string("payload", payload);
        }
        out.strings("evidence", &receipt.evidence);
        out.strings("remaining", &receipt.remaining);
        println!("{}", out.finish());
    } else {
        println!(
            "{} {} {}",
            receipt.execution, receipt.fulfillment, receipt.representation
        );
    }
    exit
}

pub(super) fn constructor_representation(value: &emath_exec_ir::constructor_layer::CValue) -> &'static str {
    match value {
        emath_exec_ir::constructor_layer::CValue::Int(_)
        | emath_exec_ir::constructor_layer::CValue::Rat { .. }
        | emath_exec_ir::constructor_layer::CValue::Bool(_) => "exact_scalar",
        emath_exec_ir::constructor_layer::CValue::Float64(_) => "rounded_scalar",
        emath_exec_ir::constructor_layer::CValue::Code(_) => "code",
        emath_exec_ir::constructor_layer::CValue::Absent
        | emath_exec_ir::constructor_layer::CValue::Unit => "absent",
        _ => "structured",
    }
}

pub(super) fn print_constructor_json(
    json: bool,
    execution: &str,
    fulfillment: &str,
    representation: &str,
    payload: &str,
    work_consumed: Option<u64>,
    exit: CliExit,
) -> CliExit {
    if json {
        let mut out = JsonWriter::object();
        out.string("schema_version", "emath.constructor.v1");
        out.string("command", "run");
        out.string("admission", "ok");
        out.string("execution", execution);
        out.string("fulfillment", fulfillment);
        out.string("representation", representation);
        out.string("payload", payload);
        // Work telemetry for `--work` tuning (bead emath-7zplf): the
        // completed run's measured consumption, present only on the
        // budgeted lanes that know it.
        if let Some(work) = work_consumed {
            out.int("work_consumed", work);
        }
        out.objects("evidence", &[]);
        out.strings("remaining", &[]);
        println!("{}", out.finish());
    } else {
        println!("{payload}");
    }
    exit
}

