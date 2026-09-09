//! Fixed-input search over authored functions. Only generated native code is timed.
//! Accepted candidates match the checked reference result bit-for-bit on these inputs;
//! this is not a proof of equivalence on other inputs or a universal optimum.

use crate::{CliExit, EXIT_OK, EXIT_REFUSED, execution};
use emath_artifact::JsonWriter;
use emath_core::{content_id_of_str, limits::Limits};
use emath_exec_ir::{interp::Value, progress, runner::run_direct};
use emath_ir::{Declaration, GoalKind, SemanticPackage};
use emath_lab_core::measure::{Measurement, MeasurementKind};
use emath_rust_backend::{
    BackendInput,
    rust_ir::{
        ast::{Expr, FnDef, Item, Ty, escape_ident},
        render::{render_expr, render_ty},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    process::Command,
};

pub(crate) const USAGE: &str = "search <file.emath> --function BASELINE --candidate NAME [--candidate NAME] [--set name=value] [--measure N] [--out dir] [--json]";

pub(crate) struct SearchRequest {
    path: PathBuf,
    baseline: String,
    candidates: Vec<String>,
    given: BTreeMap<String, String>,
    repetitions: usize,
    out: PathBuf,
    json: bool,
}

impl SearchRequest {
    pub(crate) fn parse(args: &[String]) -> Option<Self> {
        let (mut path, mut baseline, mut out, mut repetitions) = (None, None, None, None);
        let (mut candidates, mut given, mut json, mut index) =
            (Vec::new(), BTreeMap::new(), false, 0);
        while index < args.len() {
            match args[index].as_str() {
                "--function" => crate::assign_once(
                    &mut baseline,
                    crate::take_nonflag_value(args, &mut index)?.to_string(),
                )?,
                "--candidate" => {
                    let name = crate::take_nonflag_value(args, &mut index)?.to_string();
                    if candidates.contains(&name) || candidates.len() == 32 {
                        return None;
                    }
                    candidates.push(name);
                }
                "--set" => {
                    let (name, value) =
                        crate::take_nonflag_value(args, &mut index)?.split_once('=')?;
                    if name.is_empty()
                        || value.is_empty()
                        || given.insert(name.to_string(), value.to_string()).is_some()
                    {
                        return None;
                    }
                }
                "--measure" => {
                    let count = crate::take_nonflag_value(args, &mut index)?
                        .parse::<usize>()
                        .ok()?;
                    if !(2..=32).contains(&count) {
                        return None;
                    }
                    crate::assign_once(&mut repetitions, count)?;
                }
                "--out" | "-o" => crate::assign_once(
                    &mut out,
                    PathBuf::from(crate::take_nonflag_value(args, &mut index)?),
                )?,
                "--json" => json = true,
                value if value.starts_with('-') => return None,
                value => crate::assign_once(&mut path, PathBuf::from(value))?,
            }
            index += 1;
        }
        if candidates.is_empty() {
            return None;
        }
        Some(Self {
            path: path?,
            baseline: baseline?,
            candidates,
            given,
            repetitions: repetitions.unwrap_or(9),
            out: out.unwrap_or_else(|| "target/emath/search".into()),
            json,
        })
    }
}

pub(crate) fn run(request: SearchRequest) -> CliExit {
    match execute(&request) {
        Ok((report, path, goal_met)) => {
            if request.json {
                println!("{report}");
            } else {
                println!(
                    "compiled search complete; mathematical_goal_met={goal_met}\nreport: {}",
                    path.display()
                );
            }
            if goal_met { EXIT_OK } else { EXIT_REFUSED }
        }
        Err(error) => {
            execution::diagnostic(request.json, EXIT_REFUSED, "E-COMPILED-SEARCH", &error)
        }
    }
}

fn declaration<'a>(package: &'a SemanticPackage, name: &str) -> Result<&'a Declaration, String> {
    let found = package
        .declarations
        .iter()
        .filter(|decl| decl.name.0 == name || decl.name.leaf() == name)
        .collect::<Vec<_>>();
    match found.as_slice() {
        [decl] if decl.state.is_empty() && decl.constructors.is_empty() => Ok(decl),
        [_] => Err(format!(
            "{name}: compiled search requires a stateless function"
        )),
        _ => Err(format!("{name}: expected one source declaration")),
    }
}

fn target<'a>(package: &'a SemanticPackage, declaration: &Declaration) -> Result<&'a str, String> {
    let goals = declaration
        .goals
        .iter()
        .filter_map(|id| package.goal(*id))
        .collect::<Vec<_>>();
    match goals.as_slice() {
        [goal] if goal.kind == GoalKind::Evaluate => Ok(&goal.target),
        _ => Err(format!(
            "{}: declare one evaluate goal for compiled comparison",
            declaration.name.0
        )),
    }
}

fn result_ty(ty: &Ty) -> &Ty {
    if let Ty::Result { ok, .. } = ty {
        ok
    } else {
        ty
    }
}

fn execute(request: &SearchRequest) -> Result<(String, PathBuf, bool), String> {
    if crate::refuse_malformed_project_lock(&request.path).is_some() {
        return Err("project lock refused".into());
    }
    let distribution = crate::load_verified_language(Some(&request.path))?;
    let mut session = emath_sema::CompilerSession::new(Limits::default());
    let source = session
        .load_package(&request.path)
        .map_err(|error| format!("cannot load source: {error:?}"))?;
    let checked = session.plan(source.file);
    if checked.diagnostics.has_errors() {
        return Err(format!(
            "source admission failed: {:?}",
            checked.diagnostics
        ));
    }
    let package = &checked.package;
    let baseline = declaration(package, &request.baseline)?;
    let target_name = target(package, baseline)?;
    let mut given = BTreeMap::new();
    for (name, raw) in &request.given {
        let field = baseline
            .inputs
            .iter()
            .find(|field| field.name == *name)
            .ok_or_else(|| format!("{name}: not a baseline input"))?;
        let ty = package.ty(field.ty).ok_or("missing input type")?;
        let value = execution::parse_set_value_for(Some(ty), raw).ok_or_else(|| {
            format!(
                "cannot bind {name} as {}; use the run input syntax",
                ty.display_name()
            )
        })?;
        given.insert(name.clone(), value);
    }
    if given.len() != baseline.inputs.len() {
        return Err("bind every baseline input with --set".into());
    }
    let (reference, frames) =
        progress::with_frames(&[], false, || run_direct(package, baseline, &given));
    if reference.verdict.is_refused() {
        return Err(format!("baseline reference refused: {}", reference.verdict));
    }
    let expected = reference
        .definitions
        .get(target_name)
        .ok_or("baseline target did not compute a value")?;
    let goal_met = frames.iter().all(|frame| {
        frame.fault.is_none()
            && emath_exec_ir::native_kernel::method_complete(&frame.state) == Some(true)
    });
    let names = std::iter::once(&request.baseline)
        .chain(&request.candidates)
        .collect::<Vec<_>>();
    let mut selected_declarations = Vec::with_capacity(names.len());
    let mut seen = BTreeSet::new();
    for name in &names {
        let decl = declaration(package, name)?;
        target(package, decl)?;
        if !seen.insert(decl.id.index()) {
            return Err(format!("{name}: repeated declaration"));
        }
        if decl.inputs.len() != baseline.inputs.len()
            || decl
                .inputs
                .iter()
                .zip(&baseline.inputs)
                .any(|(a, b)| a.name != b.name || package.ty(a.ty) != package.ty(b.ty))
        {
            return Err(format!(
                "{name}: candidates must have the same ordered input names and types as the baseline"
            ));
        }
        selected_declarations.push(decl);
    }
    let mut generated = BackendInput {
        package,
        crate_name: "emath_compiled_search".into(),
        version: "0.0.0".into(),
    }
    .generate()
    .map_err(|error| error.to_string())?;
    let mut functions = Vec::with_capacity(names.len());
    for decl in &selected_declarations {
        let name = escape_ident(decl.name.leaf());
        let found = generated
            .module
            .items
            .iter()
            .filter_map(|item| {
                if let Item::Fn(function) = item {
                    (function.name == name).then_some(function)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let [function] = found.as_slice() else {
            return Err(format!("{name}: no unique generated free function"));
        };
        if functions
            .first()
            .is_some_and(|first: &&FnDef| result_ty(&first.ret) != result_ty(&function.ret))
        {
            return Err(format!(
                "{name}: candidate result type differs from baseline"
            ));
        }
        functions.push(*function);
    }
    let harness = harness(
        &functions,
        &baseline
            .inputs
            .iter()
            .map(|field| &given[&field.name])
            .collect::<Vec<_>>(),
        expected,
        request.repetitions,
    )?;
    generated.files.insert("src/main.rs".into(), harness);
    let mut bindings = JsonWriter::object();
    for (name, value) in &given {
        bindings.object_field(name, value.json().trim());
    }
    let bindings = bindings.finish();
    let meaning = package.content_id().0;
    let language = distribution.image.distribution_hash.to_string();
    let target_id = content_id_of_str(&format!(
        "{meaning}\n{language}\n{}\n{target_name}\n{bindings}",
        baseline.name.0
    ))
    .0;
    fs::create_dir_all(&request.out).map_err(|error| error.to_string())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let directory = request
        .out
        .join(format!("{}-{nonce}", target_id.replace(':', "-")));
    fs::create_dir(&directory).map_err(|error| error.to_string())?;
    let directory = fs::canonicalize(directory).map_err(|error| error.to_string())?;
    for (path, text) in generated.files {
        let path = directory.join(path);
        fs::create_dir_all(path.parent().ok_or("generated file has no parent")?)
            .map_err(|error| error.to_string())?;
        fs::write(path, text).map_err(|error| error.to_string())?;
    }
    let compiler = Command::new("rustc")
        .args(["--version", "--verbose"])
        .output()
        .map_err(|error| format!("cannot query rustc: {error}"))?;
    if !compiler.status.success() {
        return Err("rustc version query failed".into());
    }
    let build = Command::new("cargo")
        .args([
            "build",
            "--offline",
            "--release",
            "-p",
            "emath_compiled_search",
        ])
        .current_dir(&directory)
        .env("CARGO_TARGET_DIR", directory.join("target"))
        .env("CARGO_PROFILE_RELEASE_OPT_LEVEL", "3")
        .env("CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS", "true")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()
        .map_err(|error| format!("cannot start cargo: {error}"))?;
    let log = directory.join("build.log");
    let mut log_bytes = build.stdout;
    log_bytes.extend_from_slice(&build.stderr);
    fs::write(&log, log_bytes).map_err(|error| error.to_string())?;
    if !build.status.success() {
        return Err(format!(
            "E-SEARCH-BUILD: generated build failed; {}",
            log.display()
        ));
    }
    let binary = directory.join("target/release").join(format!(
        "emath_compiled_search{}",
        std::env::consts::EXE_SUFFIX
    ));
    let binary_bytes = fs::read(&binary).map_err(|error| error.to_string())?;
    let binary_id = format!("fnv1a64:{:016x}", emath_core::fnv1a64_bytes(&binary_bytes));
    let mut entries = Vec::new();
    let mut best: Option<(f64, String)> = None;
    for (index, name) in names.iter().enumerate() {
        let output = Command::new(&binary)
            .arg(index.to_string())
            .output()
            .map_err(|error| format!("cannot execute {name}: {error}"))?;
        let mut entry = JsonWriter::object();
        entry.string("function", name);
        entry.string("role", if index == 0 { "baseline" } else { "candidate" });
        entry.bool("accepted", output.status.success());
        if !output.status.success() {
            let reason = String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(4096)
                .collect::<String>();
            if index == 0 {
                return Err(format!(
                    "E-SEARCH-BASELINE: compiled baseline failed reference agreement: {reason}"
                ));
            }
            entry.string("rejection", &reason);
        } else {
            let samples = String::from_utf8(output.stdout)
                .map_err(|error| error.to_string())?
                .lines()
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("invalid native timing output: {error}"))?;
            if samples.len() != request.repetitions {
                return Err("native runner did not return every requested timing sample".into());
            }
            let measurement = Measurement {
                metric_id: (*name).clone(),
                kind: MeasurementKind::LatencyNs,
                unit: "ns".into(),
                samples,
            };
            let summary = measurement.summarize().map_err(|error| error.to_string())?;
            entry.strings(
                "samples_ns",
                &measurement
                    .samples
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>(),
            );
            entry.string("median_ns", &summary.median.to_string());
            entry.string("cv_percent", &summary.cv_pct.to_string());
            entry.bool("quarantined", summary.quarantined());
            if !summary.quarantined()
                && best
                    .as_ref()
                    .is_none_or(|(median, _)| summary.median < *median)
            {
                best = Some((summary.median, (*name).clone()));
            }
        }
        entries.push(entry.finish());
    }
    let path = directory.join("search.json");
    let mut report = JsonWriter::object();
    report.string("schema", "emath.compiled-search.v1");
    report.string("operation_status", "completed");
    report.bool("search_complete", true);
    report.bool("goal_met", goal_met);
    report.string("target_id", &target_id);
    report.string("meaning_id", &meaning);
    report.string("language_id", &language);
    report.string("baseline", &request.baseline);
    report.object_field("inputs", bindings.trim());
    report.object_field("reference_result", expected.json().trim());
    report.string("equivalence_scope", "generated outputs match the checked reference on the fixed inputs; not universal equivalence");
    report.string("measurement_scope", "native function execution and result construction; excludes build, input construction, process startup, comparison, and output destruction");
    report.string("selection_scope", "lowest median among accepted nonquarantined entries; ties retain source order; no universal optimum or certified speedup");
    if let Some((_, name)) = best {
        report.string("selected", &name);
    } else {
        report.string("selection_status", "no-stable-measurement");
    }
    report.string("engine", "compiled-rust-release");
    report.string("os", std::env::consts::OS);
    report.string("arch", std::env::consts::ARCH);
    report.string("cpu", &cpu_name());
    report.string(
        "logical_cpus",
        &std::thread::available_parallelism()
            .map_err(|error| error.to_string())?
            .get()
            .to_string(),
    );
    report.string("compiler", &String::from_utf8_lossy(&compiler.stdout));
    report.string(
        "build_command",
        "cargo build --offline --release -p emath_compiled_search",
    );
    report.string(
        "build_profile",
        "release;opt-level=3;overflow-checks=true;RUSTFLAGS and CARGO_ENCODED_RUSTFLAGS removed",
    );
    report.string("binary", &binary.display().to_string());
    report.string("binary_id", &binary_id);
    report.string("build_log", &log.display().to_string());
    report.string("report", &path.display().to_string());
    report.objects("candidates", &entries);
    let report = report.finish();
    fs::write(&path, &report).map_err(|error| error.to_string())?;
    Ok((report, path, goal_met))
}

fn cpu_name() -> String {
    if cfg!(target_os = "macos") {
        return Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap_or_else(|| "unavailable".into());
    }
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|line| line.split_once(':'))
                    .map(|(_, name)| name.trim().to_string())
            })
        })
        .unwrap_or_else(|| "unavailable".into())
}

fn literal(value: &Value, ty: &Ty) -> Result<String, String> {
    Ok(match value {
        Value::I64(value) => format!("{value}i64"),
        Value::F64(value) => format!("f64::from_bits({}u64)", value.to_bits()),
        Value::Bool(value) => value.to_string(),
        Value::Vector(values) if render_ty(ty) == "Vec<i64>" => format!("vec![{}]", values.iter().map(|value| format!("{}i64", *value as i64)).collect::<Vec<_>>().join(",")),
        Value::Vector(values) => format!("vec![{}]", values.iter().map(|value| format!("f64::from_bits({}u64)", value.to_bits())).collect::<Vec<_>>().join(",")),
        Value::BigInt(value) => format!("emath_rt::UBig::parse_decimal({:?}).expect(\"validated integer\")", value.to_decimal()),
        _ => return Err("this input carrier has no compiled command-line literal; express it in the source function".into()),
    })
}

/// The expression denotes a reference. Collection lengths guard every index.
fn predicate(value: &Value, expression: &str) -> Result<String, String> {
    Ok(match value {
        Value::I64(value) => format!("*({expression}) == {value}i64"),
        Value::F64(value) => format!("({expression}).to_bits() == {}u64", value.to_bits()),
        Value::Bool(value) => format!("*({expression}) == {value}"),
        Value::Text(value) => format!(
            "({expression}).as_str() == {}",
            render_expr(&Expr::Str(value.clone()))
        ),
        Value::Rat { num, den } => {
            format!("({expression}).0 == {num}i128 && ({expression}).1 == {den}i128")
        }
        Value::BigInt(value) => format!("({expression}).limbs() == &{:?}", value.limbs()),
        Value::List(values) => sequence_predicate(values.iter(), values.len(), expression)?,
        Value::Vector(values) => {
            let values = values.iter().copied().map(Value::F64).collect::<Vec<_>>();
            sequence_predicate(values.iter(), values.len(), expression)?
        }
        Value::Matrix { rows, cols, data } => {
            let values = (0..*rows)
                .map(|row| {
                    Value::List(
                        data[row * cols..(row + 1) * cols]
                            .iter()
                            .copied()
                            .map(Value::F64)
                            .collect(),
                    )
                })
                .collect::<Vec<_>>();
            sequence_predicate(values.iter(), values.len(), expression)?
        }
        Value::Record { fields, .. } => fields
            .iter()
            .map(|(name, value)| {
                predicate(value, &format!("&({expression}).{}", escape_ident(name)))
                    .map(|part| format!("({part})"))
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(" && "),
        Value::Complex { re, im } => format!(
            "({expression}).0.to_bits() == {}u64 && ({expression}).1.to_bits() == {}u64",
            re.to_bits(),
            im.to_bits()
        ),
        Value::Option(Some(value)) => format!(
            "match {expression} {{ Some(__item) => {}, None => false }}",
            predicate(value, "__item")?
        ),
        Value::Option(None) => format!("({expression}).is_none()"),
        Value::Result { ok, payload } => {
            let (yes, no) = if *ok { ("Ok", "Err") } else { ("Err", "Ok") };
            format!(
                "match {expression} {{ {yes}(__item) => {}, {no}(_) => false }}",
                predicate(payload, "__item")?
            )
        }
        _ => return Err("the baseline result carrier has no compiled comparison contract".into()),
    })
}

fn sequence_predicate<'a>(
    values: impl Iterator<Item = &'a Value>,
    len: usize,
    expression: &str,
) -> Result<String, String> {
    let mut parts = vec![format!("({expression}).len() == {len}")];
    for (index, value) in values.enumerate() {
        parts.push(format!(
            "({})",
            predicate(value, &format!("&({expression})[{index}]"))?
        ));
    }
    Ok(parts.join(" && "))
}

fn harness(
    functions: &[&FnDef],
    inputs: &[&Value],
    expected: &Value,
    repetitions: usize,
) -> Result<String, String> {
    let mut source = format!(
        "use emath_compiled_search::*;\nfn matches_reference(value: &{}) -> bool {{ {} }}\nfn main() {{ let choice = std::env::args().nth(1).expect(\"candidate index\"); match choice.as_str() {{\n",
        render_ty(result_ty(&functions[0].ret)),
        predicate(expected, "value")?
    );
    for (index, function) in functions.iter().enumerate() {
        if function.params.len() != inputs.len() {
            return Err("generated input arity differs from the source declaration".into());
        }
        source.push_str(&format!(
            "\"{index}\" => {{ for repetition in 0..={repetitions} {{\n"
        ));
        let mut arguments = Vec::new();
        for (position, (input, param)) in inputs.iter().zip(&function.params).enumerate() {
            source.push_str(&format!(
                "let arg_{position}: {} = {};\n",
                render_ty(&param.ty),
                literal(input, &param.ty)?
            ));
            arguments.push(format!("std::hint::black_box(arg_{position})"));
        }
        let call = format!("{}({})", escape_ident(&function.name), arguments.join(","));
        let call = if matches!(function.ret, Ty::Result { .. }) {
            call
        } else {
            format!("Ok::<_,String>({call})")
        };
        source.push_str(&format!("let attempt = std::panic::catch_unwind(|| {{ let start = std::time::Instant::now(); let result = std::hint::black_box({call}); (result, start.elapsed().as_nanos()) }});\n"));
        source.push_str("match attempt { Ok((Ok(value), elapsed)) => { if !matches_reference(&value) { eprintln!(\"E-SEARCH-COUNTEREXAMPLE: compiled output differs from the checked reference\"); std::process::exit(1); } if repetition != 0 { println!(\"{elapsed}\"); } }, Ok((Err(error), _)) => { eprintln!(\"E-SEARCH-CANDIDATE-REFUSED: {error:?}\"); std::process::exit(1); }, Err(_) => { eprintln!(\"E-SEARCH-CANDIDATE-PANIC\"); std::process::exit(1); } }\n} },\n");
    }
    source.push_str("_ => std::process::exit(2), } }\n");
    Ok(source)
}
