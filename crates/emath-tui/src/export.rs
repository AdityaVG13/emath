//! Native epoch export (bead emath-8k3zw).
//!
//! `export_native` emits two crates and builds one binary:
//!
//! - the **artifact crate**: `emath_build::build_file` over the host's
//!   module - the self-contained, std-only, typed-closure-ABI crate
//!   `emath build` produces, verified (it must compile and pass its
//!   emitted tests before anything drives it);
//! - the **epoch host crate**: a sibling bin crate over the artifact
//!   ABI (path dependency, `cargo build --release`, persistent
//!   incremental target dir - the compiled-probe doctrine). The host
//!   bin seeds with `Seed<Name>(0)`, steps batches with
//!   `Step<Name>(state, budget)`, projects the ledger, and writes an
//!   `emath.scratch.v1` checkpoint.
//!
//! The sibling is standalone std-only, so it MIRRORS the scratch
//! interchange (envelope, value rendering, ledger entries) instead of
//! linking emath-exec-ir - the compiled-probe doctrine: parity is
//! enforced by tests, not by sharing code. Cross-lane parity is the
//! bead's acceptance: for the same module, surface, and budget
//! schedule, the native lane's checkpoint is byte-identical to the VM
//! lane's. The mirror's record fields render alphabetically (the CValue
//! record carrier is a `BTreeMap`), sequences render comma-space,
//! records render `{"record": {"type": ...,"fields": {...}}}` with
//! comma-joined fields - exactly `constructor_layer::scratch`'s writer
//! shapes.
//!
//! The state shape is parsed from the EMITTED artifact (the emitted
//! Rust is the native lane's truth; no second emath-type-to-Rust
//! mapping table exists here). Loop-state contract fields are checked
//! at generation time; a state the epoch host cannot drive refuses
//! `loop_export_state` by name. The export also re-admits the module
//! on disk and refuses `loop_export_emit` if it no longer mints the
//! session's meaning id: the VM lane steps the open-time admission,
//! and an export over edited bytes would pair a stale meaning id with
//! new math.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::host::{admitted_meaning_id, HostFault, LoopHost};
use emath_build::{generated_crate_target_dir, run_cargo_timed};
use emath_rust_backend::constructor_crate::{
    emit_constructor_crate, ConstructorEmitRefusal,
};
use emath_rust_backend::rust_ir::ast::escape_ident;
use emath_rust_backend::AuthoredRecord;

/// What an export produced.
#[derive(Clone, Debug)]
pub struct ExportReport {
    /// The emitted artifact crate (the math, byte-identical to
    /// `emath build`'s emission by construction).
    pub artifact_dir: PathBuf,
    /// The epoch host crate (sibling of the artifact).
    pub host_dir: PathBuf,
    /// The compiled release binary.
    pub binary_path: PathBuf,
}

/// The Rust types the epoch host can mirror into scratch cargo.
#[derive(Clone, Debug, PartialEq, Eq)]
enum MirrorType {
    I64,
    Bool,
    Ratio,
    Seq(Box<MirrorType>),
    Record(String),
}

/// One emitted `EmathRecord_*` struct, with both name vocabularies:
/// the authored (emath) names are the scratch JSON's names and sort
/// keys (the VM's record carrier is a BTreeMap keyed by them); the
/// emitted Rust names are the field-access identifiers.
#[derive(Clone, Debug)]
struct StructShape {
    /// The authored record name (`LoopState`).
    name: String,
    /// The emitted Rust record name (`LoopState`; keywords escaped).
    rust_name: String,
    /// (authored field name, emitted Rust field name, carrier type),
    /// in declaration order.
    fields: Vec<(String, String, MirrorType)>,
}

impl StructShape {
    fn field(&self, name: &str) -> Option<&MirrorType> {
        self.fields
            .iter()
            .find(|(key, _, _)| key == name)
            .map(|(_, _, ty)| ty)
    }
}

/// Classify an emitted Rust field type; `None` is a type the mirror
/// cannot carry (tuples, closures, floats, anything exotic).
fn parse_rust_type(text: &str) -> Option<MirrorType> {
    let text = text.trim();
    if text == "i64" {
        return Some(MirrorType::I64);
    }
    if text == "bool" {
        return Some(MirrorType::Bool);
    }
    if text == "emath_rt::ExactRatio" {
        return Some(MirrorType::Ratio);
    }
    if let Some(inner) = text
        .strip_prefix("Vec<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        return parse_rust_type(inner).map(|ty| MirrorType::Seq(Box::new(ty)));
    }
    if let Some(name) = text.strip_prefix("EmathRecord_") {
        return Some(MirrorType::Record(name.to_string()));
    }
    None
}

/// The exact name bridge between the emission's authored records and
/// the emitted Rust: record names and field names both directions.
/// The backend's keyword escape (`move` -> `move_`) is not injective
/// (an authored `move_` field escapes to the same Rust name), so the
/// bridge is built from the emission's OWN authored list - never by
/// heuristic.
#[derive(Clone, Debug, Default)]
struct NameMaps {
    /// emitted Rust record name -> authored record name.
    records: BTreeMap<String, String>,
    /// emitted Rust record name -> (emitted Rust field -> authored
    /// field).
    fields: BTreeMap<String, BTreeMap<String, String>>,
}

fn name_maps(records: &[AuthoredRecord]) -> NameMaps {
    let mut maps = NameMaps::default();
    for record in records {
        let rust_record = escape_ident(&record.name);
        maps.records.insert(rust_record.clone(), record.name.clone());
        let entry = maps.fields.entry(rust_record).or_default();
        for (field, _) in &record.fields {
            entry.insert(escape_ident(field), field.clone());
        }
    }
    maps
}

/// Parse one `pub struct EmathRecord_<rust_name> { pub field: Type,
/// ... }` block out of the emitted lib.rs, resolving every field to
/// its authored name through `maps`. A field without an authored pair
/// is a generation-time refusal, never a guess.
fn parse_struct(
    lib: &str,
    rust_name: &str,
    maps: &NameMaps,
) -> Result<StructShape, String> {
    let marker = format!("pub struct EmathRecord_{rust_name} {{");
    let start = lib
        .find(&marker)
        .ok_or_else(|| format!("the emitted artifact does not declare `EmathRecord_{rust_name}`"))?;
    let body = &lib[start + marker.len()..];
    let end = body.find('}').ok_or_else(|| {
        format!("`EmathRecord_{rust_name}` has no closing brace in the emitted artifact")
    })?;
    let emath_name = maps.records.get(rust_name).ok_or_else(|| {
        format!(
            "the emitted record `{rust_name}` has no authored name in the emission's record list"
        )
    })?;
    let field_map = maps
        .fields
        .get(rust_name)
        .ok_or_else(|| format!("the emitted record `{rust_name}` has no authored field list"))?;
    let mut fields = Vec::new();
    for line in body[..end].lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub ") else {
            continue;
        };
        let Some((rust_field, ty)) = rest.split_once(": ") else {
            return Err(format!(
                "`EmathRecord_{rust_name}` field declaration `{line}` is not `pub name: Type,`"
            ));
        };
        let ty = parse_rust_type(ty.trim_end_matches(',')).ok_or_else(|| {
            format!(
                "`EmathRecord_{rust_name}` field `{rust_field}` has type `{ty}`, which is not scratch cargo (i64, bool, ExactRatio, Vec<T>, nested EmathRecord_* only)"
            )
        })?;
        let emath_field = field_map.get(rust_field).ok_or_else(|| {
            format!(
                "`EmathRecord_{rust_name}` field `{rust_field}` has no authored name in the emission's record list"
            )
        })?;
        fields.push((emath_field.clone(), rust_field.to_string(), ty));
    }
    Ok(StructShape {
        name: emath_name.clone(),
        rust_name: rust_name.to_string(),
        fields,
    })
}

/// Parse the state struct and every nested record, transitively,
/// starting from the authored state type name.
fn state_shapes(
    lib: &str,
    state_type: &str,
    records: &[AuthoredRecord],
) -> Result<Vec<StructShape>, String> {
    let maps = name_maps(records);
    let mut shapes = Vec::new();
    let mut queue = vec![escape_ident(state_type)];
    let mut seen = BTreeSet::new();
    while let Some(rust_name) = queue.pop() {
        if !seen.insert(rust_name.clone()) {
            continue;
        }
        let shape = parse_struct(lib, &rust_name, &maps)?;
        for (_, _, ty) in &shape.fields {
            collect_records(ty, &mut queue);
        }
        shapes.push(shape);
    }
    Ok(shapes)
}

fn collect_records(ty: &MirrorType, queue: &mut Vec<String>) {
    match ty {
        MirrorType::Record(name) => queue.push(name.clone()),
        MirrorType::Seq(inner) => collect_records(inner, queue),
        _ => {}
    }
}

/// The loop-state contract, checked at generation time: the fields the
/// ledger projection and the transcript read. Returns the element
/// record name (the archive's item type).
fn check_state_contract(state_type: &str, shapes: &[StructShape]) -> Result<String, String> {
    let shape = shapes
        .iter()
        .find(|s| s.name == state_type)
        .ok_or_else(|| format!("the state shape for `{state_type}` is missing"))?;
    let refuse = |what: &str| format!("the state record `{state_type}` {what}");
    let MirrorType::Seq(element) = shape
        .field("archive")
        .ok_or_else(|| refuse("has no `archive` field"))?
    else {
        return Err(refuse("`archive` is not a sequence"));
    };
    let MirrorType::Record(element_name) = element.as_ref() else {
        return Err(refuse("`archive` does not carry record elements"));
    };
    let element_shape = shapes
        .iter()
        .find(|s| &s.name == element_name)
        .ok_or_else(|| refuse("`archive` element record is missing from the shape"))?;
    if element_shape.field("accepted") != Some(&MirrorType::Bool) {
        return Err(format!(
            "the archive element record `{element_name}` has no `accepted` field of type bool"
        ));
    }
    if element_shape.field("key") != Some(&MirrorType::I64) {
        return Err(format!(
            "the archive element record `{element_name}` has no `key` field of type i64"
        ));
    }
    if element_shape.field("score") != Some(&MirrorType::Ratio) {
        return Err(format!(
            "the archive element record `{element_name}` has no `score` field of type ExactRatio"
        ));
    }
    for field in ["incumbent", "batch", "used", "verdict", "mode"] {
        if shape.field(field) != Some(&MirrorType::I64) {
            return Err(refuse(&format!("has no `{field}` field of type i64")));
        }
    }
    let MirrorType::Record(case_set) = shape
        .field("case_set")
        .ok_or_else(|| refuse("has no `case_set` field"))?
    else {
        return Err(refuse("`case_set` is not a record"));
    };
    let case_shape = shapes
        .iter()
        .find(|s| &s.name == case_set)
        .ok_or_else(|| refuse("`case_set` record is missing from the shape"))?;
    if case_shape.field("ids") != Some(&MirrorType::Seq(Box::new(MirrorType::I64))) {
        return Err(refuse("`case_set.ids` is not a sequence of i64"));
    }
    Ok(element_name.clone())
}

/// Export the native epoch host for `host`'s session surface into
/// `out_dir` and build the binary.
///
/// # Errors
/// `loop_export_emit` (artifact emission refused or not runnable),
/// `loop_export_state` (the state shape is not epoch-host cargo),
/// `loop_export_compile` (the host crate did not build).
pub fn export_native(host: &LoopHost, out_dir: &Path) -> Result<ExportReport, HostFault> {
    // One emitter, two callers: `emath build` and this export share
    // constructor_crate, so the exported artifact is byte-identical to
    // the CLI's emission by construction.
    let source = std::fs::read_to_string(host.module_path()).map_err(|error| {
        HostFault::fault(
            "loop_export_emit",
            format!(
                "cannot read the module {}: {error}",
                host.module_path().display()
            ),
        )
    })?;
    let (tree, parse) = emath_syntax::parse_str(&source);
    if parse.has_errors() {
        let first = parse
            .errors()
            .map(|error| error.message.clone())
            .next()
            .unwrap_or_else(|| "parse refused".to_string());
        return Err(HostFault::fault(
            "loop_export_emit",
            format!("the module does not parse: {first}"),
        ));
    }
    // The export pairs with THIS session: the VM lane steps the
    // open-time admission, so the emitted artifact must come from the
    // same bytes that minted the host's meaning id. A module edited
    // after the session opened refuses by name - never a stale
    // meaning id over new math.
    let meaning_now = admitted_meaning_id(host.module_path(), &source)?;
    if meaning_now != host.identity().meaning_id {
        return Err(HostFault::fault(
            "loop_export_emit",
            format!(
                "the module {} changed since the session opened (meaning id {} is not the session's {}): re-open the session before exporting",
                host.module_path().display(),
                meaning_now,
                host.identity().meaning_id
            ),
        ));
    }
    let emission = emit_constructor_crate(&tree, host.module_path()).map_err(
        |refusal: ConstructorEmitRefusal| match refusal {
            ConstructorEmitRefusal::NotConstructor(message) => HostFault::fault(
                "loop_export_emit",
                format!("artifact emission refused: {message}"),
            ),
            ConstructorEmitRefusal::ImportsRefused { e_code, detail } => HostFault::fault(
                "loop_export_emit",
                format!("artifact emission refused ({e_code}): {detail}"),
            ),
        },
    )?;
    if !emission.runnable {
        return Err(HostFault::fault(
            "loop_export_emit",
            format!(
                "the module is not runnable natively: {}",
                emission.unresolved.join("; ")
            ),
        ));
    }
    let artifact_dir = out_dir.join("artifact");
    std::fs::create_dir_all(artifact_dir.join("src")).map_err(|error| {
        HostFault::fault(
            "loop_export_compile",
            format!(
                "cannot create {}: {error}",
                artifact_dir.join("src").display()
            ),
        )
    })?;
    std::fs::write(artifact_dir.join("src/lib.rs"), &emission.lib).map_err(|error| {
        HostFault::fault(
            "loop_export_compile",
            format!(
                "cannot write {}: {error}",
                artifact_dir.join("src/lib.rs").display()
            ),
        )
    })?;
    std::fs::write(artifact_dir.join("Cargo.toml"), &emission.manifest).map_err(|error| {
        HostFault::fault(
            "loop_export_compile",
            format!(
                "cannot write {}: {error}",
                artifact_dir.join("Cargo.toml").display()
            ),
        )
    })?;
    let lib = emission.lib.clone();
    let crate_name = emission.package_name.clone();
    let state_type = host.surface().state_type.clone();
    let shapes = state_shapes(&lib, &state_type, &emission.records).map_err(|detail| {
        HostFault::fault("loop_export_state", format!("state shape refused: {detail}"))
    })?;
    // The contract check is the gate; the element name is inside it.
    check_state_contract(&state_type, &shapes).map_err(|detail| {
        HostFault::fault("loop_export_state", format!("state contract refused: {detail}"))
    })?;

    let host_dir = out_dir.join("epoch-host");
    let src = host_dir.join("src");
    std::fs::create_dir_all(&src).map_err(|error| {
        HostFault::fault(
            "loop_export_compile",
            format!("cannot create {}: {error}", src.display()),
        )
    })?;
    let crate_ident = crate_name.replace('-', "_");
    let package = format!(
        r#"# Generated by emath (deterministic; do not edit). Native
# epoch host (bead emath-8k3zw) over the artifact ABI: a path
# sibling of the artifact crate, std-only, driving the session
# surface Seed/Step pair and mirroring emath.scratch.v1.
[package]
name = "epoch-host-{package}-{target}"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = "epoch-host"
path = "src/main.rs"

[dependencies]
{crate_ident} = {{ path = {artifact_dir:?} }}
"#,
        package = crate_name.to_lowercase(),
        target = host.surface().step.to_lowercase(),
        artifact_dir = artifact_dir.canonicalize().unwrap_or(artifact_dir.clone()),
    );
    std::fs::write(host_dir.join("Cargo.toml"), package).map_err(|error| {
        HostFault::fault(
            "loop_export_compile",
            format!("cannot write epoch-host Cargo.toml: {error}"),
        )
    })?;
    let main = generate_main(&crate_ident, host, &state_type, &shapes);
    std::fs::write(src.join("main.rs"), main).map_err(|error| {
        HostFault::fault(
            "loop_export_compile",
            format!("cannot write epoch-host main.rs: {error}"),
        )
    })?;

    let target = generated_crate_target_dir(&format!(
        "epoch-host-{}-{}",
        crate_name, host.surface().step
    ));
    let mut command = std::process::Command::new("cargo");
    command
        .arg("build")
        .arg("--release")
        .arg("--quiet")
        .current_dir(&host_dir)
        .env("CARGO_TARGET_DIR", &target);
    let output = run_cargo_timed(command, Duration::from_secs(600)).map_err(|error| {
        HostFault::fault(
            "loop_export_compile",
            format!("epoch-host cargo build: {error}"),
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(HostFault::fault(
            "loop_export_compile",
            format!("epoch-host cargo build failed: {stderr}"),
        ));
    }
    let binary_path = target.join("release").join("epoch-host");
    if !binary_path.is_file() {
        return Err(HostFault::fault(
            "loop_export_compile",
            format!(
                "epoch-host binary missing after build: {}",
                binary_path.display()
            ),
        ));
    }
    Ok(ExportReport {
        artifact_dir,
        host_dir,
        binary_path,
    })
}

/// Substitute `<Token>` placeholders in a template chunk. Tokens are
/// delimited by angle brackets to stay visible in review.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut text = template.to_string();
    for (token, value) in values {
        text = text.replace(&format!("<{token}>"), value);
    }
    text
}

/// Generate the standalone `main.rs` epoch host.
fn generate_main(
    crate_ident: &str,
    host: &LoopHost,
    state_type: &str,
    shapes: &[StructShape],
) -> String {
    let surface = host.surface();
    let identity = host.identity();
    // The state record's emitted Rust name (keywords escaped); the
    // transcript keeps the authored name, the type paths take this.
    let state_rust = shapes
        .iter()
        .find(|shape| shape.name == state_type)
        .map(|shape| shape.rust_name.clone())
        .unwrap_or_else(|| escape_ident(state_type));
    let mut text = String::new();

    text.push_str(&fill(
        r#"#![forbid(unsafe_code)]
//! Generated by emath (deterministic; do not edit).
//! Native epoch host (bead emath-8k3zw) over the artifact ABI for
//! `<Step>` on `<StateType>`: seeds with `<Seed>`(0), steps batches at
//! an explicit budget, projects the ledger, and writes an
//! `emath.scratch.v1` checkpoint byte-identical to the VM lane's for
//! the same schedule (cross-lane parity; enforced by tests, not by
//! shared code - the compiled-probe doctrine). The interchange mirror
//! follows constructor_layer::scratch's writer: records render
//! alphabetically (BTreeMap carrier), sequences comma-space, ledger
//! entries in field order.

const MEANING_ID: &str = "<MeaningId>";
const LANGUAGE_ID: &str = "<LanguageId>";
const TARGET: &str = "<Target>";
const STATE_TYPE: &str = "<StateType>";

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1);
}

fn verdict_name(verdict: i64) -> &'static str {
    match verdict {
        0 => "running",
        1 => "goal_attained",
        2 => "domain_exhausted",
        3 => "plateau",
        4 => "budget_exhausted",
        _ => "unknown",
    }
}

fn json_quote(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + 2);
    out.push('"');
    for ch in source.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn render_i64(out: &mut String, value: i64) {
    out.push_str(&value.to_string());
}

fn render_bool(out: &mut String, value: bool) {
    out.push_str(if value { "true" } else { "false" });
}

fn render_ratio(out: &mut String, value: <Crate>::emath_rt::ExactRatio) {
    out.push_str(&format!("{{\"rat\": [{}, {}]}}", value.0, value.1));
}

fn i64_list(items: &[i64]) -> String {
    let parts: Vec<String> = items.iter().map(|n| n.to_string()).collect();
    format!("[{}]", parts.join(", "))
}

"#,
        &[
            ("Step", &surface.step),
            ("Seed", &surface.seed),
            ("StateType", state_type),
            ("MeaningId", &identity.meaning_id),
            ("LanguageId", &identity.language_id),
            ("Target", &identity.target),
            ("Crate", crate_ident),
        ],
    ));

    // One render function per record shape. Names are exact from the
    // emission's authored list: the JSON type name and field names are
    // the AUTHORED names (the VM's record carrier is a BTreeMap keyed
    // by them, so ordering follows them too), while field ACCESS uses
    // the emitted Rust names.
    for shape in shapes {
        text.push_str(&format!(
            "fn render_record_{}(out: &mut String, v: &{}::EmathRecord_{}) {{\n",
            shape.rust_name, crate_ident, shape.rust_name
        ));
        text.push_str("    out.push_str(\"{\\\"record\\\": {\\\"type\\\": \");\n");
        text.push_str(&format!("    out.push_str(&json_quote(\"{}\"));\n", shape.name));
        text.push_str("    out.push_str(\",\\\"fields\\\": {\");\n");
        let mut sorted: Vec<&(String, String, MirrorType)> = shape.fields.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        for (index, (emath_field, rust_field, ty)) in sorted.iter().enumerate() {
            if index > 0 {
                text.push_str("    out.push(',');\n");
            }
            text.push_str(&format!(
                "    out.push_str(&json_quote(\"{emath_field}\"));\n    out.push_str(\": \");\n"
            ));
            push_render_call(&mut text, ty, &format!("v.{rust_field}"), 1);
        }
        text.push_str("    out.push_str(\"}}}\");\n}\n\n");
    }

    text.push_str(&fill(
        r#"struct Entry {
    batch: u64,
    verdict: i64,
    used: i64,
    incumbent_key: i64,
    score_num: i128,
    score_den: i128,
    archive_len: u64,
    promoted: Vec<i64>,
    quarantined: Vec<i64>,
    case_ids: Vec<i64>,
    halted: bool,
}

fn project(
    previous: &<Crate>::EmathRecord_<StateRust>,
    next: &<Crate>::EmathRecord_<StateRust>,
) -> Entry {
    if next.archive.len() < previous.archive.len() {
        fail("loop_state_contract: the archive shrank across a batch: archive ordinals are append-only");
    }
    let mut promoted: Vec<i64> = Vec::new();
    let mut quarantined: Vec<i64> = Vec::new();
    for (ordinal, record) in next.archive.iter().enumerate() {
        let was_accepted = match previous.archive.get(ordinal) {
            Some(prev) => prev.accepted,
            None => false,
        };
        if record.accepted && !was_accepted {
            promoted.push(record.key);
        }
        if !record.accepted && was_accepted {
            quarantined.push(record.key);
        }
    }
    let incumbent = usize::try_from(next.incumbent).unwrap_or_else(|_| {
        fail(&format!(
            "loop_state_contract: incumbent ordinal {} is not a valid archive index",
            next.incumbent
        ))
    });
    let incumbent_record = next.archive.get(incumbent).unwrap_or_else(|| {
        fail(&format!(
            "loop_state_contract: incumbent ordinal {incumbent} is outside the archive"
        ))
    });
    Entry {
        batch: next.batch as u64,
        verdict: next.verdict,
        used: next.used,
        incumbent_key: incumbent_record.key,
        score_num: i128::from(incumbent_record.score.0),
        score_den: i128::from(incumbent_record.score.1),
        archive_len: next.archive.len() as u64,
        promoted,
        quarantined,
        case_ids: next.case_set.ids.clone(),
        halted: next.verdict == 4,
    }
}

fn render_entry(entry: &Entry) -> String {
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"batch\": {},\n", entry.batch));
    out.push_str(&format!("  \"verdict\": {},\n", entry.verdict));
    out.push_str(&format!("  \"used\": {},\n", entry.used));
    out.push_str(&format!("  \"incumbent_key\": {},\n", entry.incumbent_key));
    out.push_str(&format!(
        "  \"incumbent_score\": {{\"rat\": [{}, {}]}},\n",
        entry.score_num, entry.score_den
    ));
    out.push_str(&format!("  \"archive_len\": {},\n", entry.archive_len));
    out.push_str(&format!("  \"promoted\": {},\n", i64_list(&entry.promoted)));
    out.push_str(&format!("  \"quarantined\": {},\n", i64_list(&entry.quarantined)));
    out.push_str(&format!("  \"case_ids\": {},\n", i64_list(&entry.case_ids)));
    out.push_str(&format!(
        "  \"halted\": {}",
        if entry.halted { "true" } else { "false" }
    ));
    out.push_str("\n}");
    out
}

fn write_scratch(
    path: &std::path::Path,
    revision: u64,
    state: &<Crate>::EmathRecord_<StateRust>,
    entries: &[Entry],
) {
    let mut state_json = String::new();
    render_record_<StateRust>(&mut state_json, state);
    let ledger_body = if entries.is_empty() {
        "[]".to_string()
    } else {
        let mut body = String::from("[\n");
        for (index, entry) in entries.iter().enumerate() {
            if index > 0 {
                body.push_str(",\n");
            }
            body.push_str(render_entry(entry).trim());
        }
        body.push_str("\n  ]");
        body
    };
    let mut out = String::from("{\n");
    out.push_str(&format!(
        "  \"schema_version\": {},\n",
        json_quote("emath.scratch.v1")
    ));
    out.push_str(&format!("  \"meaning_id\": {},\n", json_quote(MEANING_ID)));
    out.push_str(&format!("  \"language_id\": {},\n", json_quote(LANGUAGE_ID)));
    out.push_str(&format!("  \"target\": {},\n", json_quote(TARGET)));
    out.push_str(&format!("  \"revision\": {},\n", revision));
    out.push_str(&format!("  \"state\": {},\n", state_json));
    out.push_str(&format!("  \"ledger\": {}", ledger_body));
    out.push_str("\n}\n");
    if let Err(error) = std::fs::write(path, out) {
        fail(&format!(
            "scratch_io: cannot write scratch {}: {error}",
            path.display()
        ));
    }
}

fn incumbent_key(state: &<Crate>::EmathRecord_<StateRust>) -> i64 {
    let ordinal = usize::try_from(state.incumbent).unwrap_or_else(|_| {
        fail(&format!(
            "loop_state_contract: incumbent ordinal {} is not a valid archive index",
            state.incumbent
        ))
    });
    state
        .archive
        .get(ordinal)
        .unwrap_or_else(|| {
            fail(&format!(
                "loop_state_contract: incumbent ordinal {ordinal} is outside the archive"
            ))
        })
        .key
}

fn seed_line(state: &<Crate>::EmathRecord_<StateRust>) -> String {
    let key = incumbent_key(state);
    let ordinal = usize::try_from(state.incumbent).unwrap_or_else(|_| {
        fail("loop_state_contract: incumbent ordinal is not a valid archive index")
    });
    let record = state.archive.get(ordinal).unwrap_or_else(|| {
        fail(&format!(
            "loop_state_contract: incumbent ordinal {ordinal} is outside the archive"
        ))
    });
    format!(
        "seed batch {} verdict {} used {} incumbent key {} score {}/{} mode {}",
        state.batch,
        verdict_name(state.verdict),
        state.used,
        key,
        record.score.0,
        record.score.1,
        state.mode,
    )
}

fn batch_line(entry: &Entry) -> String {
    format!(
        "batch {} verdict {} used {} incumbent key {} score {}/{} promoted {} quarantined {} cases {} archive {}",
        entry.batch,
        verdict_name(entry.verdict),
        entry.used,
        entry.incumbent_key,
        entry.score_num,
        entry.score_den,
        i64_list(&entry.promoted),
        i64_list(&entry.quarantined),
        i64_list(&entry.case_ids),
        entry.archive_len,
    )
}

fn main() {
    let mut batches: Option<u32> = None;
    let mut budget: i64 = 60;
    let mut scratch: Option<std::path::PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--batches" => {
                let raw = args
                    .next()
                    .unwrap_or_else(|| fail("--batches requires a count"));
                batches = Some(
                    raw.parse()
                        .unwrap_or_else(|_| fail(&format!("`{raw}` is not a batch count"))),
                );
            }
            "--budget" => {
                let raw = args
                    .next()
                    .unwrap_or_else(|| fail("--budget requires a value"));
                budget = raw
                    .parse()
                    .unwrap_or_else(|_| fail(&format!("`{raw}` is not a budget")));
            }
            "--scratch" => {
                let raw = args
                    .next()
                    .unwrap_or_else(|| fail("--scratch requires a path"));
                scratch = Some(std::path::PathBuf::from(raw));
            }
            other => fail(&format!(
                "unknown argument `{other}`; the epoch host accepts --batches N --budget B --scratch PATH"
            )),
        }
    }
    let Some(batches) = batches else {
        fail("missing --batches N (the epoch count is explicit, never guessed)");
    };
    let Some(scratch) = scratch else {
        fail("missing --scratch PATH (the checkpoint destination is explicit, never guessed)");
    };
    println!("module <Crate>");
    println!("target {} state {}", TARGET, STATE_TYPE);
    println!("meaning {}", MEANING_ID);
    let started = std::time::Instant::now();
    let mut state = <Crate>::<Seed>(0).unwrap_or_else(|error| fail(&error));
    println!("{}", seed_line(&state));
    let mut entries: Vec<Entry> = Vec::new();
    for batch in 1..=batches {
        let previous = state.clone();
        state = <Crate>::<Step>(state, budget).unwrap_or_else(|error| fail(&error));
        if state.batch != i64::from(batch) {
            fail(&format!(
                "loop_ledger_law: the state's batch counter {} does not match the committed revision {batch}",
                state.batch
            ));
        }
        let entry = project(&previous, &state);
        println!("{}", batch_line(&entry));
        entries.push(entry);
    }
    let elapsed_ns = started.elapsed().as_nanos();
    println!(
        "end batches {} verdict {}",
        batches,
        verdict_name(state.verdict)
    );
    println!("timing native_step_total_ns {elapsed_ns} batches {batches}");
    write_scratch(&scratch, u64::from(batches), &state, &entries);
}
"#,
        &[
            ("Crate", crate_ident),
            ("StateType", state_type),
            ("StateRust", &state_rust),
            ("Seed", &surface.seed),
            ("Step", &surface.step),
        ],
    ));
    text
}

/// Emit the rendering code for one typed field access.
fn push_render_call(text: &mut String, ty: &MirrorType, expr: &str, depth: usize) {
    let pad = "    ".repeat(depth);
    match ty {
        MirrorType::I64 => {
            text.push_str(&format!("{pad}render_i64(out, {expr});\n"));
        }
        MirrorType::Bool => {
            text.push_str(&format!("{pad}render_bool(out, {expr});\n"));
        }
        MirrorType::Ratio => {
            text.push_str(&format!("{pad}render_ratio(out, {expr});\n"));
        }
        MirrorType::Seq(inner) => {
            text.push_str(&format!("{pad}out.push('[');\n"));
            text.push_str(&format!("{pad}for (index, item) in ({expr}).iter().enumerate() {{\n"));
            text.push_str(&format!("{pad}    if index > 0 {{ out.push_str(\", \"); }}\n"));
            // Sequence iteration yields references; scalar leaves
            // deref (`*item`), records coerce (`&&T` -> `&T`), and
            // nested sequences take the reference as-is.
            let inner_expr = match inner.as_ref() {
                MirrorType::I64 | MirrorType::Bool | MirrorType::Ratio => "*item",
                _ => "item",
            };
            push_render_call(text, inner, inner_expr, depth + 1);
            text.push_str(&format!("{pad}}}\n"));
            text.push_str(&format!("{pad}out.push(']');\n"));
        }
        MirrorType::Record(name) => {
            text.push_str(&format!("{pad}render_record_{name}(out, &{expr});\n"));
        }
    }
}
