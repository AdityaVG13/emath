use super::{SemanticPackage, BTreeMap, MethodFrame, Instant, JsonWriter, ENGINE, progress, run_test, parse_set_value_for, run_direct, TestRun, GoalKind, TestVerdict, Value};

pub(super) fn execute_measured_case(
    package: &SemanticPackage,
    declaration: &emath_ir::Declaration,
    test: Option<usize>,
    raw: &BTreeMap<String, String>,
    repetitions: usize,
    previous: &[MethodFrame],
) -> Result<(String, Option<String>, Vec<MethodFrame>), String> {
    if repetitions == 0 {
        return execute_case(package, declaration, test, raw, previous, true).map(|(result, frames)| (result, None, frames));
    }
    let mut samples = Vec::with_capacity(repetitions);
    let mut answer = None;
    for _ in 0..repetitions {
        let start = Instant::now();
        let result = execute_case(package, declaration, test, raw, previous, true)?;
        let elapsed = u64::try_from(start.elapsed().as_nanos())
            .map_err(|_| "execution duration exceeds u64 nanoseconds")?;
        if answer.as_ref().is_some_and(|prior| prior != &result) {
            return Err("E-MEASURE-RESULT: repeated execution changed the result; no latency comparison is admitted".into());
        }
        samples.push(elapsed);
        if answer.is_none() {
            answer = Some(result);
        }
    }
    let measurement = emath_lab_core::measure::Measurement {
        metric_id: "reference-case".into(),
        kind: emath_lab_core::measure::MeasurementKind::LatencyNs,
        unit: "ns".into(),
        samples,
    };
    let summary = measurement.summarize().map_err(|error| error.to_string())?;
    let mut out = JsonWriter::object();
    out.string("kind", measurement.kind.as_str());
    out.string("unit", &measurement.unit);
    out.string("engine", ENGINE);
    out.string("os", std::env::consts::OS);
    out.string("arch", std::env::consts::ARCH);
    out.string("scope", "reference case binding, lowering, execution and value rendering; excludes admission and storage; not generated-code performance");
    out.string(
        "evidence",
        "observed wall time; not a certified bound or speedup claim",
    );
    out.strings(
        "samples_ns",
        &measurement
            .samples
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>(),
    );
    out.string("median_ns", &summary.median.to_string());
    out.bool("quarantined", summary.quarantined());
    let (answer, frames) = answer.ok_or("measurement needs at least one execution")?;
    Ok((answer, Some(out.finish()), frames))
}

pub(super) fn execute_case(
    package: &SemanticPackage,
    declaration: &emath_ir::Declaration,
    test: Option<usize>,
    raw: &BTreeMap<String, String>,
    previous: &[MethodFrame],
    advance: bool,
) -> Result<(String, Vec<MethodFrame>), String> {
    let (result, frames) = progress::with_frames(previous, advance, || {
    let result = if let Some(index) = test {
        let test = package
            .tests
            .get(index)
            .ok_or("saved example index is invalid")?;
        run_test(package, declaration, test)
    } else {
        let mut given = BTreeMap::new();
        for (name, value) in raw {
            let field = declaration
                .inputs
                .iter()
                .chain(&declaration.state)
                .chain(
                    declaration
                        .constructors
                        .iter()
                        .flat_map(|constructor| &constructor.parameters),
                )
                .find(|field| field.name == *name)
                .ok_or_else(|| {
                    format!("'{name}' is not a declared input, state, or constructor parameter")
                })?;
            let ty = package
                .ty(field.ty)
                .ok_or("declared input type is absent")?;
            let value = parse_set_value_for(Some(ty), value).ok_or_else(|| format!("cannot bind '{name}' to {}; check the value and vector length; other carriers need a source example", ty.display_name()))?;
            given.insert(name.clone(), value);
        }
        run_direct(package, declaration, &given)
    };
    Ok::<_, String>(result)
    });
    let result = result?;
    Ok((result_json(package, declaration, &result, &frames), frames))
}

pub(super) fn result_json(
    package: &SemanticPackage,
    declaration: &emath_ir::Declaration,
    run: &TestRun,
    frames: &[MethodFrame],
) -> String {
    let mut remaining = Vec::new();
    for id in &declaration.goals {
        if let Some(goal) = package.goal(*id) {
            if goal.kind != GoalKind::Evaluate {
                remaining.push(format!(
                    "{} {}: this case computed definitions, not this separate goal",
                    goal.kind.as_str(),
                    goal.target
                ));
            }
        }
    }
    for output in &declaration.outputs {
        if !run.outputs.contains_key(&output.name)
            && !run.definitions.contains_key(&output.name)
            && !run.state.contains_key(&output.name)
        {
            remaining.push(format!("output '{}' has no computed value", output.name));
        }
    }
    for frame in frames {
        if progress::complete(&frame.state) != Some(true) {
            remaining.push(format!("{}: the saved method has not met its mathematical goal", frame.capability));
        }
        if let Some(fault) = &frame.fault { remaining.push(fault.clone()); }
    }
    let success = matches!(run.verdict, TestVerdict::Passed | TestVerdict::Computed);
    let has_result = !run.definitions.is_empty()
        || !run.outputs.is_empty()
        || !run.state.is_empty()
        || run.verdict.expect_passed();
    let mut out = JsonWriter::object();
    out.string("declaration", declaration.name.leaf());
    out.string("example", &run.name);
    out.string("relation", "same-problem");
    out.string(
        "status",
        if run.verdict.is_symbolic() {
            "unresolved"
        } else if success && has_result {
            "computed"
        } else {
            "failed"
        },
    );
    out.string(
        "evidence",
        if run.verdict.expect_passed() {
            "source-example-checked"
        } else {
            "reference-execution;not-a-proof"
        },
    );
    out.object_field("inputs", &values_json(&run.given));
    if !run.state.is_empty() {
        out.object_field("state", &values_json(&run.state));
    }
    out.object_field(
        "outputs",
        &values_json(if run.outputs.is_empty() {
            &run.definitions
        } else {
            &run.outputs
        }),
    );
    if let TestVerdict::Symbolic { forms, holes, .. } = &run.verdict {
        let mut symbolic = JsonWriter::object();
        for (name, form) in forms {
            symbolic.string(name, form);
        }
        out.object_field("symbolic", &symbolic.finish());
        let mut required = JsonWriter::object();
        for (name, ty) in holes {
            required.string(name, ty);
        }
        out.object_field("required_inputs", &required.finish());
    }
    if let Some(reason) = run.verdict.reason_text() {
        remaining.push(reason);
    }
    if matches!(run.verdict, TestVerdict::Failed) {
        remaining.push("the source expectation evaluated to false".into());
    }
    if !has_result {
        remaining.push("no mathematical value was produced by this case".into());
    }
    out.strings("remaining", &remaining);
    out.finish()
}

pub(super) fn values_json(values: &BTreeMap<String, Value>) -> String {
    let mut out = JsonWriter::object();
    for (name, value) in values { out.object_field(name, &value.json()); }
    out.finish()
}

