//! Compiled function-spec probe (emath-bta82): a native binary carrying
//! the same `--set` CLI contract as `emath eval` — same strict parsing,
//! same value vocabulary, same receipt text shape. The interpreter stays
//! the reference semantics; the compiled path must match it exactly on
//! the parity battery workloads.
//!
//! The probe crate is a SIBLING of the published artifact (never inside
//! it): artifact identity covers only the staged file set, so adding a
//! probe must not perturb it. The shim re-implements (mirroring, with a
//! documented contract) `emath eval`'s strict parsing and value display
//! because the generated crate is standalone std-only — parity is
//! enforced by tests, not by sharing code.
//!
//! Receipt fields shipped: `meaning_id` (embedded constant computed at
//! build time via `package.meaning_id`) and `inputs_hash` (FNV-1a-64
//! over the sorted raw `--set` bindings, computed at runtime). `world`
//! and `method` are structurally absent for function-spec probes
//! (`emath eval` refuses `--world` on function files, E-EVAL-008), so
//! they print typed markers, never a fabricated value.

use super::*;

/// Input binding kind for the generated shim (mirrors `emath eval`'s
/// binding vocabulary: Float64, Int, Nat, Vector[Float64], Vector[Int],
/// Vector[Nat]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProbeKind {
    F64,
    I64,
    Nat,
    /// Stage-2 (emath-t63iz): exact big field element, bound as exact
    /// decimal and rendered via `to_decimal` (never an f64 round trip).
    BigInt,
    VecF64,
    VecInt,
    VecNat,
}

/// Classified one input field's declared type; `None` for a type the
/// probe contract cannot bind (the caller reports it typed).
pub(super) fn classify_input(
    package: &emath_ir::SemanticPackage,
    ty: emath_ir::TypeId,
) -> Option<ProbeKind> {
    match package.ty(ty)? {
        emath_ir::TypeNode::Float64 => Some(ProbeKind::F64),
        emath_ir::TypeNode::Int => Some(ProbeKind::I64),
        emath_ir::TypeNode::Nat => Some(ProbeKind::Nat),
        emath_ir::TypeNode::BigInt => Some(ProbeKind::BigInt),
        emath_ir::TypeNode::Vector { element, .. } => match &**element {
            emath_ir::TypeNode::Float64 => Some(ProbeKind::VecF64),
            emath_ir::TypeNode::Int => Some(ProbeKind::VecInt),
            emath_ir::TypeNode::Nat => Some(ProbeKind::VecNat),
            _ => None,
        },
        _ => None,
    }
}

/// Probe artifact: the compiled binary path.
#[derive(Clone, Debug)]
pub struct ProbeReport {
    pub binary_path: PathBuf,
}

/// Emit the probe crate as a sibling of the published artifact and
/// compile it with cargo (`--release`, persistent incremental target
/// dir). Returns the binary path.
pub fn emit_compiled_probe(
    package: &emath_ir::SemanticPackage,
    crate_name: &str,
    artifact_dir: &Path,
    entrypoint: &str,
    target_dir: &Path,
) -> Result<ProbeReport, BuildError> {
    let declaration = package
        .declarations
        .iter()
        .find(|d| d.name.leaf() == entrypoint)
        .ok_or_else(|| {
            BuildError::Backend(format!(
                "compiled probe: `{entrypoint}` is not a declared entrypoint of `{crate_name}`"
            ))
        })?;
    if declaration.outputs.len() != 1 {
        return Err(BuildError::Backend(format!(
            "compiled probe: `{entrypoint}` has {} declared outputs; the probe contract ships exactly one (typed refusal, not a half-shim)",
            declaration.outputs.len()
        )));
    }
    let mut inputs: Vec<(String, ProbeKind)> = Vec::new();
    for field in &declaration.inputs {
        let Some(kind) = classify_input(package, field.ty) else {
            return Err(BuildError::Backend(format!(
                "compiled probe: input `{}` has a type the probe contract cannot bind (Float64, Int, Nat, Vector[Float64], Vector[Int], Vector[Nat] only)",
                field.name
            )));
        };
        inputs.push((field.name.clone(), kind));
    }
    let output_kind = classify_input(package, declaration.outputs[0].ty).ok_or_else(|| {
        BuildError::Backend(format!(
            "compiled probe: output `{}` has a type the probe contract cannot render",
            declaration.outputs[0].name
        ))
    })?;
    let output_name = declaration.outputs[0].name.clone();
    let meaning_id = match package.meaning_id(&[]) {
        Ok(id) => id.to_string(),
        Err(error) => {
            return Err(BuildError::Backend(format!(
                "compiled probe: cannot derive meaning id for `{entrypoint}`: {error}"
            )));
        }
    };

    // Cargo resolves a path dependency (and CARGO_TARGET_DIR) relative
    // to the probe manifest's directory, so both must be absolute even
    // when the build ran with a relative --out.
    let cwd = std::env::current_dir()
        .map_err(|error| BuildError::Io(format!("cannot read cwd: {error}")))?;
    let absolutize = |path: &Path| -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        }
    };
    let artifact_dir = absolutize(artifact_dir);
    let target_dir = &absolutize(target_dir);

    let probe_dir = target_dir
        .join("probe")
        .join(format!("{crate_name}-{entrypoint}"));
    let src = probe_dir.join("src");
    std::fs::create_dir_all(&src)
        .map_err(|error| BuildError::Io(format!("cannot create {}: {error}", src.display())))?;

    let probe_crate = format!("probe-{}-{entrypoint}", crate_name.to_lowercase());
    let manifest = format!(
        "# Generated by emath (deterministic; do not edit). Compiled\n\
         # function-spec probe: same --set CLI contract as `emath eval`.\n\
         [package]\n\
         name = \"{probe_crate}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2024\"\n\
         publish = false\n\
         \n\
         [workspace]\n\
         \n\
         [[bin]]\n\
         name = \"{entrypoint}\"\n\
         path = \"src/main.rs\"\n\
         \n\
         [dependencies]\n\
         {crate_name} = {{ path = \"{}\" }}\n",
        artifact_dir.display()
    );
    std::fs::write(probe_dir.join("Cargo.toml"), manifest)
        .map_err(|error| BuildError::Io(format!("cannot write probe Cargo.toml: {error}")))?;
    let shim = shim_source(
        crate_name,
        entrypoint,
        &inputs,
        &output_name,
        output_kind,
        &meaning_id,
    );
    std::fs::write(src.join("main.rs"), shim)
        .map_err(|error| BuildError::Io(format!("cannot write probe main.rs: {error}")))?;

    // Persistent incremental target dir keyed by the probe crate (the
    // probe source dir itself is stable across rebuilds).
    let cargo_target = generated_crate_target_dir(&format!("probe-{crate_name}-{entrypoint}"));
    let mut command = std::process::Command::new("cargo");
    command
        .arg("build")
        .arg("--release")
        .arg("--quiet")
        .current_dir(&probe_dir)
        .env("CARGO_TARGET_DIR", &cargo_target);
    let output = run_cargo_timed(command, std::time::Duration::from_secs(300))
        .map_err(|error| BuildError::VerifyFailed(format!("probe cargo build: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(BuildError::VerifyFailed(format!(
            "probe cargo build failed: {stderr}"
        )));
    }
    let binary_path = cargo_target.join("release").join(entrypoint);
    if !binary_path.exists() {
        return Err(BuildError::VerifyFailed(format!(
            "probe binary missing after build: {}",
            binary_path.display()
        )));
    }
    Ok(ProbeReport { binary_path })
}

/// Generate the standalone `main.rs` shim.
fn shim_source(
    crate_name: &str,
    entrypoint: &str,
    inputs: &[(String, ProbeKind)],
    output_name: &str,
    output_kind: ProbeKind,
    meaning_id: &str,
) -> String {
    let mut text = String::new();
    text.push_str("#![forbid(unsafe_code)]\n");
    text.push_str("//! Generated by emath (deterministic; do not edit).\n");
    text.push_str(&format!(
        "//! Compiled function-spec probe for `{entrypoint}` — same\n"
    ));
    text.push_str("//! `--set` CLI contract as `emath eval` (strict scalar parsing;\n");
    text.push_str("//! whole elements for integer vectors; Nat non-negative). The\n");
    text.push_str("//! reference semantics stay with the interpreter; this binary\n");
    text.push_str("//! must match it exactly on the parity battery workloads.\n\n");
    helpers(&mut text, crate_name);
    text.push_str(&render_and_normalize(output_kind, crate_name));
    text.push_str("\nfn main() {\n");
    arg_loop(&mut text);
    // Undeclared names refuse (eval's E-EVAL-005 contract); they are
    // never silently dropped.
    let declared_list = inputs
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let declared_members = inputs
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    text.push_str(&format!(
        "    let declared: &[&str] = &[{declared_members}];\n"
    ));
    text.push_str("    for (name, _) in &raw_sets {\n");
    text.push_str("        if !declared.contains(&name.as_str()) {\n");
    text.push_str(&format!(
        "            fail(&format!(\"undeclared input `{{name}}`; declared inputs: {declared_list}\"));\n"
    ));
    text.push_str("        }\n");
    text.push_str("    }\n");
    // Bind each declared input by name, in declaration order.
    for (name, kind) in inputs {
        bind_input(&mut text, name, *kind, crate_name);
    }
    text.push_str("    let mut hash_input = raw_sets\n");
    text.push_str("        .iter()\n");
    text.push_str("        .map(|(name, raw)| format!(\"{name}={raw}\"))\n");
    text.push_str("        .collect::<Vec<_>>();\n");
    text.push_str("    hash_input.sort();\n");
    text.push_str("    let inputs_hash = fnv1a64(hash_input.join(\";\").as_bytes());\n");
    text.push_str("    println!(\"inputs_from set\");\n");
    text.push_str(&format!(
        "    println!(\"receipt engine=compiled-probe meaning_id={meaning_id} inputs_hash=fnv1a64:{{inputs_hash:016x}} world=not-applicable-to-function-probes method=not-applicable-to-function-probes\");\n"
    ));
    for (name, kind) in inputs {
        echo_input(&mut text, name, *kind);
    }
    let call_args = inputs
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    text.push_str(&format!(
        "    let value = {crate_name}::{entrypoint}({call_args}).normalize().unwrap_or_else(|error| fail(&error));\n"
    ));
    text.push_str(&format!(
        "    println!(\"output {output_name} = {{}}\", render(value));\n"
    ));
    text.push_str("}\n");
    text
}

/// Typed-failure helpers: every refusal is one actionable line on
/// stderr with a nonzero exit — no cascade, no silent coercion.
fn helpers(text: &mut String, crate_name: &str) {
    text.push_str("fn fail(message: &str) -> ! {\n    eprintln!(\"error: {message}\");\n    std::process::exit(1);\n}\n\n");
    text.push_str("/// Exact-integer parse (mirrors eval's scalar Int/Nat contract).\n");
    text.push_str("fn parse_i64(raw: &str, name: &str) -> i64 {\n    let value: f64 = raw.trim().parse().unwrap_or_else(|_| fail(&format!(\"cannot parse `--set {name}={raw}` as a value of the declared input type\")));\n    if value.fract() != 0.0 || value.abs() > 9.3e18 {\n        fail(&format!(\"`--set {name}={raw}` is not an exact integer\"));\n    }\n    value as i64\n}\n\n");
    text.push_str("/// Exact big-integer parse (mirrors the stage-2 BigInt binding:\n");
    text.push_str("/// non-negative decimal digits only, |F| < 2^256).\n");
    text.push_str(&format!(
        "fn parse_bigint(raw: &str, name: &str) -> {crate_name}::emath_rt::UBig {{\n    let trimmed = raw.trim();\n    if trimmed.starts_with('-') || trimmed.is_empty() || !trimmed.chars().all(|c| c.is_ascii_digit()) {{\n        fail(&format!(\"`--set {{name}}={{raw}}` is not a non-negative decimal integer\"));\n    }}\n    let value = {crate_name}::emath_rt::UBig::parse_decimal(trimmed)\n        .unwrap_or_else(|_| fail(&format!(\"cannot parse `--set {{name}}={{raw}}` as a big integer\")));\n    if value.bits() > {crate_name}::emath_rt::LIMIT_BITS {{\n        fail(&format!(\"`--set {{name}}={{raw}}` exceeds the stage-2 bound |F| < 2^256\"));\n    }}\n    value\n}}\n\n"
    ));
    text.push_str("/// Finite-decimal parse (mirrors eval's scalar Float64 contract).\n");
    text.push_str("fn parse_f64(raw: &str, name: &str) -> f64 {\n    let value: f64 = raw.trim().parse().unwrap_or_else(|_| fail(&format!(\"cannot parse `--set {name}={raw}` as a value of the declared input type\")));\n    if !value.is_finite() {\n        fail(&format!(\"`--set {name}={raw}` must be finite\"));\n    }\n    value\n}\n\n");
    text.push_str("/// Vector parse: `[a, b, c]`; `whole`/`non_negative` carry the\n");
    text.push_str("/// integer-element and Nat strictness of the declared element type.\n");
    text.push_str("fn parse_vec(raw: &str, name: &str, whole: bool, non_negative: bool) -> Vec<f64> {\n    let trimmed = raw.trim();\n    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {\n        fail(&format!(\"`--set {name}={raw}` must be a `[a, b, c]` vector\"));\n    }\n    let inner = trimmed[1..trimmed.len() - 1].trim();\n    if inner.is_empty() {\n        fail(&format!(\"`--set {name}={raw}` is an empty vector\"));\n    }\n    inner\n        .split(',')\n        .map(|part| {\n            let element: f64 = part.trim().parse().unwrap_or_else(|_| fail(&format!(\"cannot parse `--set {name}={raw}` element `{part}` as a finite decimal\")));\n            if !element.is_finite()\n                || (whole && (element.fract() != 0.0 || element.abs() > 9.3e18))\n                || (non_negative && element < 0.0)\n            {\n                fail(&format!(\"`--set {name}={raw}` element `{part}` does not match the declared input type\"));\n            }\n            element\n        })\n        .collect()\n}\n\n");
    text.push_str("/// FNV-1a-64 over the sorted raw bindings (inputs_hash receipt field).\n");
    text.push_str("fn fnv1a64(bytes: &[u8]) -> u64 {\n    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;\n    for byte in bytes {\n        hash ^= u64::from(*byte);\n        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);\n    }\n    hash\n}\n\n");
    text.push_str("/// f64 display mirror of the interpreter's `format_f64`: finite\n");
    text.push_str("/// values gain a trailing `.0` when they would look like integers;\n");
    text.push_str("/// non-finite render `NaN` / `Infinity` / `-Infinity`.\n");
    text.push_str("fn display_f64(value: f64) -> String {\n    if value.is_nan() { return \"NaN\".to_string(); }\n    if value.is_infinite() {\n        return if value.is_sign_positive() { \"Infinity\".to_string() } else { \"-Infinity\".to_string() };\n    }\n    let mut text = format!(\"{value}\");\n    if !text.contains('.') && !text.contains('e') && !text.contains('E') {\n        text.push_str(\".0\");\n    }\n    text\n}\n\n");
    text.push_str("/// Vector display mirror of `Value::Vector` (`[a, b, c]`).\n");
    text.push_str("fn display_vec(values: &[f64]) -> String {\n    let parts: Vec<String> = values.iter().map(|v| display_f64(*v)).collect();\n    format!(\"[{}]\", parts.join(\", \"))\n}\n\n");
}

/// The output renderer plus the `Normalize` impl pair (bare `T` and
/// `Result<T, String>` — indexing/slice bodies fault as `Err`).
fn render_and_normalize(kind: ProbeKind, crate_name: &str) -> String {
    let rust_type: String = match kind {
        ProbeKind::F64 => "f64".to_string(),
        ProbeKind::I64 | ProbeKind::Nat => "i64".to_string(),
        ProbeKind::BigInt => format!("{crate_name}::emath_rt::UBig"),
        ProbeKind::VecF64 | ProbeKind::VecInt | ProbeKind::VecNat => "Vec<f64>".to_string(),
    };
    let render_body = match kind {
        ProbeKind::F64 => "display_f64(value)".to_string(),
        ProbeKind::I64 | ProbeKind::Nat => "value.to_string()".to_string(),
        ProbeKind::BigInt => "value.to_decimal()".to_string(),
        ProbeKind::VecF64 | ProbeKind::VecInt | ProbeKind::VecNat => {
            "display_vec(&value)".to_string()
        }
    };
    let mut text = String::new();
    text.push_str(&format!(
        "fn render(value: {rust_type}) -> String {{ {render_body} }}\n\n"
    ));
    text.push_str(&format!(
        "trait Normalize {{ fn normalize(self) -> Result<{rust_type}, String>; }}\n"
    ));
    text.push_str(&format!(
        "impl Normalize for {rust_type} {{ fn normalize(self) -> Result<{rust_type}, String> {{ Ok(self) }} }}\n"
    ));
    text.push_str(&format!(
        "impl Normalize for Result<{rust_type}, String> {{ fn normalize(self) -> Result<{rust_type}, String> {{ self }} }}\n\n"
    ));
    text
}

/// The `--set` argument loop: `--set name=value` pairs only; duplicates
/// and unknown flags refuse typed.
fn arg_loop(text: &mut String) {
    text.push_str("    let mut raw_sets: Vec<(String, String)> = Vec::new();\n");
    text.push_str("    let mut args = std::env::args().skip(1);\n");
    text.push_str("    while let Some(arg) = args.next() {\n");
    text.push_str("        if arg == \"--set\" {\n");
    text.push_str("            let binding = args.next().unwrap_or_else(|| fail(\"--set requires a name=value binding\"));\n");
    text.push_str("            let Some((name, raw)) = binding.split_once('=') else {\n");
    text.push_str(
        "                fail(&format!(\"--set binding `{binding}` must be name=value\"));\n",
    );
    text.push_str("            };\n");
    text.push_str("            raw_sets.push((name.to_string(), raw.to_string()));\n");
    text.push_str("        } else {\n");
    text.push_str("            fail(&format!(\"unknown argument `{arg}`; the probe accepts --set name=value only\"));\n");
    text.push_str("        }\n");
    text.push_str("    }\n");
    text.push_str("    let mut seen = std::collections::BTreeSet::new();\n");
    text.push_str("    for (name, _) in &raw_sets {\n");
    text.push_str("        if !seen.insert(name.clone()) {\n");
    text.push_str(
        "            fail(&format!(\"duplicate `--set` binding for input `{name}`\"));\n",
    );
    text.push_str("        }\n");
    text.push_str("    }\n");
}

/// One typed binding for one declared input (missing = typed refusal).
fn bind_input(text: &mut String, name: &str, kind: ProbeKind, crate_name: &str) {
    let parse_expr = match kind {
        ProbeKind::F64 => format!("parse_f64(&raw, \"{name}\")"),
        ProbeKind::I64 => format!("parse_i64(&raw, \"{name}\")"),
        ProbeKind::Nat => format!(
            "{{ let value = parse_i64(&raw, \"{name}\"); if value < 0 {{ fail(&format!(\"`--set {name}={{}}` must be non-negative\", raw)); }} value }}"
        ),
        ProbeKind::BigInt => format!("parse_bigint(&raw, \"{name}\")"),
        ProbeKind::VecF64 => format!("parse_vec(&raw, \"{name}\", false, false)"),
        ProbeKind::VecInt => format!("parse_vec(&raw, \"{name}\", true, false)"),
        ProbeKind::VecNat => format!("parse_vec(&raw, \"{name}\", true, true)"),
    };
    let rust_type = match kind {
        ProbeKind::F64 => "f64".to_string(),
        ProbeKind::I64 | ProbeKind::Nat => "i64".to_string(),
        ProbeKind::BigInt => format!("{crate_name}::emath_rt::UBig"),
        ProbeKind::VecF64 | ProbeKind::VecInt | ProbeKind::VecNat => "Vec<f64>".to_string(),
    };
    text.push_str(&format!(
        "    let {name}: {rust_type} = match raw_sets.iter().find(|(key, _)| key == \"{name}\") {{\n"
    ));
    text.push_str(&format!("        Some((_, raw)) => {parse_expr},\n"));
    text.push_str(&format!(
        "        None => fail(\"missing input `{name}`; use --set {name}=<value>\"),\n"
    ));
    text.push_str("    };\n");
}

/// Input echo, mirroring `Value::Display` per binding kind.
fn echo_input(text: &mut String, name: &str, kind: ProbeKind) {
    match kind {
        ProbeKind::F64 => {
            text.push_str(&format!(
                "    println!(\"input {name} = {{}}\", display_f64({name}));\n"
            ));
        }
        ProbeKind::I64 | ProbeKind::Nat => {
            text.push_str(&format!("    println!(\"input {name} = {{}}\", {name});\n"));
        }
        ProbeKind::BigInt => {
            text.push_str(&format!(
                "    println!(\"input {name} = {{}}\", {name}.to_decimal());\n"
            ));
        }
        ProbeKind::VecF64 | ProbeKind::VecInt | ProbeKind::VecNat => {
            text.push_str(&format!(
                "    println!(\"input {name} = {{}}\", display_vec(&{name}));\n"
            ));
        }
    }
}
