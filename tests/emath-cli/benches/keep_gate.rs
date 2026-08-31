//! Keep-gate benchmark driver (gauntlet-06).
//!
//! Custom harness (`harness = false`) benchmark `comprehensive_bench`.
//! The driver owns the clock (`std::time::Instant`), runs warmup 2 /
//! min 3 / max 10 samples / ~5s target per family, summarizes samples
//! through `emath-lab-core` (with `cv_pct`; cells above 5% CV are
//! quarantined, never read as an honest baseline) and writes
//! `.bench-history/<family>.latest.json` plus a guard file.
//!
//! Claims use `cargo bench --profile release-perf --bench
//! comprehensive_bench`, never `--release`. No compiler algorithm is
//! changed here; the cells time the existing pipeline (no rustc of
//! generated crates inside a cell).

use emath_core::limits::Limits;
use emath_core::tree::Item;
use emath_lab_core as lab;
use emath_sema::session::CompilerSession;
use emath_syntax::parse_lossless;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const SCHEMA: &str = "emath.keep-gate";
const GUARD_SCHEMA: &str = "emath.keep-gate-guard";
/// Warmup runs discarded before sampling.
const WARMUP_RUNS: usize = 2;
/// Minimum samples before the time target can stop a family.
const MIN_SAMPLES: usize = 3;
/// Maximum samples per family.
const MAX_SAMPLES: usize = 10;
/// Per-family time target.
const TARGET_ELAPSED: Duration = Duration::from_secs(5);
/// History dir under the workspace root.
const HISTORY_REL: &str = ".bench-history";
/// Reference genesis source used by the codegen cells.
const GLYPHS_REL: &str = "tests/valid/arbitrary-glyphs.emath";
/// Committed generated golden for the identity comparison.
const SG_GENERATED_LIB_REL: &str = "examples/generated/semantic-genesis-worlds/src/lib.rs";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Locate the `emath` binary. The keep-gate bench moved into the
/// `tests/emath-cli` package, which has no `[[bin]]`, so Cargo no longer
/// defines `CARGO_BIN_EXE_emath` for it; resolve the workspace binary by
/// profile instead (env var first for direct/legacy invocations).
fn emath_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_emath") {
        return PathBuf::from(path);
    }
    let exe = if cfg!(windows) { "emath.exe" } else { "emath" };
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root().join("target"));
    for profile in ["release-perf", "release", "debug"] {
        let candidate = target.join(profile).join(exe);
        if candidate.exists() {
            return candidate;
        }
    }
    target.join("debug").join(exe)
}

/// One measured cell result.
struct Sample {
    /// Wall-clock nanoseconds of the cell run.
    ns: u64,
    /// Bytes hashed into `identity=` (emitted artifacts or inputs).
    identity_bytes: Vec<u8>,
    /// Identity provenance description.
    identity_label: String,
    /// Whether the identity matches a committed golden (where one exists).
    golden_match: Option<bool>,
    /// Human-readable cell description.
    detail: String,
}

struct FamilyConfig {
    smoke: bool,
}

impl FamilyConfig {
    fn warmup(&self) -> usize {
        if self.smoke { 1 } else { WARMUP_RUNS }
    }
    fn min_samples(&self) -> usize {
        if self.smoke { 2 } else { MIN_SAMPLES }
    }
    fn max_samples(&self) -> usize {
        if self.smoke { 2 } else { MAX_SAMPLES }
    }
}

/// Runs the sampling loop for one family cell.
fn collect_samples(
    config: &FamilyConfig,
    mut run: impl FnMut() -> Result<Sample, String>,
) -> Result<Vec<Sample>, String> {
    for _ in 0..config.warmup() {
        run()?;
    }
    let start = Instant::now();
    let mut samples = Vec::new();
    while samples.len() < config.max_samples() {
        samples.push(run()?);
        if samples.len() >= config.min_samples() && start.elapsed() >= TARGET_ELAPSED {
            break;
        }
    }
    Ok(samples)
}

/// Corpus `.emath` files under a directory, sorted for determinism.
fn emath_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot list {}: {error}", dir.display()))?
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "emath"))
        .map(|entry| entry.path())
        .collect();
    files.sort();
    Ok(files)
}

/// parse family: lossless parse of the `.emath` language corpus per
/// sample. The `language/examples` tree is excluded: the genesis glyph
/// files use the genesis syntax (`parse_genesis`), not the language parse.
fn cell_parse(root: &Path, config: &FamilyConfig) -> Result<Vec<Sample>, String> {
    let corpus = emath_files(&root.join("tests/valid"))?;
    let total = corpus.len();
    let cell = move || {
        let start = Instant::now();
        let mut identity = Vec::new();
        let mut statements = 0_usize;
        for file in &corpus {
            let bytes = std::fs::read(file)
                .map_err(|error| format!("cannot read {}: {error}", file.display()))?;
            identity.extend_from_slice(&bytes);
            let text = String::from_utf8_lossy(&bytes);
            let parsed = parse_lossless(&text, emath_core::FileId(0), &Limits::default());
            if parsed.diagnostics.has_errors() {
                return Err(format!("corpus file failed to parse: {}", file.display()));
            }
            for item in &parsed.tree.items {
                if let Item::Declaration(declaration) = item {
                    statements += declaration.body.len();
                }
            }
        }
        if statements == 0 {
            return Err("corpus parse produced no statements".into());
        }
        let ns = elapsed_ns(start);
        Ok(Sample {
            ns,
            identity_bytes: identity,
            identity_label: format!("concatenated bytes of {total} corpus .emath files"),
            golden_match: None,
            detail: format!("parse_lossless over {total} corpus files"),
        })
    };
    collect_samples(config, cell)
}

/// check family: admission check of the valid fixtures per sample.
fn cell_check(root: &Path, config: &FamilyConfig) -> Result<Vec<Sample>, String> {
    let fixtures = emath_files(&root.join("tests/valid"))?;
    let count = fixtures.len();
    let cell = move || {
        let start = Instant::now();
        let mut identity = Vec::new();
        let mut session = CompilerSession::new(Limits::default());
        for file in &fixtures {
            let path = file.display().to_string();
            let package = session
                .load_package(&path)
                .map_err(|error| format!("cannot load {}: {}", file.display(), error))?;
            identity.extend_from_slice(package.text.as_bytes());
            let result = session.check(package.file);
            if result.diagnostics.has_errors() {
                return Err(format!(
                    "check refused claimed-valid fixture: {}",
                    file.display()
                ));
            }
        }
        let ns = elapsed_ns(start);
        Ok(Sample {
            ns,
            identity_bytes: identity,
            identity_label: format!("concatenated source of {count} valid fixtures"),
            golden_match: None,
            detail: format!("CompilerSession check over {count} valid fixtures"),
        })
    };
    collect_samples(config, cell)
}

/// codegen-parametric family: full genesis analysis + in-memory codegen
/// (no rustc, no disk) of the glyph example per sample.
fn cell_codegen_parametric(root: &Path, config: &FamilyConfig) -> Result<Vec<Sample>, String> {
    let glyphs = root.join(GLYPHS_REL);
    let golden_lib = std::fs::read(root.join(SG_GENERATED_LIB_REL))
        .map_err(|error| format!("cannot read committed golden: {error}"))?;
    let golden_sha = lab::hex(&lab::digest(&golden_lib));
    let cell = move || {
        let start = Instant::now();
        let analysis = emath_cli::genesis_cmd::analyze(&glyphs)
            .map_err(|error| format!("genesis analyze refused: {error}"))?;
        let worlds = emath_cli::genesis_cmd::builtin_worlds(&analysis.inference.signature);
        // Portfolio witnesses (`one_point`, `csa_seeded`) have no lowering;
        // generate() refuses them with E-GEN-094. Time the compiled trio only.
        let specs = worlds
            .iter()
            .filter_map(|world| {
                let label = world.name.to_ascii_lowercase();
                if !matches!(
                    label.as_str(),
                    "free_symbolic" | "boolean_algebra" | "modular_numeric"
                ) {
                    return None;
                }
                let operators = world
                    .operators
                    .iter()
                    .filter_map(|operator| match &operator.semantics {
                        emath_world_ir::OperatorSemantics::DeclaredExpression(meaning) => {
                            Some((operator.symbol.0.clone(), meaning.clone()))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                Some(emath_world_codegen_rust::WorldSpec { label, operators })
            })
            .collect::<Vec<_>>();
        if specs.len() != 3 {
            return Err(format!(
                "expected 3 compiled worlds, got {}",
                specs.len()
            ));
        }
        let generated = emath_world_codegen_rust::generate(
            &analysis.term,
            &analysis.inference.signature,
            &specs,
        )
        .map_err(|refusal| format!("{}: {}", refusal.code, refusal.message))?;
        let lib_rs = generated
            .files
            .get("src/lib.rs")
            .ok_or_else(|| "generated crate has no src/lib.rs".to_string())?;
        let identity_bytes = lib_rs.as_bytes().to_vec();
        let generated_sha = lab::hex(&lab::digest(&identity_bytes));
        let ns = elapsed_ns(start);
        let sha_now = generated_sha.clone();
        let golden_now = golden_sha.clone();
        Ok(Sample {
            ns,
            identity_bytes,
            identity_label: "generated src/lib.rs bytes (in-memory, no rustc)".into(),
            golden_match: Some(sha_now == golden_now),
            detail: "analyze + builtin_worlds + generate for the glyph example".into(),
        })
    };
    collect_samples(config, cell)
}

/// artifact-json family: deterministic manifest + source-map JSON emit.
fn cell_artifact_json(config: &FamilyConfig) -> Result<Vec<Sample>, String> {
    let glyphs_rel = GLYPHS_REL.to_string();
    let cell = move || {
        let start = Instant::now();
        let mut object = emath_artifact::JsonWriter::object();
        object.string("schema", "emath.generated-crate-manifest");
        object.string("crate_name", "bench-cell");
        object.string("source", &glyphs_rel);
        object.strings(
            "worlds",
            &["free_symbolic".to_string(), "modular_numeric".to_string()],
        );
        object.object_field("files", "[\"Cargo.toml\",\"src/lib.rs\"]");
        let manifest = object.finish();
        let source_map = emath_artifact::write_generated_crate_source_map(
            &glyphs_rel,
            &["Cargo.toml".to_string(), "src/lib.rs".to_string()],
        );
        let mut identity = Vec::new();
        identity.extend_from_slice(manifest.as_bytes());
        identity.extend_from_slice(&[0xff]);
        identity.extend_from_slice(source_map.as_bytes());
        let ns = elapsed_ns(start);
        Ok(Sample {
            ns,
            identity_bytes: identity,
            identity_label: "generated-crate-manifest + source-map JSON bytes".into(),
            golden_match: None,
            detail: "emath_artifact JSON writers emit the genesis manifest + source map".into(),
        })
    };
    collect_samples(config, cell)
}

/// genesis-replay family: analyze the glyph example twice and pin the
/// deterministic replay ids inside the timed cell.
fn cell_genesis_replay(root: &Path, config: &FamilyConfig) -> Result<Vec<Sample>, String> {
    let glyphs = root.join(GLYPHS_REL);
    let cell = move || {
        let start = Instant::now();
        let first = emath_cli::genesis_cmd::analyze(&glyphs)
            .map_err(|error| format!("genesis analyze refused: {error}"))?;
        let second = emath_cli::genesis_cmd::analyze(&glyphs)
            .map_err(|error| format!("genesis replay refused: {error}"))?;
        if first.parse_id != second.parse_id
            || first.signature_id != second.signature_id
            || first.term_id != second.term_id
        {
            return Err(format!(
                "replay divergence: parse {} vs {}, signature {} vs {}, term {:016x} vs {:016x}",
                first.parse_id,
                second.parse_id,
                first.signature_id,
                second.signature_id,
                first.term_id,
                second.term_id
            ));
        }
        let identity = format!(
            "{}:{}:{:016x}",
            first.parse_id, first.signature_id, first.term_id
        )
        .into_bytes();
        let ns = elapsed_ns(start);
        Ok(Sample {
            ns,
            identity_bytes: identity,
            identity_label: "replay ids `parse:signature:term` (determinism pin)".into(),
            golden_match: None,
            detail: "analyze twice + equality pin (replay fidelity)".into(),
        })
    };
    collect_samples(config, cell)
}

/// cli8p family (MT8 analog): 8 parallel `emath check` subprocesses on
/// 8 disjoint files per sample. Real processes, not in-process MVCC.
fn cell_cli8p(root: &Path, config: &FamilyConfig) -> Result<Vec<Sample>, String> {
    let bin = emath_bin();
    let files = [
        "tests/valid/square.emath",
        "tests/valid/affine_scorer.emath",
        "language/examples/intro/hello-square.emath",
        "language/examples/intro/scratch.emath",
        "language/examples/intro/l1_guided.emath",
        "language/examples/intro/units.emath",
        "language/examples/intro/autodiff.emath",
        "language/examples/intro/solve.emath",
    ];
    let cell = move || {
        let start = Instant::now();
        let children = files
            .iter()
            .map(|file| {
                Command::new(&bin)
                    .arg("check")
                    .arg(root.join(file))
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .map_err(|error| format!("cannot spawn `emath check`: {error}"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        for mut child in children {
            let status = child
                .wait()
                .map_err(|error| format!("cannot wait for `emath check`: {error}"))?;
            // A refused fixture still measured the pipeline; only a
            // spawn/IO failure or signal is a cell error.
            if !status.success() && status.code().is_none() {
                return Err(format!("emath check was signaled: {status:?}"));
            }
        }
        let ns = elapsed_ns(start);
        let identity = files.join("|").into_bytes();
        Ok(Sample {
            ns,
            identity_bytes: identity,
            identity_label: "8 disjoint check file paths".into(),
            golden_match: None,
            detail: "8 parallel `emath check` processes (CLI8p)".into(),
        })
    };
    collect_samples(config, cell)
}

/// Summary of one entry as a lab JSON object. Integer fields are sample
/// counts/byte values that stay exact below 2^53, so the casts lose
/// nothing.
#[allow(clippy::cast_precision_loss)]
fn summary_object(summary: &lab::Summary) -> lab::json::JsonValue {
    use lab::json::JsonValue;
    JsonValue::Object(vec![
        ("count".into(), JsonValue::Number(summary.count as f64)),
        ("min".into(), JsonValue::Number(summary.min as f64)),
        ("max".into(), JsonValue::Number(summary.max as f64)),
        ("mean".into(), JsonValue::Number(summary.mean)),
        ("median".into(), JsonValue::Number(summary.median)),
        ("p90".into(), JsonValue::Number(summary.p90)),
        ("p99".into(), JsonValue::Number(summary.p99)),
        ("cv_pct".into(), JsonValue::Number(summary.cv_pct)),
        ("quarantined".into(), JsonValue::Bool(summary.quarantined())),
    ])
}

/// Wall nanoseconds since `start` (saturates rather than truncates).
fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn git_sha() -> String {
    if let Ok(value) = std::env::var("EMATH_KEEP_GATE_GIT_SHA") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();
    if output.is_empty() {
        "unknown".into()
    } else {
        output
    }
}

/// Host identity for a run. Volatile: never part of keep-gate byte-compare.
fn hostname() -> String {
    for key in ["HOSTNAME", "HOST"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

/// Machine fingerprint recorded beside the gate metrics. Timing and host
/// fields stay out of identity comparison (`compare.volatile`).
#[allow(clippy::cast_precision_loss)]
fn machine_object() -> lab::json::JsonValue {
    use lab::json::JsonValue;
    let cpu_count = std::thread::available_parallelism()
        .map(|count| count.get() as f64)
        .unwrap_or(0.0);
    JsonValue::Object(vec![
        ("os".into(), JsonValue::String(std::env::consts::OS.into())),
        ("arch".into(), JsonValue::String(std::env::consts::ARCH.into())),
        ("family".into(), JsonValue::String(std::env::consts::FAMILY.into())),
        ("cpu_count".into(), JsonValue::Number(cpu_count)),
        ("hostname".into(), JsonValue::String(hostname())),
    ])
}

/// Fields a byte-compare of keep-gate JSON must skip (run provenance + timing).
fn volatile_field_names() -> lab::json::JsonValue {
    use lab::json::JsonValue;
    JsonValue::Array(
        [
            "git_sha",
            "timestamp_unix",
            "machine",
            "samples",
            "summary",
        ]
        .into_iter()
        .map(|name| JsonValue::String(name.into()))
        .collect(),
    )
}

/// Fields that pin a keep-gate cell: schema, family, profile, identity SHA.
fn gate_field_names() -> lab::json::JsonValue {
    use lab::json::JsonValue;
    JsonValue::Array(
        ["schema", "family", "profile", "identity"]
            .into_iter()
            .map(|name| JsonValue::String(name.into()))
            .collect(),
    )
}

fn compare_object() -> lab::json::JsonValue {
    use lab::json::JsonValue;
    JsonValue::Object(vec![
        ("gate".into(), gate_field_names()),
        ("volatile".into(), volatile_field_names()),
    ])
}

/// Echo a written artifact so remote `rch exec` logs can reconstruct it locally.
fn emit_written_file(path: &Path, rendered: &str) {
    let name = path
        .file_name()
        .map_or_else(|| "unknown".into(), |name| name.to_string_lossy().into_owned());
    println!("keep-gate-file {name}");
    println!("{rendered}");
    println!("keep-gate-file-end");
}

/// Writes `.bench-history/<family>.latest.json` and returns the family
/// summary line for stdout.
fn write_history(
    history_dir: &Path,
    family: &str,
    detail: &str,
    samples: &[Sample],
) -> Result<String, String> {
    use lab::json::JsonValue;
    let samples_ns: Vec<u64> = samples.iter().map(|sample| sample.ns).collect();
    let measurement = lab::Measurement {
        metric_id: family.to_string() + ".ns",
        kind: lab::MeasurementKind::LatencyNs,
        unit: "ns".into(),
        samples: samples_ns.clone(),
    };
    let summary = measurement
        .summarize()
        .map_err(|error| format!("{}: {}", error.code, error.message))?;
    #[allow(clippy::cast_precision_loss)] // ns counts below 2^53 stay exact
    let raw: Vec<JsonValue> = samples_ns
        .iter()
        .map(|sample| JsonValue::Number(*sample as f64))
        .collect();
    let identity = &samples[0];
    let identity_sha = lab::hex(&lab::digest(&identity.identity_bytes));
    let identity_sha_json = identity_sha.clone();
    let document = JsonValue::Object(vec![
        ("schema".into(), JsonValue::String(SCHEMA.into())),
        ("family".into(), JsonValue::String(family.into())),
        ("profile".into(), JsonValue::String("release-perf".into())),
        ("cell".into(), JsonValue::String(detail.into())),
        ("git_sha".into(), JsonValue::String(git_sha())),
        ("timestamp_unix".into(), JsonValue::Number(now_unix_f64())),
        ("machine".into(), machine_object()),
        ("compare".into(), compare_object()),
        ("samples".into(), JsonValue::Array(raw)),
        ("summary".into(), summary_object(&summary)),
        (
            "identity".into(),
            JsonValue::Object(vec![
                ("kind".into(), JsonValue::String("sha256".into())),
                ("value".into(), JsonValue::String(identity_sha_json)),
                (
                    "label".into(),
                    JsonValue::String(identity.identity_label.clone()),
                ),
                (
                    "golden_lib_rs_match".into(),
                    match identity.golden_match {
                        Some(matched) => JsonValue::Bool(matched),
                        None => JsonValue::Null,
                    },
                ),
            ]),
        ),
    ]);
    std::fs::create_dir_all(history_dir)
        .map_err(|error| format!("cannot create {}: {error}", history_dir.display()))?;
    let target = history_dir.join(format!("{family}.latest.json"));
    let rendered = lab::json::write(&document);
    std::fs::write(&target, &rendered)
        .map_err(|error| format!("cannot write {}: {error}", target.display()))?;
    emit_written_file(&target, &rendered);
    let status = if summary.quarantined() {
        "quarantined"
    } else {
        "ok"
    };
    Ok(format!(
        "{family}: {:.2} ms median | cv {:.2}% | {status} | samples {} | identity={}",
        summary.median / 1e6,
        summary.cv_pct,
        samples_ns.len(),
        identity_sha
    ))
}

/// Wall seconds since the Unix epoch (0 when the clock is absurd).
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

/// Current wall-clock as an f64 JSON number (epoch seconds; exact below
/// the f64 integer range for any plausible timestamp).
#[allow(clippy::cast_precision_loss)]
fn now_unix_f64() -> f64 {
    now_unix() as f64
}

/// Guard file: deterministic-codegen + std-only flags plus provenance.
fn write_guard(history_dir: &Path) -> Result<(), String> {
    use lab::json::JsonValue;
    let document = JsonValue::Object(vec![
        ("schema".into(), JsonValue::String(GUARD_SCHEMA.into())),
        ("deterministic_codegen".into(), JsonValue::Bool(true)),
        ("phase1_std_only".into(), JsonValue::Bool(true)),
        ("git_sha".into(), JsonValue::String(git_sha())),
        ("timestamp_unix".into(), JsonValue::Number(now_unix_f64())),
        ("machine".into(), machine_object()),
        (
            "compare".into(),
            JsonValue::Object(vec![
                (
                    "gate".into(),
                    JsonValue::Array(
                        [
                            "schema",
                            "deterministic_codegen",
                            "phase1_std_only",
                        ]
                        .into_iter()
                        .map(|name| JsonValue::String(name.into()))
                        .collect(),
                    ),
                ),
                (
                    "volatile".into(),
                    JsonValue::Array(
                        ["git_sha", "timestamp_unix", "machine"]
                            .into_iter()
                            .map(|name| JsonValue::String(name.into()))
                            .collect(),
                    ),
                ),
            ]),
        ),
    ]);
    let target = history_dir.join("guard.json");
    let rendered = lab::json::write(&document);
    std::fs::write(&target, &rendered)
        .map_err(|error| format!("cannot write {}: {error}", target.display()))?;
    emit_written_file(&target, &rendered);
    Ok(())
}

fn main() {
    // Install the source-parser backend once per process, exactly like
    // the CLI entry point; CompilerSession parses refuse without it
    // (E-SYN-120).
    emath_syntax::install_source_parser();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let smoke = args.iter().any(|arg| arg == "--smoke");
    let history_dir = args
        .iter()
        .position(|arg| arg == "--history-dir")
        .and_then(|index| args.get(index + 1))
        .map_or_else(|| root().join(HISTORY_REL), PathBuf::from);
    let config = FamilyConfig { smoke };
    let root = root();
    let families: Vec<(&str, Result<Vec<Sample>, String>)> = vec![
        ("parse", cell_parse(&root, &config)),
        ("check", cell_check(&root, &config)),
        (
            "codegen-parametric",
            cell_codegen_parametric(&root, &config),
        ),
        ("artifact-json", cell_artifact_json(&config)),
        ("genesis-replay", cell_genesis_replay(&root, &config)),
        ("cli8p", cell_cli8p(&root, &config)),
    ];
    let mut failures = 0_usize;
    for (family, samples) in &families {
        match samples {
            Ok(samples) => match write_history(&history_dir, family, &samples[0].detail, samples) {
                Ok(line) => println!("keep-gate: {line}"),
                Err(error) => {
                    eprintln!("keep-gate: {family}: history write failed: {error}");
                    failures += 1;
                }
            },
            Err(error) => {
                eprintln!("keep-gate: {family}: cell failed: {error}");
                failures += 1;
            }
        }
    }
    if let Err(error) = write_guard(&history_dir) {
        eprintln!("keep-gate: guard write failed: {error}");
        failures += 1;
    }
    std::process::exit(i32::from(failures != 0));
}
