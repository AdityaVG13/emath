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
const GLYPHS_REL: &str = "language/examples/01_arbitrary_glyphs.emath";
/// Committed generated golden for the identity comparison.
const SG_GENERATED_LIB_REL: &str = "examples/generated/semantic-genesis-worlds/src/lib.rs";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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
        let specs = worlds
            .iter()
            .map(|world| {
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
                emath_world_codegen_rust::WorldSpec {
                    label: world.name.to_ascii_lowercase(),
                    operators,
                }
            })
            .collect::<Vec<_>>();
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
    let bin = env!("CARGO_BIN_EXE_emath");
    let files = [
        "tests/valid/square.emath",
        "tests/valid/affine_scorer.emath",
        "language/examples/00_hello_square.emath",
        "language/examples/00_stateful_affine_scorer.emath",
        "language/examples/11_parametric_unknown_operator.emath",
        "language/examples/01_cache_policy.emath",
        "language/examples/02_tensor_program.emath",
        "language/examples/03_graph_router.emath",
    ];
    let cell = move || {
        let start = Instant::now();
        let children = files
            .iter()
            .map(|file| {
                Command::new(bin)
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
    std::fs::write(&target, lab::json::write(&document))
        .map_err(|error| format!("cannot write {}: {error}", target.display()))?;
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
    ]);
    let target = history_dir.join("guard.json");
    std::fs::write(&target, lab::json::write(&document))
        .map_err(|error| format!("cannot write {}: {error}", target.display()))
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
