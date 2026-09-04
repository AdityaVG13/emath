//! emath build pipeline: `.emath` → check → plan → EMIR → Rust backend →
//! staged artifact → atomic publish → independent verification.
//!
//! `compile_direct_module` keeps the V3 build-script contract: build an
//! artifact from a spec file into an output directory. `build_text` and
//! `build_file` expose the full report for hosts and the CLI.

#![forbid(unsafe_code)]

pub mod deps;
pub mod edition;
pub mod first_cutover;
pub mod metrics;
mod probe;
pub mod publication;
pub mod script;

pub use deps::{
    CargoDependency, DepError, DepPlan, DepPolicy, DepRequest, DepSource, RuntimeKind, TargetKind,
    check_declared, plan_dependencies, requests_for,
};
pub use edition::{ManifestEditionError, manifest_edition, parse_edition_field};
pub use first_cutover::{
    CutoverError, FIRST_CUTOVER_IDS, activate_first_cutover, rollback_feature,
};
pub use metrics::{BENCHMARK_RECEIPT_SCHEMA, BENCHMARK_RECEIPT_VERSION, MetricsCollector};
pub use publication::{
    PublicationError, PublicationEvidence, PublicationMode, authority_status, publish_feature,
};
pub use script::{ScriptError, ScriptLock, ScriptReport, locked_build_script};

use emath_artifact::{
    ARTIFACT_MANIFEST_SCHEMA, ArtifactClass, ArtifactError, ArtifactManifest,
    EVIDENCE_BUNDLE_SCHEMA, EvidenceBundleRecord, PlanRecord, SOURCE_MAP_SCHEMA, SourceMap,
    SourceMapEntry, StagedFile, manifest_identity, plan_to_record, publish,
    required_artifact_paths, stage, verify_artifact, write_artifact_manifest,
    write_evidence_bundle, write_resolution_plan, write_source_map,
};
use emath_core::{Diagnostics, SchemaId, content_id_of_str};
use emath_ir::{ClaimVerdict, EvidenceClaim, EvidenceLevel, ResolutionPlan};
use emath_rust_backend::rust_ir::profiles::CrateProfile;
use emath_rust_backend::{BackendInput, BackendOutput};
use emath_sema::session::CompilerSession;
use emath_syntax::install_source_parser;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// Persistent cargo target dir for generated crates (`$CWD/target/emath-cargo/<key>`).
/// Incremental rustc survives across `emath run` / `--verify` because those
/// paths wipe their source staging dirs after each invocation.
#[must_use]
pub fn generated_crate_target_dir(key: &str) -> PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    dir.push("target");
    dir.push("emath-cargo");
    dir.push(key.replace(['/', ':'], "-"));
    dir
}

pub const COMPILER_DESCRIPTOR: &str = concat!("emath-phase1/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, Default)]
pub struct BuildOptions {
    /// Whether the staged crate is verified with `cargo test` before publish.
    pub verify_generated_crate: bool,
    /// Emit a compiled function-spec probe for this entrypoint (see
    /// `probe::`): a standalone sibling binary with the same `--set`
    /// contract as `emath eval`. `None` (default) emits no probe.
    pub bin_entrypoint: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BuildReport {
    pub artifact_dir: PathBuf,
    pub artifact_id: emath_core::ContentId,
    pub package_id: emath_core::ContentId,
    pub crate_name: String,
    pub plan_ids: Vec<String>,
    pub assumptions: Vec<String>,
    pub exports: Vec<String>,
    /// Diagnostic codes when the package is refused (empty on success).
    pub refusal_codes: Vec<String>,
    /// Compiled probe binary path when `BuildOptions::bin_entrypoint`
    /// was set (sibling of the artifact, never inside it).
    pub probe_binary: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildError {
    ReadFailed(String),
    AdmittedWithErrors(Vec<String>),
    Backend(String),
    VerifyFailed(String),
    Artifact(ArtifactError),
    Io(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadFailed(detail) => write!(f, "cannot read spec: {detail}"),
            Self::AdmittedWithErrors(codes) => {
                write!(f, "admission refused with {}", codes.join(", "))
            }
            Self::Backend(detail) => write!(f, "backend refused: {detail}"),
            Self::VerifyFailed(detail) => {
                write!(f, "generated crate verification failed: {detail}")
            }
            Self::Artifact(error) => write!(f, "artifact error: {error}"),
            Self::Io(detail) => write!(f, "io error: {detail}"),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<ArtifactError> for BuildError {
    fn from(value: ArtifactError) -> Self {
        Self::Artifact(value)
    }
}

/// V3-compatible entry point: build the spec into `output_path`, which
/// receives `<output_path>/emath/<artifact-id>/...`.
pub fn compile_direct_module(
    specification_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> Result<(), String> {
    let report = build_file(specification_path, output_path, BuildOptions::default())
        .map_err(|error| error.to_string())?;
    if !report.refusal_codes.is_empty() {
        return Err(format!(
            "admission refused: {}",
            report.refusal_codes.join(", ")
        ));
    }
    Ok(())
}

/// Full pipeline over a spec file on disk.
pub fn build_file(
    specification_path: impl AsRef<Path>,
    target_dir: impl AsRef<Path>,
    options: BuildOptions,
) -> Result<BuildReport, BuildError> {
    let path = specification_path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|error| BuildError::ReadFailed(format!("{}: {error}", path.display())))?;
    let name = path.file_name().map_or_else(
        || "spec.emath".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    build_text(&name, &text, target_dir, options)
}

/// Full pipeline over in-memory text (used by the CLI, tests and hosts).
pub fn build_text(
    name: &str,
    text: &str,
    target_dir: impl AsRef<Path>,
    options: BuildOptions,
) -> Result<BuildReport, BuildError> {
    // The session parses `.emath` text, so the source-parser backend must
    // be installed for every host that builds (CLI, build scripts,
    // builders). Idempotent.
    install_source_parser();
    let target_dir = target_dir.as_ref();
    std::fs::create_dir_all(target_dir).map_err(|error| {
        BuildError::Io(format!("cannot create {}: {error}", target_dir.display()))
    })?;

    let mut collector = MetricsCollector::new();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let file = session.load_text(name, text.to_string());
    let check_started = std::time::Instant::now();
    let plan_result = session.plan(file);
    collector.record_duration_ns(
        "check_plan",
        u64::try_from(check_started.elapsed().as_nanos()).unwrap_or(u64::MAX),
    );
    let diagnostics = plan_result.diagnostics;

    let mut refusal_codes: Vec<String> = diagnostics
        .items()
        .iter()
        .filter(|d| d.severity == emath_core::Severity::Error)
        .map(|d| d.code.to_string())
        .collect();
    refusal_codes.sort();
    refusal_codes.dedup();

    let mut package = plan_result.package;
    package.seal();

    if !refusal_codes.is_empty() {
        // Typed refusal: no artifact, no half-built crate.
        return Err(BuildError::AdmittedWithErrors(refusal_codes));
    }

    collector.record_count("plan_count", plan_result.plans.len() as u64);
    collector.record_count("diagnostics", diagnostics.items().len() as u64);
    let artifact_started = std::time::Instant::now();
    let report = build_package(
        &package,
        name,
        &diagnostics,
        &plan_result.plans,
        target_dir,
        options,
    )?;
    collector.record_duration_ns(
        "artifact_pipeline",
        u64::try_from(artifact_started.elapsed().as_nanos()).unwrap_or(u64::MAX),
    );
    collector.record_count("compile_success", 1);
    collector.record_count(
        "artifact_bytes",
        published_artifact_bytes(&report.artifact_dir),
    );

    // Benchmark receipt: sibling of the published artifact directory, never
    // inside it (the artifact's staged file set is identity-verified).
    let receipt = collector.benchmark_receipt(name, &report.artifact_id.0);
    let receipt_path = target_dir.join("benchmark-receipt.json");
    std::fs::write(&receipt_path, receipt).map_err(|error| {
        BuildError::Io(format!("cannot write {}: {error}", receipt_path.display()))
    })?;
    Ok(report)
}

/// Total bytes of the published artifact files (recursive).
fn published_artifact_bytes(artifact_dir: &Path) -> u64 {
    fn walk(dir: &Path, total: &mut u64) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, total);
            } else if let Ok(meta) = entry.metadata() {
                *total = total.saturating_add(meta.len());
            }
        }
    }
    let mut total = 0;
    walk(artifact_dir, &mut total);
    total
}

mod package;

pub use package::*;

/// build.rs helpers (`AGENT_BUILD_PROMPT`: host integration) ----------------
///
/// Emit `cargo:rerun-if-changed=<path>`; call from a build script.
pub fn emit_rerun_if_changed(path: impl AsRef<Path>) {
    println!("cargo:rerun-if-changed={}", path.as_ref().display());
}

/// Build an artifact into `$OUT_DIR` and print the artifact id for the
/// host. Returns the full report.
pub fn build_into_out_dir(
    spec_path: impl AsRef<Path>,
    out_dir: impl AsRef<Path>,
) -> Result<BuildReport, BuildError> {
    let report = build_file(
        spec_path,
        out_dir,
        BuildOptions {
            verify_generated_crate: false,
            ..BuildOptions::default()
        },
    )?;
    println!("cargo:emath-artifact-id={}", report.artifact_id.0);
    Ok(report)
}

pub mod builder;
