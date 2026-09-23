use super::{SavedRun, Path, CliExit, parse_json_document, diagnostic, EXIT_USAGE, case_computed, JsonWriter, SCHEMA, ENGINE, DEFAULT_WORK, text, JsonValue, EXIT_OK, EXIT_REFUSED};

pub(super) fn emit(
    state: &SavedRun,
    path: &Path,
    json: bool,
    issue: Option<(CliExit, &str, &str)>,
) -> CliExit {
    if let Some((_, code, message)) = issue {
        eprintln!("error: {code}: {message}");
    }
    let parsed: Result<Vec<_>, _> = state
        .completed
        .iter()
        .chain(state.active_result.iter())
        .map(|value| parse_json_document(value))
        .collect();
    let results = match parsed {
        Ok(results) => results,
        Err(error) => return diagnostic(json, EXIT_USAGE, "E-RUN-STATE", &error.to_string()),
    };
    let current_target_met = state.completed.len() == state.total
        && state.total > 0
        && results.iter().all(case_computed);
    let completed = current_target_met && state.preserves_original();
    let failed = results
        .iter()
        .any(|result| result.string_field("status").ok().as_deref() == Some("failed"));
    let mut out = JsonWriter::object();
    out.string("schema", SCHEMA);
    out.string(
        "operation_status",
        if issue.is_some_and(|(_, code, _)| code == "E-RUN-CANCELLED") {
            "cancelled"
        } else if failed || issue.is_some() {
            "partial-failed"
        } else {
            "completed"
        },
    );
    out.string("target_id", &state.target_id());
    out.string("original_target_id", &state.original_target());
    out.string(
        "relation",
        state
            .branch
            .as_ref()
            .map_or("same-problem", |branch| branch.relation.as_str()),
    );
    out.string("result_relation_scope", "current-target");
    out.object_field("branch", &state.branch_json());
    out.objects("measurements", &state.measurements);
    let diagnostics =
        issue.map(|(_, code, message)| crate::json_diagnostic_entry(code, "error", message));
    out.objects("diagnostics", &diagnostics.into_iter().collect::<Vec<_>>());
    out.string("meaning_id", &state.meaning_id);
    out.string("language_id", &state.language_id);
    out.string("engine", ENGINE);
    out.string("checkpoint", &path.to_string_lossy());
    out.int("revision", state.revision as u64);
    out.string("work_unit", "case-initialization-or-authored-method-step");
    out.int("completed_cases", state.completed.len() as u64);
    out.int(
        "remaining_cases",
        state.total.saturating_sub(state.completed.len()) as u64,
    );
    let visible_results: Vec<_> = state.completed.iter().chain(state.active_result.iter()).cloned().collect();
    out.objects("results", &visible_results);
    let mut next = Vec::new();
    if state.completed.len() < state.total {
        let claimed = std::fs::read_to_string(path.with_extension("request.json"))
            .ok()
            .and_then(|bytes| parse_json_document(&bytes).ok());
        let origin = claimed
            .as_ref()
            .and_then(|claim| claim.string_field("origin").ok())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let revision = claimed
            .as_ref()
            .and_then(|claim| claim.int_field("revision").ok())
            .unwrap_or(state.revision as u64);
        let work = claimed
            .as_ref()
            .and_then(|claim| claim.int_field("work").ok())
            .unwrap_or(
                state
                    .total
                    .saturating_sub(state.completed.len())
                    .min(DEFAULT_WORK) as u64,
            );
        let mut action = JsonWriter::object();
        action.string("operation", "step");
        action.string("target_relation", "same-target");
        action.strings(
            "argv",
            &[
                "emath".into(),
                "step".into(),
                origin,
                "--expect-revision".into(),
                revision.to_string(),
                "--work".into(),
                work.to_string(),
                "--json".into(),
            ],
        );
        next.push(action.finish());
    }
    out.objects("next", &next);
    if json {
        println!("{}", out.finish());
    } else {
        for result in &results {
            println!(
                "{} / {}: {}",
                text(result, "declaration").unwrap_or_default(),
                text(result, "example").unwrap_or_default(),
                text(result, "status").unwrap_or_default()
            );
            if let Ok(JsonValue::Obj(outputs)) = result.field("outputs") {
                for (name, value) in outputs {
                    println!(
                        "  {name} = {} [{}]",
                        text(value, "value").unwrap_or_default(),
                        text(value, "type").unwrap_or_default()
                    );
                }
            }
            if let Ok(JsonValue::Obj(state)) = result.field("state") {
                for (name, value) in state {
                    println!(
                        "  state.{name} = {} [{}]",
                        text(value, "value").unwrap_or_default(),
                        text(value, "type").unwrap_or_default()
                    );
                }
            }
            for label in ["symbolic", "required_inputs"] {
                if let Ok(JsonValue::Obj(entries)) = result.field(label) {
                    for (name, value) in entries {
                        if let JsonValue::Str(value) = value {
                            println!("  {label}: {name} = {value}");
                        }
                    }
                }
            }
            if let Ok(JsonValue::Arr(remaining)) = result.field("remaining") {
                for value in remaining {
                    if let JsonValue::Str(value) = value {
                        println!("  remaining: {value}");
                    }
                }
            }
        }
        println!(
            "fulfillment={}; completed={}/{}; checkpoint={}",
            if completed { "satisfied" } else { "partial" },
            state.completed.len(),
            state.total,
            path.display()
        );
        if let Some(branch) = &state.branch {
            println!(
                "relation={}; original_target={}; no results merge into the parent",
                branch.relation, branch.original_target
            );
        }
        for measurement in &state.measurements {
            println!("measurement: {measurement}");
        }
        if state.completed.len() < state.total {
            println!("next: emath inspect {path:?} --json");
        }
    }
    if let Some((exit, _, _)) = issue {
        exit
    } else if completed {
        EXIT_OK
    } else {
        EXIT_REFUSED
    }
}

