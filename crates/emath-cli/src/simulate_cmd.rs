//! `emath simulate`: explicit Euler / RK4 / RK45 on an admitted `emath model`.

use super::{print_diagnostics, usage, EXIT_OK, EXIT_REFUSED, EXIT_USAGE};
use emath_artifact::JsonWriter;
use emath_core::limits::Limits;
use emath_exec_ir::interp::{format_f64, Value};
use emath_exec_ir::{simulate_continuous_with, SimulateOptions, StepMethod};
use emath_sema::CompilerSession;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn dispatch_simulate(args: &[String]) -> u8 {
    match parse_simulate_args(args) {
        Ok(parsed) => simulate_cmd(&parsed),
        Err(message) => {
            eprintln!("error: {message}");
            usage(
                "simulate <file.emath> [--dt N] [--t0 N] [--t1 N] [--method euler|rk4|rk45] [--atol N] [--rtol N] [--dt-max N] [--event name=value] [--set name=value] [--json]",
            )
        }
    }
}

struct SimulateArgs {
    path: PathBuf,
    dt: f64,
    t0: f64,
    t1: f64,
    method: StepMethod,
    bindings: BTreeMap<String, f64>,
    json: bool,
    atol: Option<f64>,
    rtol: Option<f64>,
    dt_max: Option<f64>,
    event: Option<(String, f64)>,
}

fn parse_simulate_args(args: &[String]) -> Result<SimulateArgs, String> {
    let mut path = None;
    let mut dt = 0.1;
    let mut t0 = 0.0;
    let mut t1 = 1.0;
    let mut method = StepMethod::Rk4;
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
                dt = parse_f64(args.get(index).ok_or("--dt needs a number")?, "--dt")?;
            }
            "--t0" => {
                index += 1;
                t0 = parse_f64(args.get(index).ok_or("--t0 needs a number")?, "--t0")?;
            }
            "--t1" => {
                index += 1;
                t1 = parse_f64(args.get(index).ok_or("--t1 needs a number")?, "--t1")?;
            }
            "--method" => {
                index += 1;
                method = parse_method(args.get(index).ok_or("--method needs a name")?)?;
            }
            "--atol" => {
                index += 1;
                atol = Some(parse_positive_f64(
                    args.get(index).ok_or("--atol needs a number")?,
                    "--atol",
                )?);
            }
            "--rtol" => {
                index += 1;
                rtol = Some(parse_positive_f64(
                    args.get(index).ok_or("--rtol needs a number")?,
                    "--rtol",
                )?);
            }
            "--dt-max" => {
                index += 1;
                dt_max = Some(parse_positive_f64(
                    args.get(index).ok_or("--dt-max needs a number")?,
                    "--dt-max",
                )?);
            }
            "--event" => {
                index += 1;
                event = Some(parse_event(
                    args.get(index).ok_or("--event needs name=value")?,
                )?);
            }
            "--set" => {
                index += 1;
                let binding = args.get(index).ok_or("--set needs name=value")?;
                let (name, value) = binding.split_once('=').ok_or_else(|| {
                    format!("`--set {binding}` must be `name=value`")
                })?;
                if name.is_empty() {
                    return Err("--set name must be non-empty".to_string());
                }
                bindings.insert(name.to_string(), parse_f64(value, "--set")?);
            }
            other if other.starts_with('-') && other != "-" => {}
            other => path = Some(PathBuf::from(other)),
        }
        index += 1;
    }
    Ok(SimulateArgs {
        path: path.ok_or_else(|| "missing <file.emath>".to_string())?,
        dt,
        t0,
        t1,
        method,
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
        other => Err(format!(
            "unknown method `{other}` (expected euler, rk4, or rk45)"
        )),
    }
}

fn simulate_cmd(args: &SimulateArgs) -> u8 {
    let mut session = CompilerSession::new(Limits::default());
    let Ok(package) = session.load_package(&args.path) else {
        eprintln!("error: cannot read {}", args.path.display());
        return EXIT_USAGE;
    };
    let result = session.check(package.file);
    if result.diagnostics.has_errors() {
        print_diagnostics(&result.diagnostics);
        return EXIT_REFUSED;
    }
    let Some(declaration) = result
        .package
        .declarations
        .iter()
        .find(|declaration| declaration.kind_label == "model")
    else {
        eprintln!(
            "error: {} has no `emath model` declaration",
            args.path.display()
        );
        return EXIT_REFUSED;
    };
    let mut inputs = BTreeMap::new();
    for field in &declaration.inputs {
        let Some(value) = args.bindings.get(&field.name).copied() else {
            eprintln!("error: missing input `{}` (pass --set {}=...)", field.name, field.name);
            return EXIT_USAGE;
        };
        inputs.insert(field.name.clone(), Value::F64(value));
    }
    let mut state = BTreeMap::new();
    for field in &declaration.state {
        let Some(value) = args.bindings.get(&field.name).copied() else {
            eprintln!("error: missing state `{}` (pass --set {}=...)", field.name, field.name);
            return EXIT_USAGE;
        };
        state.insert(field.name.clone(), Value::F64(value));
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
            eprintln!("error: {error}");
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
        out.objects("samples", &samples);
        print!("{}", out.finish());
        return;
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
    }
}

fn value_json(value: &Value) -> String {
    match value {
        Value::F64(number) => format_f64(*number),
        Value::Bool(flag) => flag.to_string(),
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
    }
}
