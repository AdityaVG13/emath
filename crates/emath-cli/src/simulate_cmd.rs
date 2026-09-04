//! `emath simulate`: explicit, adaptive, implicit, and symplectic integration.

use super::{
    CliExit, EXIT_OK, EXIT_REFUSED, EXIT_USAGE, json_diagnostic_entry, json_diagnostics_entries,
    print_diagnostics, print_json_diagnostics, split_error_code,
};
use emath_artifact::JsonWriter;
use emath_core::limits::Limits;
use emath_exec_ir::interp::{Value, format_f64};
use emath_exec_ir::{SimulateOptions, StepMethod, simulate_continuous_with};
use emath_ir::{Extent, Field, TypeNode};
use emath_sema::CompilerSession;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) fn dispatch_simulate(args: &SimulateArgs) -> CliExit {
    simulate_cmd(args)
}

pub(crate) struct SimulateArgs {
    path: PathBuf,
    model: Option<String>,
    dt: f64,
    t0: f64,
    t1: f64,
    method: StepMethod,
    bindings: BTreeMap<String, Value>,
    json: bool,
    atol: Option<f64>,
    rtol: Option<f64>,
    dt_max: Option<f64>,
    event: Option<(String, f64)>,
}

fn assign_once<T>(slot: &mut Option<T>, value: T) -> Result<(), String> {
    if slot.is_some() {
        Err("duplicate flag".to_string())
    } else {
        *slot = Some(value);
        Ok(())
    }
}

pub(crate) fn parse_simulate_args(args: &[String]) -> Result<SimulateArgs, String> {
    let mut path = None;
    let mut model = None;
    let mut dt = None;
    let mut t0 = None;
    let mut t1 = None;
    let mut method = None;
    let mut bindings = BTreeMap::new();
    let mut json = false;
    let mut atol = None;
    let mut rtol = None;
    let mut dt_max = None;
    let mut event = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--dt" => {
                index += 1;
                assign_once(
                    &mut dt,
                    parse_positive_f64(args.get(index).ok_or("--dt needs a number")?, "--dt")?,
                )?;
            }
            "--t0" => {
                index += 1;
                assign_once(
                    &mut t0,
                    parse_f64(args.get(index).ok_or("--t0 needs a number")?, "--t0")?,
                )?;
            }
            "--t1" => {
                index += 1;
                assign_once(
                    &mut t1,
                    parse_f64(args.get(index).ok_or("--t1 needs a number")?, "--t1")?,
                )?;
            }
            "--method" => {
                index += 1;
                assign_once(
                    &mut method,
                    parse_method(args.get(index).ok_or("--method needs a name")?)?,
                )?;
            }
            "--model" => {
                index += 1;
                let name = args.get(index).ok_or("--model needs a declaration name")?;
                if name.is_empty() {
                    return Err("--model name must be non-empty".to_string());
                }
                assign_once(&mut model, name.to_string())?;
            }
            "--atol" => {
                index += 1;
                assign_once(
                    &mut atol,
                    parse_positive_f64(args.get(index).ok_or("--atol needs a number")?, "--atol")?,
                )?;
            }
            "--rtol" => {
                index += 1;
                assign_once(
                    &mut rtol,
                    parse_positive_f64(args.get(index).ok_or("--rtol needs a number")?, "--rtol")?,
                )?;
            }
            "--dt-max" => {
                index += 1;
                assign_once(
                    &mut dt_max,
                    parse_positive_f64(
                        args.get(index).ok_or("--dt-max needs a number")?,
                        "--dt-max",
                    )?,
                )?;
            }
            "--event" => {
                index += 1;
                assign_once(
                    &mut event,
                    parse_event(args.get(index).ok_or("--event needs name=value")?)?,
                )?;
            }
            "--set" => {
                index += 1;
                let binding = args.get(index).ok_or("--set needs name=value")?;
                let (name, value) = binding
                    .split_once('=')
                    .ok_or_else(|| format!("`--set {binding}` must be `name=value`"))?;
                if name.is_empty() {
                    return Err("--set name must be non-empty".to_string());
                }
                bindings.insert(name.to_string(), parse_set_value(value)?);
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown flag `{other}`"));
            }
            other if path.is_some() => {
                return Err(format!("unexpected extra file `{other}`"));
            }
            other => path = Some(PathBuf::from(other)),
        }
        index += 1;
    }
    Ok(SimulateArgs {
        path: path.ok_or_else(|| "missing <file.emath>".to_string())?,
        model,
        dt: dt.unwrap_or(0.1),
        t0: t0.unwrap_or(0.0),
        t1: t1.unwrap_or(1.0),
        method: method.unwrap_or(StepMethod::Rk4),
        bindings,
        json,
        atol,
        rtol,
        dt_max,
        event,
    })
}

fn parse_f64(text: &str, flag: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("{flag} `{text}` is not a finite Float64"))
}

/// `--set name=value` literals: a scalar, `[v0, v1, …]` vector, or
/// `[[r0c0, …], …]` row-major matrix. Nested depth > 2 is refused.
fn parse_set_value(text: &str) -> Result<Value, String> {
    parse_literal(text.trim(), "--set")
}

fn parse_literal(text: &str, flag: &str) -> Result<Value, String> {
    let text = text.trim();
    if let Some(inner) = text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        return parse_list_literal(inner, flag);
    }
    if text.starts_with('[') {
        return Err(format!("{flag} `{text}` has an unbalanced `[`"));
    }
    parse_f64(text, flag).map(Value::F64)
}

fn parse_list_literal(inner: &str, flag: &str) -> Result<Value, String> {
    let items = split_top_level_commas(inner, flag)?;
    if items.is_empty() {
        return Ok(Value::Vector(Vec::new()));
    }
    let parsed = items
        .iter()
        .map(|item| parse_literal(item, flag))
        .collect::<Result<Vec<_>, _>>()?;
    if parsed.iter().all(|item| matches!(item, Value::F64(_))) {
        return Ok(Value::Vector(
            parsed
                .into_iter()
                .map(|item| match item {
                    Value::F64(number) => number,
                    _ => unreachable!(),
                })
                .collect(),
        ));
    }
    if parsed.iter().all(|item| matches!(item, Value::Vector(_))) {
        let Value::Vector(first) = &parsed[0] else {
            unreachable!()
        };
        let cols = first.len();
        let rows = parsed.len();
        let mut data = Vec::with_capacity(rows * cols);
        for item in parsed {
            let Value::Vector(row) = item else {
                unreachable!()
            };
            if row.len() != cols {
                return Err(format!("{flag} matrix rows must all have length {cols}"));
            }
            data.extend(row);
        }
        return Ok(Value::Matrix { rows, cols, data });
    }
    Err(format!(
        "{flag} list entries must be all scalars (vector) or all equal-length rows (matrix)"
    ))
}

fn split_top_level_commas<'a>(text: &'a str, flag: &str) -> Result<Vec<&'a str>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    for (index, ch) in trimmed.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!("{flag} `{text}` has an unbalanced `]`"));
                }
            }
            ',' if depth == 0 => {
                let item = trimmed[start..index].trim();
                if item.is_empty() {
                    return Err(format!("{flag} list has an empty entry"));
                }
                items.push(item);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("{flag} `{text}` has an unbalanced `[`"));
    }
    let last = trimmed[start..].trim();
    if last.is_empty() {
        return Err(format!("{flag} list has a trailing comma"));
    }
    items.push(last);
    Ok(items)
}

fn parse_positive_f64(text: &str, flag: &str) -> Result<f64, String> {
    let value = parse_f64(text, flag)?;
    if value <= 0.0 {
        return Err(format!("{flag} must be a positive finite Float64"));
    }
    Ok(value)
}

fn parse_event(text: &str) -> Result<(String, f64), String> {
    let (name, value) = text
        .split_once('=')
        .ok_or_else(|| format!("`--event {text}` must be `name=value`"))?;
    if name.is_empty() {
        return Err("--event name must be non-empty".to_string());
    }
    Ok((name.to_string(), parse_f64(value, "--event")?))
}

fn parse_method(name: &str) -> Result<StepMethod, String> {
    match name {
        "euler" => Ok(StepMethod::Euler),
        "rk4" => Ok(StepMethod::Rk4),
        "rk45" => Ok(StepMethod::Rk45),
        "backward-euler" => Ok(StepMethod::BackwardEuler),
        "velocity-verlet" => Ok(StepMethod::VelocityVerlet),
        other => Err(format!(
            "unknown method `{other}` (expected euler, rk4, rk45, backward-euler, or velocity-verlet)"
        )),
    }
}

pub fn simulate_error_json(text: &str) -> String {
    let (code, message) = split_error_code(text).unwrap_or(("error", text));
    let mut out = JsonWriter::object();
    out.string("command", "simulate");
    out.bool("admitted", false);
    out.objects(
        "diagnostics",
        &[json_diagnostic_entry(code, "error", message)],
    );
    out.finish()
}

fn emit_simulate_error(text: &str, json: bool) {
    eprintln!("error: {text}");
    if json {
        print!("{}", simulate_error_json(text));
    }
}

fn simulate_cmd(args: &SimulateArgs) -> CliExit {
    let mut session = CompilerSession::new(Limits::default());
    let Ok(package) = session.load_package(&args.path) else {
        emit_simulate_error(
            &format!(
                "E-PKG-080: cannot read source file ({})",
                args.path.display()
            ),
            args.json,
        );
        return EXIT_REFUSED;
    };
    let result = session.check(package.file);
    if result.diagnostics.has_errors() {
        print_diagnostics(&result.diagnostics);
        if args.json {
            print_json_diagnostics(
                "simulate",
                false,
                &json_diagnostics_entries(&result.diagnostics),
            );
        }
        return EXIT_REFUSED;
    }
    let Some(declaration) = result.package.declarations.iter().find(|declaration| {
        declaration.kind_label == "model"
            && args
                .model
                .as_deref()
                .is_none_or(|name| declaration.name.leaf() == name)
    }) else {
        let message = match &args.model {
            Some(name) => format!(
                "E-MODEL-001: {} has no `emath model {name}` declaration",
                args.path.display()
            ),
            None => format!("{} has no `emath model` declaration", args.path.display()),
        };
        emit_simulate_error(&message, args.json);
        return EXIT_REFUSED;
    };
    let mut inputs = BTreeMap::new();
    for field in &declaration.inputs {
        match bind_field(&args.bindings, field, result.package.ty(field.ty), "input") {
            Ok(value) => {
                inputs.insert(field.name.clone(), value);
            }
            Err(message) => {
                emit_simulate_error(&message, args.json);
                return EXIT_USAGE;
            }
        }
    }
    // Causalized implicit-residual models solve their `algebraic:`
    // unknowns at each step; the interpreter needs the initial guesses in
    // the same value map (causal_newton refuses a silent 0.0 default).
    for field in &declaration.algebraic {
        match bind_field(
            &args.bindings,
            field,
            result.package.ty(field.ty),
            "algebraic guess",
        ) {
            Ok(value) => {
                inputs.insert(field.name.clone(), value);
            }
            Err(message) => {
                emit_simulate_error(&message, args.json);
                return EXIT_USAGE;
            }
        }
    }
    let mut state = BTreeMap::new();
    for field in &declaration.state {
        match bind_field(&args.bindings, field, result.package.ty(field.ty), "state") {
            Ok(value) => {
                state.insert(field.name.clone(), value);
            }
            Err(message) => {
                emit_simulate_error(&message, args.json);
                return EXIT_USAGE;
            }
        }
    }
    match simulate_continuous_with(
        &result.package,
        declaration,
        &inputs,
        &state,
        args.t0,
        args.t1,
        args.dt,
        args.method,
        &SimulateOptions {
            atol: args.atol,
            rtol: args.rtol,
            dt_max: args.dt_max,
            event: args.event.clone(),
        },
    ) {
        Ok(trajectory) => {
            emit_trajectory(&args.path, declaration.name.leaf(), args, &trajectory);
            EXIT_OK
        }
        Err(error) => {
            emit_simulate_error(&error.to_string(), args.json);
            EXIT_REFUSED
        }
    }
}

fn emit_trajectory(
    path: &Path,
    model: &str,
    args: &SimulateArgs,
    trajectory: &emath_exec_ir::Trajectory,
) {
    if args.json {
        let mut samples = Vec::new();
        for sample in &trajectory.samples {
            let mut entry = JsonWriter::object();
            entry.field("t", &format_f64(sample.t));
            let mut state = JsonWriter::object();
            for (name, value) in &sample.state {
                state.field(name, &value_json(value));
            }
            entry.object_field("state", &state.finish());
            samples.push(entry.finish());
        }
        let mut out = JsonWriter::object();
        out.string("command", "simulate");
        out.string("file", &path.display().to_string());
        out.string("model", model);
        out.string("method", method_name(args.method));
        out.field("dt", &format_f64(args.dt));
        out.field("t0", &format_f64(args.t0));
        out.field("t1", &format_f64(args.t1));
        if !trajectory.events.is_empty() {
            let fired = trajectory
                .events
                .iter()
                .map(|event| format!("{}@{}", event.name, format_f64(event.t)))
                .collect::<Vec<_>>()
                .join(",");
            out.string("events", &fired);
        }
        out.objects("samples", &samples);
        print!("{}", out.finish());
        return;
    }
    for event in &trajectory.events {
        println!("event {} fired at t={}", event.name, format_f64(event.t));
    }
    println!(
        "simulate {} model={} method={} dt={} t0={} t1={} samples={}",
        path.display(),
        model,
        method_name(args.method),
        format_f64(args.dt),
        format_f64(args.t0),
        format_f64(args.t1),
        trajectory.samples.len()
    );
    for sample in &trajectory.samples {
        let mut parts = Vec::new();
        for (name, value) in &sample.state {
            parts.push(format!("{name}={}", value));
        }
        println!("t={} {}", format_f64(sample.t), parts.join(" "));
    }
}

fn method_name(method: StepMethod) -> &'static str {
    match method {
        StepMethod::Euler => "euler",
        StepMethod::Rk4 => "rk4",
        StepMethod::Rk45 => "rk45",
        StepMethod::BackwardEuler => "backward-euler",
        StepMethod::VelocityVerlet => "velocity-verlet",
    }
}

fn bind_field(
    bindings: &BTreeMap<String, Value>,
    field: &Field,
    ty: Option<&TypeNode>,
    kind: &str,
) -> Result<Value, String> {
    let Some(value) = bindings.get(&field.name) else {
        return Err(format!(
            "missing {kind} `{}` (pass --set {}=...)",
            field.name, field.name
        ));
    };
    coerce_binding(&field.name, value, ty)
}

fn coerce_binding(name: &str, value: &Value, ty: Option<&TypeNode>) -> Result<Value, String> {
    match ty {
        Some(TypeNode::Vector {
            extent: Some(Extent::Fixed(len)),
            ..
        }) => match value {
            Value::Vector(items) if items.len() == *len => Ok(value.clone()),
            Value::Vector(items) => Err(format!(
                "`{name}` is Vector[{len}]; --set {name}=[...] has length {}",
                items.len()
            )),
            other => Err(format!(
                "`{name}` is Vector[{len}]; pass --set {name}=[v0, v1, …], got {other}"
            )),
        },
        Some(TypeNode::Matrix {
            rows: Some(Extent::Fixed(rows)),
            cols: Some(Extent::Fixed(cols)),
            ..
        }) => match value {
            Value::Matrix {
                rows: got_rows,
                cols: got_cols,
                ..
            } if got_rows == rows && got_cols == cols => Ok(value.clone()),
            Value::Matrix {
                rows: got_rows,
                cols: got_cols,
                ..
            } => Err(format!(
                "`{name}` is Matrix[{rows}, {cols}]; --set {name}=[[...]] is {got_rows}×{got_cols}"
            )),
            other => Err(format!(
                "`{name}` is Matrix[{rows}, {cols}]; pass --set {name}=[[r0c0, …], …], got {other}"
            )),
        },
        Some(TypeNode::Vector { .. } | TypeNode::Matrix { .. } | TypeNode::Tensor { .. }) => {
            Err(format!(
                "`--set` cannot bind `{name}` ({})",
                ty.map(TypeNode::display_name).unwrap_or_default()
            ))
        }
        // Float64, Int, Nat, UnitRef, Refinement, and other scalar slots.
        _ => match value {
            Value::F64(number) => Ok(Value::F64(*number)),
            Value::I64(number) => Ok(Value::F64(*number as f64)),
            // Exact rationals are not f64-settable: demoting would break
            // exactness, so the typed refusal names the value.
            other => Err(format!(
                "`{name}` is a scalar; pass --set {name}=<Float64>, got {other}"
            )),
        },
    }
}

fn value_json(value: &Value) -> String {
    match value {
        Value::F64(number) => format_f64(*number),
        Value::I64(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Text(text) => format!("{text:?}"),
        Value::Series {
            points,
            interpolation,
            extrapolation,
        } => {
            let points = points
                .iter()
                .map(|(time, value)| format!("[{}, {}]", format_f64(*time), format_f64(*value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{{\"points\": [{points}], \"interpolation\": {interpolation:?}, \"extrapolation\": {extrapolation:?}}}"
            )
        }
        Value::Set(items) | Value::List(items) => {
            let body = items.iter().map(value_json).collect::<Vec<_>>().join(", ");
            format!("[{body}]")
        }
        Value::Record { type_name, fields } => {
            let mut entries = vec![format!("\"$type\": {type_name:?}")];
            entries.extend(
                fields
                    .iter()
                    .map(|(name, value)| format!("{name:?}: {}", value_json(value))),
            );
            format!("{{{}}}", entries.join(", "))
        }
        Value::Vector(items) => {
            let body: Vec<String> = items.iter().copied().map(format_f64).collect();
            format!("[{}]", body.join(", "))
        }
        Value::Matrix { rows, cols, data } => {
            let mut rows_json = Vec::with_capacity(*rows);
            for row in 0..*rows {
                let mut cells = Vec::with_capacity(*cols);
                for col in 0..*cols {
                    cells.push(format_f64(data[row * cols + col]));
                }
                rows_json.push(format!("[{}]", cells.join(", ")));
            }
            format!("[{}]", rows_json.join(", "))
        }
        Value::Tensor { data, .. } => {
            let body: Vec<String> = data.iter().copied().map(format_f64).collect();
            format!("[{}]", body.join(", "))
        }
        // Stage-2 (emath-t63iz): exact big values render as canonical
        // decimal digits (JSON-ish number shape, no f64 round trip).
        Value::BigInt(value) => value.to_decimal(),
        Value::BigVector(items) => {
            let body: Vec<String> = items.iter().map(|v| v.to_decimal()).collect();
            format!("[{}]", body.join(", "))
        }
        Value::Rat { num, den } => format!("{num}/{den}"),
        Value::Complex { re, im } => {
            if *im == 0.0 {
                format_f64(*re)
            } else {
                format!(
                    "{}{}{}i",
                    format_f64(*re),
                    if *im >= 0.0 { " + " } else { " - " },
                    format_f64(im.abs())
                )
            }
        }
        Value::Interval { lo, hi } => {
            format!("[{}, {}]", format_f64(*lo), format_f64(*hi))
        }
        // Option/Result carriers: deterministic tagged JSON-ish
        // shapes matching the interp Display convention.
        Value::Option(Some(inner)) => format!("some({})", value_json(inner)),
        Value::Option(None) => "none".to_string(),
        Value::Result { ok, payload } => {
            if *ok {
                format!("ok({})", value_json(payload))
            } else {
                format!("err({})", value_json(payload))
            }
        }
        Value::Program(program) => format!("{:?}", format!("program({program:?})")),
    }
}
