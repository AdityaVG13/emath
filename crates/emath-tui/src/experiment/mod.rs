//! The experiment host: build, run, and measure an explicitly supplied
//! baseline and candidate, then hand the captured facts to an authored
//! evaluator and return its decision.
//!
//! Ownership boundary. The host collects facts: artifact and source
//! digests, the workload digest, the toolchain/hardware fingerprint, raw
//! host-clocked wall times with their pair ordering, captured outputs, and
//! build costs. The authored evaluator (an ordinary `emath function`, e.g.
//! `HotpathEvaluation` in `language/templates/research_targets/rust_hotpath.emath`)
//! decides what those facts mean. The host never ranks, filters, averages,
//! or promotes; it only refuses to call the evaluator when execution was
//! incomplete.
//!
//! Evaluator seam. The evaluator declares a subset of [`HOST_FACTS`] as its
//! inputs, by name, and has exactly one output: a record with an Int
//! `status` field (and a `consumed: sequence(Int)` field when it declares
//! `audit_id`). An input outside the fact set refuses at admission.
//!
//! Commit semantics. State is an `emath.scratch.v1` checkpoint whose
//! identity binds the evaluator meaning id and an experiment digest over
//! manifest, source, workload, toolchain and hardware. A pair is journaled
//! as pending before it is launched and committed after both runs finish.
//! A pending pair found on resume is retained as an `uncertain` attempt,
//! never counted, and the pair index reruns: external execution is at
//! least once, measurement counting is exactly once.
//!
//! Audit ledger. A host file, outside candidate control, lists every audit
//! workload digest an evaluation has consumed. The evaluator receives the
//! consumed ids; a workload consumed once can never look fresh again, across
//! resumes and across experiments sharing the ledger. The audit id is
//! derived from the workload's content digest by the host; it is not a
//! caller-supplied number. It still proves nothing about disjointness from
//! data the candidate's author saw elsewhere.

pub mod process;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use emath_artifact::{JsonValue, parse_json_document};
use emath_core::tree::{Item, SyntaxTree};
use emath_core::{json_quote, sha256_digest};
use emath_exec_ir::constructor_layer::{
    CValue, IMAGE_IDENTITY, ScratchIdentity, evaluate_function_at, load_scratch, save_scratch,
};
use emath_exec_ir::exact_int::ExactInt;
use emath_syntax::parse_str;

use crate::host::{HostFault, admitted_meaning_id, section_fields, value_int};
use process::{CancelToken, RunOutcome, RunSpec, TrialAccess};

/// The manifest schema line.
pub const MANIFEST_SCHEMA: &str = "emath.experiment.v1";
/// The audit ledger schema line.
pub const LEDGER_SCHEMA: &str = "emath.audit-ledger.v1";

/// The facts an evaluator may declare as inputs, with their carriers.
pub const HOST_FACTS: &[(&str, &str)] = &[
    ("baseline_outputs", "sequence(Int): baseline stdout integers from its first measured run"),
    ("candidate_outputs", "sequence(Int): candidate stdout integers from its first measured run"),
    ("outputs_stable", "Bool: every run of each arm reproduced that arm's first outputs"),
    ("samples", "sequence(PairedObs): paired wall-time nanoseconds in pair-index order"),
    ("baseline_context", "EvalContext: host-derived workload/evaluator ids"),
    ("candidate_context", "EvalContext: host-derived workload/evaluator ids"),
    ("audit_id", "Int: host id of the workload's content digest"),
    ("consumed", "sequence(Int): audit ids already consumed in the ledger"),
    ("costs", "CostAccounting: proposal (manifest or -1), candidate compilation and execution ns"),
];

fn fault(code: &str, message: impl Into<String>) -> HostFault {
    HostFault {
        code: code.into(),
        message: message.into(),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn digest_hex(bytes: &[u8]) -> String {
    hex(&sha256_digest(bytes))
}

/// A non-negative 60-bit id from a digest (authored ids are Int).
fn id60(text: &str) -> i128 {
    let digest = sha256_digest(text.as_bytes());
    let mut word = [0u8; 8];
    word.copy_from_slice(&digest[..8]);
    i128::from(u64::from_be_bytes(word) >> 4)
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Whether pair `index` runs the candidate first. Seeded and index-keyed,
/// so a resumed session reproduces the schedule.
#[must_use]
pub fn candidate_first(seed: u64, index: u64) -> bool {
    splitmix64(seed ^ index.wrapping_mul(0xD6E8_FEB8_6659_FD93)) & 1 == 1
}

// ---- manifest -----------------------------------------------------------

/// The admitted manifest. Paths are absolute.
#[derive(Clone, Debug)]
pub struct ExperimentManifest {
    pub experiment_id: String,
    pub baseline: PathBuf,
    pub candidate: PathBuf,
    pub workload: PathBuf,
    pub evaluator_module: PathBuf,
    pub evaluator_function: String,
    pub pairs: u64,
    pub warmup: u64,
    pub seed: u64,
    pub experiment_ms: u64,
    pub build_ms: u64,
    pub trial_ms: u64,
    pub output_bytes: u64,
    pub memory_mb: Option<u64>,
    pub network: bool,
    pub filesystem_write: bool,
    pub require_isolation: bool,
    pub proposal_cost: Option<i64>,
}

fn obj<'a>(value: &'a JsonValue, what: &str) -> Result<&'a [(String, JsonValue)], HostFault> {
    match value {
        JsonValue::Obj(entries) => Ok(entries),
        other => Err(fault("experiment_manifest", format!("`{what}` must be an object, found {other:?}"))),
    }
}

fn get<'a>(entries: &'a [(String, JsonValue)], key: &str) -> Option<&'a JsonValue> {
    entries.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

fn req<'a>(entries: &'a [(String, JsonValue)], key: &str) -> Result<&'a JsonValue, HostFault> {
    get(entries, key).ok_or_else(|| fault("experiment_manifest", format!("missing required field `{key}`")))
}

fn string(value: &JsonValue, key: &str) -> Result<String, HostFault> {
    match value {
        JsonValue::Str(text) if !text.is_empty() => Ok(text.clone()),
        other => Err(fault("experiment_manifest", format!("`{key}` must be a non-empty string, found {other:?}"))),
    }
}

fn number(value: &JsonValue, key: &str) -> Result<u64, HostFault> {
    match value {
        JsonValue::Num(text) => text
            .parse::<u64>()
            .map_err(|_| fault("experiment_manifest", format!("`{key}` must be a non-negative integer, found {text}"))),
        other => Err(fault("experiment_manifest", format!("`{key}` must be a number, found {other:?}"))),
    }
}

fn boolean(value: &JsonValue, key: &str) -> Result<bool, HostFault> {
    match value {
        JsonValue::Bool(b) => Ok(*b),
        other => Err(fault("experiment_manifest", format!("`{key}` must be a boolean, found {other:?}"))),
    }
}

fn opt_number(entries: &[(String, JsonValue)], key: &str) -> Result<Option<u64>, HostFault> {
    match get(entries, key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(value) => number(value, key).map(Some),
    }
}

fn locate(base: &Path, relative: &str, key: &str) -> Result<PathBuf, HostFault> {
    let path = base.join(relative);
    path.canonicalize().map_err(|err| {
        fault("experiment_manifest", format!("`{key}` path {} does not resolve: {err}", path.display()))
    })
}

impl ExperimentManifest {
    /// Parse and validate a manifest file.
    pub fn read(path: &Path) -> Result<Self, HostFault> {
        let text = std::fs::read_to_string(path)
            .map_err(|err| fault("experiment_read", format!("cannot read manifest {}: {err}", path.display())))?;
        let json = parse_json_document(&text)
            .map_err(|err| fault("experiment_manifest", format!("{} is not JSON: {err}", path.display())))?;
        let base = path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let top = obj(&json, "manifest")?;
        let schema = string(req(top, "schema")?, "schema")?;
        if schema != MANIFEST_SCHEMA {
            return Err(fault("experiment_manifest", format!("schema `{schema}` is not `{MANIFEST_SCHEMA}`")));
        }
        let arm = |key: &str| -> Result<PathBuf, HostFault> {
            let entries = obj(req(top, key)?, key)?;
            locate(&base, &string(req(entries, "crate")?, "crate")?, key)
        };
        let evaluator = obj(req(top, "evaluator")?, "evaluator")?;
        let measurement = obj(req(top, "measurement")?, "measurement")?;
        let budget = obj(req(top, "budget")?, "budget")?;
        let access = obj(req(top, "access")?, "access")?;
        let proposal_cost = match get(top, "proposal_cost") {
            None | Some(JsonValue::Null) => None,
            Some(value) => Some(i64::try_from(number(value, "proposal_cost")?).map_err(|_| {
                fault("experiment_manifest", "`proposal_cost` exceeds the Int host projection")
            })?),
        };
        let manifest = Self {
            experiment_id: string(req(top, "experiment_id")?, "experiment_id")?,
            baseline: arm("baseline")?,
            candidate: arm("candidate")?,
            workload: locate(&base, &string(req(top, "workload")?, "workload")?, "workload")?,
            evaluator_module: locate(&base, &string(req(evaluator, "module")?, "module")?, "evaluator.module")?,
            evaluator_function: string(req(evaluator, "function")?, "function")?,
            pairs: number(req(measurement, "pairs")?, "pairs")?,
            warmup: number(req(measurement, "warmup")?, "warmup")?,
            seed: number(req(measurement, "seed")?, "seed")?,
            experiment_ms: number(req(budget, "experiment_ms")?, "experiment_ms")?,
            build_ms: number(req(budget, "build_ms")?, "build_ms")?,
            trial_ms: number(req(budget, "trial_ms")?, "trial_ms")?,
            output_bytes: number(req(budget, "output_bytes")?, "output_bytes")?,
            memory_mb: opt_number(budget, "memory_mb")?,
            network: boolean(req(access, "network")?, "network")?,
            filesystem_write: boolean(req(access, "filesystem_write")?, "filesystem_write")?,
            require_isolation: boolean(req(access, "require_isolation")?, "require_isolation")?,
            proposal_cost,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), HostFault> {
        if self.pairs == 0 {
            return Err(fault("experiment_manifest", "`measurement.pairs` must be at least 1"));
        }
        for (key, value) in [
            ("experiment_ms", self.experiment_ms),
            ("build_ms", self.build_ms),
            ("trial_ms", self.trial_ms),
            ("output_bytes", self.output_bytes),
        ] {
            if value == 0 {
                return Err(fault("experiment_manifest", format!("`budget.{key}` must be positive")));
            }
        }
        if self.baseline == self.candidate {
            return Err(fault("experiment_manifest", "baseline and candidate are the same crate"));
        }
        for (arm, dir) in [("baseline", &self.baseline), ("candidate", &self.candidate)] {
            for file in ["Cargo.toml", "Cargo.lock"] {
                if !dir.join(file).is_file() {
                    return Err(fault(
                        "experiment_manifest",
                        format!(
                            "the {arm} crate {} has no {file}: builds run `--locked --offline` so the \
                             measured source cannot drift",
                            dir.display()
                        ),
                    ));
                }
            }
            if self.workload.starts_with(dir) {
                return Err(fault(
                    "experiment_manifest",
                    format!("the audit workload lies inside the {arm} crate; keep it outside candidate source"),
                ));
            }
        }
        Ok(())
    }
}

// ---- admission ------------------------------------------------------------

/// The declared evaluator, admitted.
#[derive(Clone, Debug)]
struct Evaluator {
    tree: SyntaxTree,
    meaning_id: String,
    inputs: Vec<String>,
}

fn admit_evaluator(manifest: &ExperimentManifest) -> Result<Evaluator, HostFault> {
    let module = &manifest.evaluator_module;
    let source = std::fs::read_to_string(module)
        .map_err(|err| fault("experiment_evaluator", format!("cannot read {}: {err}", module.display())))?;
    emath_syntax::install_source_parser();
    let (tree, diagnostics) = parse_str(&source);
    if diagnostics.has_errors() {
        return Err(fault("experiment_evaluator", format!("{} does not parse: {diagnostics:?}", module.display())));
    }
    let meaning_id = admitted_meaning_id(module, &source).map_err(|f| fault("experiment_evaluator", f.message))?;
    let name = &manifest.evaluator_function;
    let decl = tree
        .items
        .iter()
        .find_map(|item| match item {
            Item::Declaration(decl) if decl.as_kind == "function" && &decl.name == name => Some(decl),
            _ => None,
        })
        .ok_or_else(|| fault("experiment_evaluator", format!("{} declares no function `{name}`", module.display())))?;
    let inputs: Vec<String> = section_fields(decl, "inputs").into_iter().map(|(n, _)| n).collect();
    if section_fields(decl, "outputs").len() != 1 {
        return Err(fault("experiment_evaluator", format!("`{name}` must have exactly one output (the decision record)")));
    }
    for input in &inputs {
        if !HOST_FACTS.iter().any(|(fact, _)| fact == input) {
            let facts: Vec<String> = HOST_FACTS.iter().map(|(n, d)| format!("{n}: {d}")).collect();
            return Err(fault(
                "experiment_evaluator",
                format!("`{name}` input `{input}` is not a host fact; declare only:\n  {}", facts.join("\n  ")),
            ));
        }
    }
    Ok(Evaluator { tree, meaning_id, inputs })
}

fn command_text(program: &str, args: &[&str]) -> String {
    std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map_or_else(|| "unavailable".into(), |out| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Toolchain and hardware/software facts bound into the identity.
#[must_use]
pub fn environment_fingerprint() -> Vec<(String, String)> {
    let cpu = if cfg!(target_os = "macos") {
        command_text("sysctl", &["-n", "machdep.cpu.brand_string"])
    } else {
        std::fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|text| {
                text.lines()
                    .find(|line| line.starts_with("model name"))
                    .map(|line| line.split(':').nth(1).unwrap_or("").trim().to_string())
            })
            .unwrap_or_else(|| "unavailable".into())
    };
    let cores = std::thread::available_parallelism().map_or(0, std::num::NonZero::get);
    vec![
        ("rustc".into(), command_text("rustc", &["-vV"])),
        ("cargo".into(), command_text("cargo", &["-V"])),
        ("os".into(), command_text("uname", &["-srm"])),
        ("cpu".into(), cpu),
        ("available_parallelism".into(), cores.to_string()),
        ("rustflags".into(), std::env::var("RUSTFLAGS").unwrap_or_default()),
    ]
}

fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            walk(&path, root, out)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
            out.push((rel, path));
        }
    }
    Ok(())
}

/// Digest of every source file of a crate (excluding `target/` and dot
/// entries), keyed by relative path.
pub fn source_digest(dir: &Path) -> Result<String, HostFault> {
    let mut files = Vec::new();
    walk(dir, dir, &mut files).map_err(|err| fault("experiment_read", format!("cannot walk {}: {err}", dir.display())))?;
    files.sort();
    let mut bytes = Vec::new();
    for (rel, path) in files {
        let content = std::fs::read(&path)
            .map_err(|err| fault("experiment_read", format!("cannot read {}: {err}", path.display())))?;
        bytes.extend_from_slice(rel.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(content.len().to_string().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&content);
    }
    Ok(digest_hex(&bytes))
}

fn package_name(dir: &Path) -> Result<String, HostFault> {
    let text = std::fs::read_to_string(dir.join("Cargo.toml"))
        .map_err(|err| fault("experiment_read", format!("cannot read {}/Cargo.toml: {err}", dir.display())))?;
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package
            && let Some(rest) = line.strip_prefix("name")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            return Ok(value.trim().trim_matches('"').to_string());
        }
    }
    Err(fault("experiment_manifest", format!("{}/Cargo.toml has no [package] name", dir.display())))
}

/// The build command every arm uses; part of the identity.
pub const BUILD_OPTIONS: &str = "cargo build --release --locked --offline";

/// How each limit is (or is not) enforced on this host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enforcement {
    pub limit: &'static str,
    pub status: String,
}

/// An admitted experiment: validated manifest, admitted evaluator, and
/// the identity every checkpoint and ledger entry is bound to.
#[derive(Clone, Debug)]
pub struct ExperimentHost {
    manifest: ExperimentManifest,
    evaluator: Evaluator,
    baseline_source: String,
    candidate_source: String,
    workload_digest: String,
    environment: Vec<(String, String)>,
    identity: String,
    sandboxed: bool,
    enforcement: Vec<Enforcement>,
}

impl ExperimentHost {
    /// Admit a manifest: validate it, admit the evaluator, digest sources
    /// and workload, fingerprint the environment, and decide enforcement.
    /// Nothing is built or run.
    pub fn admit(manifest_path: &Path) -> Result<Self, HostFault> {
        let manifest = ExperimentManifest::read(manifest_path)?;
        let evaluator = admit_evaluator(&manifest)?;
        let baseline_source = source_digest(&manifest.baseline)?;
        let candidate_source = source_digest(&manifest.candidate)?;
        let workload = std::fs::read(&manifest.workload)
            .map_err(|err| fault("experiment_read", format!("cannot read workload: {err}")))?;
        let workload_digest = digest_hex(&workload);
        let environment = environment_fingerprint();
        let sandboxed = process::sandbox_exec().is_some();
        if manifest.require_isolation && !sandboxed {
            return Err(fault(
                "experiment_isolation",
                "the manifest requires isolation but this host has no supported sandbox \
                 (macOS sandbox-exec); a plain subprocess is not a sandbox. Set \
                 `access.require_isolation` to false to run unisolated and have that reported",
            ));
        }
        let memory = match manifest.memory_mb {
            None => "not requested".to_string(),
            Some(mb) if !sandboxed && process::prlimit().is_some() => {
                format!("enforced: prlimit --as={} bytes (address space)", mb * 1024 * 1024)
            }
            Some(_) => {
                return Err(fault(
                    "experiment_unenforceable",
                    "`budget.memory_mb` was requested but this host cannot enforce a memory limit \
                     (macOS ignores RLIMIT_AS; Linux needs prlimit). Remove it to run without one",
                ));
            }
        };
        let deny = |allowed: bool, what: &str| {
            if allowed {
                "permitted by manifest".to_string()
            } else if sandboxed {
                format!("enforced: sandbox-exec denies {what}")
            } else {
                "UNENFORCED: no sandbox on this host".to_string()
            }
        };
        let enforcement = vec![
            Enforcement {
                limit: "trial_time",
                status: format!("enforced: host kills each run at min({} ms, remaining experiment budget)", manifest.trial_ms),
            },
            Enforcement {
                limit: "build_time",
                status: format!("enforced: host kills cargo at min({} ms, remaining experiment budget)", manifest.build_ms),
            },
            Enforcement {
                limit: "experiment_time",
                status: format!(
                    "enforced: {} ms across all sessions for builds and runs; the authored evaluation is outside this clock",
                    manifest.experiment_ms
                ),
            },
            Enforcement {
                limit: "output",
                status: format!("enforced: stdout capped at {} bytes, stderr tail at 64 KiB", manifest.output_bytes),
            },
            Enforcement { limit: "memory", status: memory },
            Enforcement {
                limit: "processes",
                status: if sandboxed {
                    "enforced: sandbox-exec denies fork and any exec but the arm binary".into()
                } else {
                    "UNENFORCED: children may fork; the host kills direct children on stop".into()
                },
            },
            Enforcement { limit: "network", status: deny(manifest.network, "network*") },
            Enforcement { limit: "filesystem_write", status: deny(manifest.filesystem_write, "file-write*") },
            Enforcement {
                limit: "protected_reads",
                status: if sandboxed {
                    "enforced: builds and runs cannot read the workload, ledger or checkpoint files".into()
                } else {
                    "UNENFORCED: no sandbox on this host".into()
                },
            },
            Enforcement {
                limit: "environment",
                status: "enforced: runs start with an empty environment; input is stdin only".into(),
            },
        ];
        let mut canonical = String::new();
        let mut line = |key: &str, value: &str| {
            canonical.push_str(key);
            canonical.push('=');
            canonical.push_str(value);
            canonical.push('\n');
        };
        line("schema", MANIFEST_SCHEMA);
        line("experiment_id", &manifest.experiment_id);
        line("baseline_source", &baseline_source);
        line("candidate_source", &candidate_source);
        line("workload", &workload_digest);
        line("evaluator_meaning", &evaluator.meaning_id);
        line("evaluator_function", &manifest.evaluator_function);
        line("build", BUILD_OPTIONS);
        line(
            "measurement",
            &format!("pairs={} warmup={} seed={}", manifest.pairs, manifest.warmup, manifest.seed),
        );
        line(
            "budget",
            &format!(
                "experiment_ms={} build_ms={} trial_ms={} output_bytes={} memory_mb={:?}",
                manifest.experiment_ms, manifest.build_ms, manifest.trial_ms, manifest.output_bytes, manifest.memory_mb
            ),
        );
        line(
            "access",
            &format!(
                "network={} filesystem_write={} sandboxed={sandboxed}",
                manifest.network, manifest.filesystem_write
            ),
        );
        line("proposal_cost", &format!("{:?}", manifest.proposal_cost));
        for (key, value) in &environment {
            line(&format!("env.{key}"), value);
        }
        let identity = digest_hex(canonical.as_bytes());
        Ok(Self {
            manifest,
            evaluator,
            baseline_source,
            candidate_source,
            workload_digest,
            environment,
            identity,
            sandboxed,
            enforcement,
        })
    }

    #[must_use]
    pub fn manifest(&self) -> &ExperimentManifest {
        &self.manifest
    }

    /// The experiment digest (manifest, sources, workload, evaluator, env).
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }

    #[must_use]
    pub fn enforcement(&self) -> &[Enforcement] {
        &self.enforcement
    }

    #[must_use]
    pub fn environment(&self) -> &[(String, String)] {
        &self.environment
    }

    #[must_use]
    pub fn workload_digest(&self) -> &str {
        &self.workload_digest
    }

    #[must_use]
    pub fn source_digests(&self) -> (&str, &str) {
        (&self.baseline_source, &self.candidate_source)
    }

    /// The host id of the audit workload (its content digest, truncated).
    #[must_use]
    pub fn audit_id(&self) -> i128 {
        id60(&format!("audit:{}", self.workload_digest))
    }

    fn scratch_identity(&self) -> ScratchIdentity {
        ScratchIdentity {
            meaning_id: self.evaluator.meaning_id.clone(),
            language_id: IMAGE_IDENTITY.into(),
            target: format!("experiment:{}:{}", self.manifest.experiment_id, self.identity),
        }
    }
}

// ---- durable state --------------------------------------------------------

/// One committed pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairRow {
    pub index: u64,
    pub candidate_first: bool,
    pub baseline_ns: u128,
    pub candidate_ns: u128,
    pub session: u64,
}

/// A warmup run: recorded, never passed to the evaluator as a sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WarmupRow {
    pub session: u64,
    pub arm: String,
    pub wall_ns: u128,
}

/// A launched-but-uncounted attempt: `uncertain` (host lost it) or
/// `cancelled` (host killed it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttemptRow {
    pub index: u64,
    pub session: u64,
    pub status: String,
}

/// One build of one arm in one session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildRow {
    pub arm: String,
    pub binary_digest: String,
    pub build_ns: u128,
    pub session: u64,
}

/// A terminal execution failure; no evaluation follows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureRow {
    pub arm: String,
    pub kind: String,
    pub detail: String,
}

/// The checkpointed experiment state.
#[derive(Clone, Debug, Default)]
pub struct ExperimentState {
    pub session: u64,
    pub pending: Option<u64>,
    pub elapsed_ns: u128,
    pub pairs: Vec<PairRow>,
    pub warmups: Vec<WarmupRow>,
    pub attempts: Vec<AttemptRow>,
    pub builds: Vec<BuildRow>,
    pub baseline_outputs: Option<Vec<i128>>,
    pub candidate_outputs: Option<Vec<i128>>,
    pub outputs_stable: bool,
    pub failure: Option<FailureRow>,
    pub decision: Option<CValue>,
}

fn int(value: impl Into<i128>) -> CValue {
    CValue::Int(ExactInt::from(value.into()))
}

fn big(value: u128) -> CValue {
    int(i128::try_from(value).unwrap_or(i128::MAX))
}

fn text(value: &str) -> CValue {
    CValue::Str(value.into())
}

fn record(type_name: &str, fields: Vec<(&str, CValue)>) -> CValue {
    CValue::Record {
        type_name: type_name.into(),
        fields: Arc::new(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
    }
}

fn seq(items: Vec<CValue>) -> CValue {
    CValue::Sequence(Arc::new(items))
}

fn int_seq(items: &[i128]) -> CValue {
    seq(items.iter().map(|&n| int(n)).collect())
}

fn state_fault(message: impl Into<String>) -> HostFault {
    fault("experiment_state", message)
}

fn fields_of(value: &CValue) -> Result<&BTreeMap<String, CValue>, HostFault> {
    match value {
        CValue::Record { fields, .. } => Ok(fields.as_ref()),
        other => Err(state_fault(format!("expected a record, found {other:?}"))),
    }
}

fn field<'a>(value: &'a CValue, name: &str) -> Result<&'a CValue, HostFault> {
    fields_of(value)?.get(name).ok_or_else(|| state_fault(format!("missing field `{name}`")))
}

fn as_i128(value: &CValue) -> Result<i128, HostFault> {
    match value {
        CValue::Int(n) => n.to_i128().ok_or_else(|| state_fault("integer exceeds i128")),
        other => Err(state_fault(format!("expected an Int, found {other:?}"))),
    }
}

fn as_u(value: &CValue) -> Result<u128, HostFault> {
    u128::try_from(as_i128(value)?).map_err(|_| state_fault("negative count in state"))
}

fn as_str(value: &CValue) -> Result<String, HostFault> {
    match value {
        CValue::Str(text) => Ok(text.to_string()),
        other => Err(state_fault(format!("expected a Str, found {other:?}"))),
    }
}

fn as_bool(value: &CValue) -> Result<bool, HostFault> {
    match value {
        CValue::Bool(b) => Ok(*b),
        other => Err(state_fault(format!("expected a Bool, found {other:?}"))),
    }
}

fn as_seq(value: &CValue) -> Result<&[CValue], HostFault> {
    match value {
        CValue::Sequence(items) => Ok(items.as_slice()),
        other => Err(state_fault(format!("expected a Sequence, found {other:?}"))),
    }
}

fn opt_ints(value: &CValue) -> Result<Option<Vec<i128>>, HostFault> {
    match value {
        CValue::Absent => Ok(None),
        other => as_seq(other)?.iter().map(as_i128).collect::<Result<_, _>>().map(Some),
    }
}

impl ExperimentState {
    fn to_value(&self) -> CValue {
        let pairs = self
            .pairs
            .iter()
            .map(|p| {
                record(
                    "PairRun",
                    vec![
                        ("index", int(p.index)),
                        ("order", text(if p.candidate_first { "candidate_first" } else { "baseline_first" })),
                        ("baseline_ns", big(p.baseline_ns)),
                        ("candidate_ns", big(p.candidate_ns)),
                        ("session", int(p.session)),
                    ],
                )
            })
            .collect();
        let warmups = self
            .warmups
            .iter()
            .map(|w| record("WarmupRun", vec![("session", int(w.session)), ("arm", text(&w.arm)), ("wall_ns", big(w.wall_ns))]))
            .collect();
        let attempts = self
            .attempts
            .iter()
            .map(|a| record("Attempt", vec![("index", int(a.index)), ("session", int(a.session)), ("status", text(&a.status))]))
            .collect();
        let builds = self
            .builds
            .iter()
            .map(|b| {
                record(
                    "ArmBuild",
                    vec![
                        ("arm", text(&b.arm)),
                        ("binary_digest", text(&b.binary_digest)),
                        ("build_ns", big(b.build_ns)),
                        ("session", int(b.session)),
                    ],
                )
            })
            .collect();
        let failure = self.failure.as_ref().map_or(CValue::Absent, |f| {
            record("Failure", vec![("arm", text(&f.arm)), ("kind", text(&f.kind)), ("detail", text(&f.detail))])
        });
        record(
            "ExperimentState",
            vec![
                ("session", int(self.session)),
                ("pending", self.pending.map_or(int(-1), int)),
                ("elapsed_ns", big(self.elapsed_ns)),
                ("pairs", seq(pairs)),
                ("warmups", seq(warmups)),
                ("attempts", seq(attempts)),
                ("builds", seq(builds)),
                ("baseline_outputs", self.baseline_outputs.as_deref().map_or(CValue::Absent, int_seq)),
                ("candidate_outputs", self.candidate_outputs.as_deref().map_or(CValue::Absent, int_seq)),
                ("outputs_stable", CValue::Bool(self.outputs_stable)),
                ("failure", failure),
                ("decision", self.decision.clone().unwrap_or(CValue::Absent)),
            ],
        )
    }

    fn from_value(value: &CValue) -> Result<Self, HostFault> {
        let pending = as_i128(field(value, "pending")?)?;
        let pairs = as_seq(field(value, "pairs")?)?
            .iter()
            .map(|p| {
                Ok(PairRow {
                    index: as_u(field(p, "index")?)? as u64,
                    candidate_first: as_str(field(p, "order")?)? == "candidate_first",
                    baseline_ns: as_u(field(p, "baseline_ns")?)?,
                    candidate_ns: as_u(field(p, "candidate_ns")?)?,
                    session: as_u(field(p, "session")?)? as u64,
                })
            })
            .collect::<Result<_, HostFault>>()?;
        let warmups = as_seq(field(value, "warmups")?)?
            .iter()
            .map(|w| {
                Ok(WarmupRow {
                    session: as_u(field(w, "session")?)? as u64,
                    arm: as_str(field(w, "arm")?)?,
                    wall_ns: as_u(field(w, "wall_ns")?)?,
                })
            })
            .collect::<Result<_, HostFault>>()?;
        let attempts = as_seq(field(value, "attempts")?)?
            .iter()
            .map(|a| {
                Ok(AttemptRow {
                    index: as_u(field(a, "index")?)? as u64,
                    session: as_u(field(a, "session")?)? as u64,
                    status: as_str(field(a, "status")?)?,
                })
            })
            .collect::<Result<_, HostFault>>()?;
        let builds = as_seq(field(value, "builds")?)?
            .iter()
            .map(|b| {
                Ok(BuildRow {
                    arm: as_str(field(b, "arm")?)?,
                    binary_digest: as_str(field(b, "binary_digest")?)?,
                    build_ns: as_u(field(b, "build_ns")?)?,
                    session: as_u(field(b, "session")?)? as u64,
                })
            })
            .collect::<Result<_, HostFault>>()?;
        let failure = match field(value, "failure")? {
            CValue::Absent => None,
            f => Some(FailureRow {
                arm: as_str(field(f, "arm")?)?,
                kind: as_str(field(f, "kind")?)?,
                detail: as_str(field(f, "detail")?)?,
            }),
        };
        let decision = match field(value, "decision")? {
            CValue::Absent => None,
            d => Some(d.clone()),
        };
        Ok(Self {
            session: as_u(field(value, "session")?)? as u64,
            pending: u64::try_from(pending).ok(),
            elapsed_ns: as_u(field(value, "elapsed_ns")?)?,
            pairs,
            warmups,
            attempts,
            builds,
            baseline_outputs: opt_ints(field(value, "baseline_outputs")?)?,
            candidate_outputs: opt_ints(field(value, "candidate_outputs")?)?,
            outputs_stable: as_bool(field(value, "outputs_stable")?)?,
            failure,
            decision,
        })
    }
}

fn write_atomic(path: &Path, write: impl FnOnce(&Path) -> Result<(), HostFault>) -> Result<(), HostFault> {
    let tmp = path.with_extension("tmp");
    write(&tmp)?;
    std::fs::rename(&tmp, path)
        .map_err(|err| state_fault(format!("cannot commit {}: {err}", path.display())))
}

// ---- audit ledger -----------------------------------------------------------

/// One ledger row: an audit workload an evaluation reserved or consumed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgerEntry {
    pub audit_id: i128,
    pub audit_digest: String,
    pub experiment: String,
    /// `pending` (evaluation started, decision not yet committed) or
    /// `consumed`.
    pub state: String,
}

fn ledger_fault(message: impl Into<String>) -> HostFault {
    fault("experiment_ledger", message)
}

/// Read the audit ledger; a missing file is an empty ledger.
pub fn read_ledger(path: &Path) -> Result<Vec<LedgerEntry>, HostFault> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(ledger_fault(format!("cannot read {}: {err}", path.display()))),
    };
    let json = parse_json_document(&text).map_err(|err| ledger_fault(format!("{} is torn: {err}", path.display())))?;
    let top = obj(&json, "ledger").map_err(|f| ledger_fault(f.message))?;
    match get(top, "schema") {
        Some(JsonValue::Str(schema)) if schema == LEDGER_SCHEMA => {}
        other => return Err(ledger_fault(format!("ledger schema is {other:?}, not `{LEDGER_SCHEMA}`"))),
    }
    let Some(JsonValue::Arr(rows)) = get(top, "entries") else {
        return Err(ledger_fault("ledger has no `entries` array"));
    };
    rows.iter()
        .map(|row| {
            let entries = obj(row, "entry").map_err(|f| ledger_fault(f.message))?;
            let s = |key: &str| match get(entries, key) {
                Some(JsonValue::Str(text)) => Ok(text.clone()),
                other => Err(ledger_fault(format!("ledger `{key}` is {other:?}"))),
            };
            let audit_id = match get(entries, "audit_id") {
                Some(JsonValue::Num(n)) => n.parse::<i128>().map_err(|_| ledger_fault("bad audit_id"))?,
                other => return Err(ledger_fault(format!("ledger `audit_id` is {other:?}"))),
            };
            let state = s("state")?;
            if state != "pending" && state != "consumed" {
                return Err(ledger_fault(format!("unknown ledger state `{state}`")));
            }
            Ok(LedgerEntry { audit_id, audit_digest: s("audit_digest")?, experiment: s("experiment")?, state })
        })
        .collect()
}

fn write_ledger(path: &Path, entries: &[LedgerEntry]) -> Result<(), HostFault> {
    let rows: Vec<String> = entries
        .iter()
        .map(|e| {
            format!(
                "{{\"audit_id\": {}, \"audit_digest\": {}, \"experiment\": {}, \"state\": {}}}",
                e.audit_id,
                json_quote(&e.audit_digest),
                json_quote(&e.experiment),
                json_quote(&e.state)
            )
        })
        .collect();
    let body = format!(
        "{{\"schema\": {}, \"entries\": [{}]}}\n",
        json_quote(LEDGER_SCHEMA),
        rows.join(", ")
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| ledger_fault(format!("cannot create {}: {err}", parent.display())))?;
    }
    write_atomic(path, |tmp| {
        std::fs::write(tmp, body).map_err(|err| ledger_fault(format!("cannot write {}: {err}", tmp.display())))
    })
}

// ---- running ----------------------------------------------------------------

/// Session configuration: where state lives and when to stop early.
#[derive(Clone, Debug)]
pub struct ExperimentConfig {
    pub state_dir: PathBuf,
    pub audit_ledger: PathBuf,
    /// Stop cleanly after committing this many pairs in this session.
    pub stop_after_pairs: Option<u64>,
    pub cancel: CancelToken,
}

/// Why a session returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExperimentStatus {
    /// The authored evaluator returned a decision. `replayed` means it was
    /// committed by an earlier session and read back, not re-evaluated.
    Decided { replayed: bool },
    /// The session stop button (`stop_after_pairs`) fired; resumable.
    Stopped,
    /// The cancel token fired; the running child was killed; resumable.
    Cancelled,
    /// A build or run failed; terminal for this state, no decision.
    Failed { arm: String, kind: String, detail: String },
    /// The whole-experiment budget ran out; terminal, no decision.
    BudgetExhausted,
}

/// What a session returns.
#[derive(Clone, Debug)]
pub struct ExperimentReport {
    pub status: ExperimentStatus,
    pub identity: String,
    pub checkpoint: PathBuf,
    pub state: ExperimentState,
    pub enforcement: Vec<Enforcement>,
}

impl ExperimentReport {
    /// The authored decision's Int `status`, when decided.
    pub fn decision_status(&self) -> Option<i128> {
        self.state.decision.as_ref().and_then(|d| value_int(d, "status").ok())
    }
}

const EXPERIMENT_BUDGET: &str = "experiment_budget";

struct Session<'a> {
    host: &'a ExperimentHost,
    config: &'a ExperimentConfig,
    checkpoint: PathBuf,
    state: ExperimentState,
    started: Instant,
    base_elapsed: u128,
    protected: Vec<PathBuf>,
    run_dir: PathBuf,
}

enum Step<T> {
    Done(T),
    Stop(ExperimentStatus),
}

impl Session<'_> {
    fn elapsed(&self) -> u128 {
        self.base_elapsed + self.started.elapsed().as_nanos()
    }

    fn remaining(&self) -> Duration {
        let budget = u128::from(self.host.manifest.experiment_ms) * 1_000_000;
        let left = budget.saturating_sub(self.elapsed());
        Duration::from_nanos(u64::try_from(left).unwrap_or(u64::MAX))
    }

    fn commit(&mut self) -> Result<(), HostFault> {
        self.state.elapsed_ns = self.elapsed();
        let identity = self.host.scratch_identity();
        let value = self.state.to_value();
        write_atomic(&self.checkpoint, |tmp| {
            save_scratch(tmp, &identity, 0, &value, &[]).map_err(HostFault::from)
        })
    }

    fn fail(&mut self, arm: &str, kind: &str, detail: String) -> Result<Step<()>, HostFault> {
        if let Some(index) = self.state.pending.take() {
            self.state.attempts.push(AttemptRow { index, session: self.state.session, status: kind.into() });
        }
        self.state.failure = Some(FailureRow { arm: arm.into(), kind: kind.into(), detail: detail.clone() });
        self.commit()?;
        Ok(Step::Stop(status_of_failure(arm, kind, &detail)))
    }

    fn build(&mut self, arm: &str, dir: &Path) -> Result<Step<PathBuf>, HostFault> {
        let target = self.config.state_dir.join("build").join(arm);
        let mut command = if self.host.sandboxed {
            let mut command = std::process::Command::new(process::sandbox_exec().unwrap_or_default());
            command.arg("-p").arg(process::build_profile(&self.protected)).arg("cargo");
            command
        } else {
            std::process::Command::new("cargo")
        };
        command
            .args(["build", "--release", "--locked", "--offline", "--quiet", "--manifest-path"])
            .arg(dir.join("Cargo.toml"))
            .arg("--target-dir")
            .arg(&target)
            .env_remove("CARGO_TARGET_DIR");
        let limit = Duration::from_millis(self.host.manifest.build_ms).min(self.remaining());
        let budget_bound = limit < Duration::from_millis(self.host.manifest.build_ms);
        let started = Instant::now();
        let output = emath_build::run_cargo_timed(command, limit);
        let build_ns = started.elapsed().as_nanos();
        let output = match output {
            Ok(output) => output,
            Err(detail) if detail.contains("E-RES-120") => {
                let kind = if budget_bound { EXPERIMENT_BUDGET } else { "build_timeout" };
                return self.fail(arm, kind, detail).map(stop_only);
            }
            Err(detail) => return self.fail(arm, "build_failed", detail).map(stop_only),
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return self.fail(arm, "build_failed", tail(&stderr)).map(stop_only);
        }
        let binary = target.join("release").join(package_name(dir)?);
        let bytes = std::fs::read(&binary)
            .map_err(|err| fault("experiment_artifact", format!("cannot read built {}: {err}", binary.display())))?;
        let binary_digest = digest_hex(&bytes);
        if let Some(first) = self.state.builds.iter().find(|b| b.arm == arm)
            && first.binary_digest != binary_digest
        {
            return Err(fault(
                "experiment_artifact",
                format!(
                    "the {arm} rebuild produced binary {binary_digest}, but earlier pairs measured {}: \
                     the build is not reproducible here, so these measurements cannot be continued. \
                     Start a new --state directory",
                    first.binary_digest
                ),
            ));
        }
        self.state.builds.push(BuildRow { arm: arm.into(), binary_digest, build_ns, session: self.state.session });
        self.commit()?;
        Ok(Step::Done(process::resolved(&binary)))
    }

    /// One run of one arm. Success returns (wall_ns, outputs).
    fn run_arm(&mut self, arm: &str, binary: &Path, workload: &[u8]) -> Result<Step<(u128, Vec<i128>)>, HostFault> {
        let trial = Duration::from_millis(self.host.manifest.trial_ms);
        let remaining = self.remaining();
        if remaining.is_zero() {
            return self.fail("experiment", EXPERIMENT_BUDGET, "no experiment budget left".into()).map(stop_only);
        }
        let timeout = trial.min(remaining);
        let access = TrialAccess {
            network: self.host.manifest.network,
            filesystem_write: self.host.manifest.filesystem_write,
            protected: self.protected.clone(),
        };
        let profile = process::trial_profile(binary, &access);
        let spec = RunSpec {
            program: binary,
            stdin: workload,
            cwd: &self.run_dir,
            timeout,
            output_limit: usize::try_from(self.host.manifest.output_bytes).unwrap_or(usize::MAX),
            sandbox_profile: self.host.sandboxed.then_some(profile.as_str()),
            address_space_limit: self.host.manifest.memory_mb.map(|mb| mb * 1024 * 1024),
        };
        match process::run_bounded(&spec, &self.config.cancel) {
            RunOutcome::Success { stdout, wall_ns } => match parse_outputs(&stdout) {
                Ok(outputs) => Ok(Step::Done((wall_ns, outputs))),
                Err(detail) => self.fail(arm, "invalid_output", detail).map(stop_only),
            },
            RunOutcome::Timeout { wall_ns } => {
                let kind = if timeout < trial { EXPERIMENT_BUDGET } else { "timeout" };
                self.fail(arm, kind, format!("killed after {wall_ns} ns (limit {timeout:?})")).map(stop_only)
            }
            RunOutcome::Cancelled { .. } => {
                if let Some(index) = self.state.pending.take() {
                    self.state.attempts.push(AttemptRow { index, session: self.state.session, status: "cancelled".into() });
                }
                self.commit()?;
                Ok(Step::Stop(ExperimentStatus::Cancelled))
            }
            RunOutcome::Crash { code, signal, stderr_tail } => self
                .fail(arm, "crash", format!("exit code {code:?}, signal {signal:?}: {stderr_tail}"))
                .map(stop_only),
            RunOutcome::OutputLimit { limit } => {
                self.fail(arm, "output_limit", format!("stdout exceeded {limit} bytes")).map(stop_only)
            }
            RunOutcome::SpawnFailed { detail } => self.fail(arm, "spawn_failed", detail).map(stop_only),
        }
    }

    fn observe(&mut self, arm: &str, outputs: Vec<i128>) {
        let slot = if arm == "baseline" { &mut self.state.baseline_outputs } else { &mut self.state.candidate_outputs };
        match slot {
            None => *slot = Some(outputs),
            Some(first) if *first != outputs => self.state.outputs_stable = false,
            Some(_) => {}
        }
    }
}

fn stop_only<T>(step: Step<()>) -> Step<T> {
    match step {
        Step::Stop(status) => Step::Stop(status),
        Step::Done(()) => unreachable!("a failure always stops"),
    }
}

fn status_of_failure(arm: &str, kind: &str, detail: &str) -> ExperimentStatus {
    if kind == EXPERIMENT_BUDGET {
        ExperimentStatus::BudgetExhausted
    } else {
        ExperimentStatus::Failed { arm: arm.into(), kind: kind.into(), detail: detail.into() }
    }
}

fn tail(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars[chars.len().saturating_sub(800)..].iter().collect()
}

/// stdout must be whitespace-separated integers; anything else is an
/// invalid output, never a partial result.
fn parse_outputs(stdout: &[u8]) -> Result<Vec<i128>, String> {
    let text = std::str::from_utf8(stdout).map_err(|_| "stdout is not UTF-8".to_string())?;
    text.split_whitespace()
        .map(|token| token.parse::<i64>().map(i128::from).map_err(|_| format!("stdout token `{token}` is not an Int")))
        .collect()
}

macro_rules! step {
    ($expr:expr) => {
        match $expr? {
            Step::Done(value) => value,
            Step::Stop(status) => return Ok(status),
        }
    };
}

impl ExperimentHost {
    /// Run (or resume) the experiment until it decides, stops, or fails.
    pub fn run(&self, config: &ExperimentConfig) -> Result<ExperimentReport, HostFault> {
        std::fs::create_dir_all(&config.state_dir)
            .map_err(|err| state_fault(format!("cannot create {}: {err}", config.state_dir.display())))?;
        let checkpoint = config.state_dir.join("checkpoint.scratch.json");
        let state = if checkpoint.is_file() {
            let loaded = load_scratch(&checkpoint, &self.scratch_identity()).map_err(|err| {
                if err.code == "scratch_identity" {
                    fault(
                        "experiment_identity",
                        format!(
                            "{} belongs to a different experiment (evaluator, workload, sources, budget, \
                             toolchain or hardware changed): {}. Observations from different identities \
                             never merge; use a new --state directory",
                            checkpoint.display(),
                            err.message
                        ),
                    )
                } else {
                    HostFault::from(err)
                }
            })?;
            ExperimentState::from_value(&loaded.state)?
        } else {
            ExperimentState { outputs_stable: true, ..ExperimentState::default() }
        };
        let run_dir = config.state_dir.join("run");
        std::fs::create_dir_all(&run_dir)
            .map_err(|err| state_fault(format!("cannot create {}: {err}", run_dir.display())))?;
        let protected = vec![
            process::resolved(&self.manifest.workload),
            process::resolved(&config.audit_ledger),
            process::resolved(&checkpoint),
        ];
        let base_elapsed = state.elapsed_ns;
        let mut session = Session {
            host: self,
            config,
            checkpoint: checkpoint.clone(),
            state,
            started: Instant::now(),
            base_elapsed,
            protected,
            run_dir,
        };
        let status = self.drive(&mut session)?;
        Ok(ExperimentReport {
            status,
            identity: self.identity.clone(),
            checkpoint,
            state: session.state,
            enforcement: self.enforcement.clone(),
        })
    }

    fn drive(&self, s: &mut Session<'_>) -> Result<ExperimentStatus, HostFault> {
        s.state.session += 1;
        if let Some(index) = s.state.pending.take() {
            s.state.attempts.push(AttemptRow { index, session: s.state.session, status: "uncertain".into() });
        }
        if s.state.decision.is_some() {
            self.finish_ledger(s.config)?;
            return Ok(ExperimentStatus::Decided { replayed: true });
        }
        if let Some(f) = &s.state.failure {
            return Ok(status_of_failure(&f.arm, &f.kind, &f.detail));
        }
        s.commit()?;

        let baseline = step!(s.build("baseline", &self.manifest.baseline));
        let candidate = step!(s.build("candidate", &self.manifest.candidate));
        let workload = std::fs::read(&self.manifest.workload)
            .map_err(|err| fault("experiment_read", format!("cannot read workload: {err}")))?;
        if digest_hex(&workload) != self.workload_digest {
            return Err(fault("experiment_identity", "the workload changed after admission"));
        }

        if (s.state.pairs.len() as u64) < self.manifest.pairs {
            for _ in 0..self.manifest.warmup {
                for (arm, binary) in [("baseline", &baseline), ("candidate", &candidate)] {
                    let (wall_ns, _) = step!(s.run_arm(arm, binary, &workload));
                    s.state.warmups.push(WarmupRow { session: s.state.session, arm: arm.into(), wall_ns });
                }
            }
            s.commit()?;
        }

        let mut committed_here = 0u64;
        while (s.state.pairs.len() as u64) < self.manifest.pairs {
            if s.config.stop_after_pairs.is_some_and(|limit| committed_here >= limit) {
                return Ok(ExperimentStatus::Stopped);
            }
            if s.config.cancel.is_cancelled() {
                return Ok(ExperimentStatus::Cancelled);
            }
            let index = s.state.pairs.len() as u64;
            let first_candidate = candidate_first(self.manifest.seed, index);
            s.state.pending = Some(index);
            s.commit()?;
            let order = if first_candidate {
                [("candidate", &candidate), ("baseline", &baseline)]
            } else {
                [("baseline", &baseline), ("candidate", &candidate)]
            };
            let mut walls = BTreeMap::new();
            for (arm, binary) in order {
                let (wall_ns, outputs) = step!(s.run_arm(arm, binary, &workload));
                walls.insert(arm, (wall_ns, outputs));
            }
            for arm in ["baseline", "candidate"] {
                let outputs = walls[arm].1.clone();
                s.observe(arm, outputs);
            }
            s.state.pairs.push(PairRow {
                index,
                candidate_first: first_candidate,
                baseline_ns: walls["baseline"].0,
                candidate_ns: walls["candidate"].0,
                session: s.state.session,
            });
            s.state.pending = None;
            s.commit()?;
            committed_here += 1;
        }
        self.evaluate(s)?;
        Ok(ExperimentStatus::Decided { replayed: false })
    }

    fn facts(&self, state: &ExperimentState, consumed: &[i128]) -> BTreeMap<String, CValue> {
        let context = record(
            "EvalContext",
            vec![
                ("workload_id", int(id60(&format!("workload:{}:{}", self.workload_digest, self.env_digest())))),
                ("evaluator_id", int(id60(&format!("evaluator:{}:{}", self.evaluator.meaning_id, self.manifest.evaluator_function)))),
            ],
        );
        let samples = state
            .pairs
            .iter()
            .map(|p| {
                record(
                    "PairedObs",
                    vec![
                        ("baseline", CValue::Rat { num: ExactInt::from(p.baseline_ns as i128), den: ExactInt::from(1i128) }),
                        ("candidate", CValue::Rat { num: ExactInt::from(p.candidate_ns as i128), den: ExactInt::from(1i128) }),
                    ],
                )
            })
            .collect();
        let compile: u128 = state.builds.iter().filter(|b| b.arm == "candidate").map(|b| b.build_ns).sum();
        let execute: u128 = state.pairs.iter().map(|p| p.candidate_ns).sum();
        let costs = record(
            "CostAccounting",
            vec![
                ("proposal_cost", int(self.manifest.proposal_cost.map_or(-1, i128::from))),
                ("compilation_cost", big(compile)),
                ("execution_cost", big(execute)),
            ],
        );
        let all = [
            ("baseline_outputs", int_seq(state.baseline_outputs.as_deref().unwrap_or(&[]))),
            ("candidate_outputs", int_seq(state.candidate_outputs.as_deref().unwrap_or(&[]))),
            ("outputs_stable", CValue::Bool(state.outputs_stable)),
            ("samples", seq(samples)),
            ("baseline_context", context.clone()),
            ("candidate_context", context),
            ("audit_id", int(self.audit_id())),
            ("consumed", int_seq(consumed)),
            ("costs", costs),
        ];
        all.into_iter()
            .filter(|(name, _)| self.evaluator.inputs.iter().any(|input| input == name))
            .map(|(name, value)| (name.to_string(), value))
            .collect()
    }

    fn env_digest(&self) -> String {
        let joined: Vec<String> = self.environment.iter().map(|(k, v)| format!("{k}={v}")).collect();
        digest_hex(joined.join("\n").as_bytes())
    }

    fn uses_audit(&self) -> bool {
        self.evaluator.inputs.iter().any(|input| input == "audit_id")
    }

    /// Reserve the audit (ledger `pending`), evaluate, commit the decision,
    /// then mark the audit `consumed`. A crash after the reservation leaves
    /// a pending row that other experiments already count as consumed.
    fn evaluate(&self, s: &mut Session<'_>) -> Result<(), HostFault> {
        let mut ledger = read_ledger(&s.config.audit_ledger)?;
        let audit_id = self.audit_id();
        if let Some(clash) = ledger.iter().find(|e| e.audit_id == audit_id && e.audit_digest != self.workload_digest) {
            return Err(ledger_fault(format!(
                "audit id {audit_id} is already bound to digest {}; refusing an id collision",
                clash.audit_digest
            )));
        }
        let consumed: Vec<i128> = ledger
            .iter()
            .filter(|e| !(e.experiment == self.identity && e.state == "pending"))
            .map(|e| e.audit_id)
            .collect();
        if self.uses_audit() && !ledger.iter().any(|e| e.experiment == self.identity && e.state == "pending") {
            ledger.push(LedgerEntry {
                audit_id,
                audit_digest: self.workload_digest.clone(),
                experiment: self.identity.clone(),
                state: "pending".into(),
            });
            write_ledger(&s.config.audit_ledger, &ledger)?;
        }
        let inputs = self.facts(&s.state, &consumed);
        let decision = evaluate_function_at(
            &self.evaluator.tree,
            &self.manifest.evaluator_function,
            &inputs,
            Some(&self.manifest.evaluator_module),
        )
        .map_err(|err| fault("experiment_evaluation", format!("{}: {}", err.code, err.message)))?;
        value_int(&decision, "status").map_err(|f| {
            fault("experiment_evaluation", format!("the decision has no Int `status`: {}", f.message))
        })?;
        if self.uses_audit() {
            let returned = as_seq(field(&decision, "consumed").map_err(|f| fault("experiment_evaluation", f.message))?)
                .map_err(|f| fault("experiment_evaluation", f.message))?;
            if !returned.iter().any(|id| matches!(id, CValue::Int(n) if n.to_i128() == Some(audit_id))) {
                return Err(fault(
                    "experiment_evaluation",
                    "an evaluator that declares `audit_id` must return a `consumed` history containing it",
                ));
            }
        }
        s.state.decision = Some(decision);
        s.commit()?;
        self.finish_ledger(s.config)
    }

    fn finish_ledger(&self, config: &ExperimentConfig) -> Result<(), HostFault> {
        if !self.uses_audit() {
            return Ok(());
        }
        let mut ledger = read_ledger(&config.audit_ledger)?;
        let mut changed = false;
        for entry in &mut ledger {
            if entry.experiment == self.identity && entry.state == "pending" {
                entry.state = "consumed".into();
                changed = true;
            }
        }
        if changed {
            write_ledger(&config.audit_ledger, &ledger)?;
        }
        Ok(())
    }
}

/// Display rendering of an authored value (not an interchange format; the
/// checkpoint is the machine-readable record).
#[must_use]
pub fn render_value(value: &CValue) -> String {
    match value {
        CValue::Int(n) => n.to_string(),
        CValue::Bool(b) => b.to_string(),
        CValue::Rat { num, den } => format!("{num}/{den}"),
        CValue::Str(text) => format!("{text:?}"),
        CValue::Sequence(items) => format!("[{}]", items.iter().map(render_value).collect::<Vec<_>>().join(", ")),
        CValue::Record { type_name, fields } => format!(
            "{type_name} {{{}}}",
            fields.iter().map(|(k, v)| format!("{k}: {}", render_value(v))).collect::<Vec<_>>().join(", ")
        ),
        CValue::Absent => "absent".into(),
        other => format!("{other:?}"),
    }
}
