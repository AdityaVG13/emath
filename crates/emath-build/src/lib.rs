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
    CutoverError, FIRST_CUTOVER_CONFORMANCE_CASES, FIRST_CUTOVER_IDS, activate_first_cutover,
    rollback_feature,
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
use emath_rust_backend::constructor_crate::ConstructorCrateEmission;
use emath_rust_backend::rust_ir::profiles::CrateProfile;
use emath_rust_backend::{BackendInput, BackendOutput};
use emath_sema::session::{CompilerSession, SourcePackage};
use emath_syntax::install_source_parser;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// Persistent cargo target dir for generated crates (`$CWD/target/emath-cargo/<key>`).
/// Incremental rustc survives across `emath test` / `--verify` because those
/// paths wipe their source staging dirs after each invocation.
#[must_use]
pub fn generated_crate_target_dir(key: &str) -> PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    dir.push("target");
    dir.push("emath-cargo");
    dir.push(key.replace(['/', ':'], "-"));
    dir
}
/// Write a generated source or manifest without invalidating unchanged Cargo inputs.
pub fn write_generated_file(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> std::io::Result<()> {
    let path = path.as_ref();
    let contents = contents.as_ref();
    if std::fs::read(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    std::fs::write(path, contents)
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
    let (source, emission) = prepare_constructor_file(specification_path.as_ref())?;
    let _ = options;
    write_constructor_crate(&source.text, target_dir.as_ref(), emission)
}

fn prepare_constructor_file(
    path: &Path,
) -> Result<(SourcePackage, ConstructorCrateEmission), BuildError> {
    install_source_parser();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let source = session.load_package(path).map_err(BuildError::ReadFailed)?;
    let checked = session.check(source.file);
    let emission = emit_constructor_text(&source.name, &source.text, checked)?;
    Ok((source, emission))
}

/// Full pipeline over in-memory text (used by the CLI, tests and hosts).
pub fn build_text(
    name: &str,
    text: &str,
    target_dir: impl AsRef<Path>,
    _options: BuildOptions,
) -> Result<BuildReport, BuildError> {
    // The session parses `.emath` text, so the source-parser backend must
    // be installed for every host that builds (CLI, build scripts,
    // builders). Idempotent.
    install_source_parser();
    let target_dir = target_dir.as_ref();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let checked = session.check_owned(name, text);
    build_constructor_text(name, text, target_dir, checked)
}

fn build_constructor_text(
    name: &str,
    text: &str,
    target_dir: &Path,
    checked: emath_sema::admit::CheckResult,
) -> Result<BuildReport, BuildError> {
    let emission = emit_constructor_text(name, text, checked)?;
    write_constructor_crate(text, target_dir, emission)
}

fn emit_constructor_text(
    name: &str,
    text: &str,
    checked: emath_sema::admit::CheckResult,
) -> Result<ConstructorCrateEmission, BuildError> {
    if checked.diagnostics.has_errors() {
        let mut codes: Vec<String> = checked
            .diagnostics
            .items()
            .iter()
            .filter(|item| item.severity == emath_core::Severity::Error)
            .map(|item| item.code.to_string())
            .collect();
        codes.sort();
        codes.dedup();
        return Err(BuildError::AdmittedWithErrors(codes));
    }
    let module = checked
        .prepared
        .ok_or_else(|| BuildError::AdmittedWithErrors(vec!["E-KIND-GONE".into()]))?;
    emath_rust_backend::constructor_crate::emit_constructor_crate(&module, Path::new(name), text)
        .map_err(|refusal| match refusal {
            emath_rust_backend::constructor_crate::ConstructorEmitRefusal::NotConstructor(_) => {
                BuildError::AdmittedWithErrors(vec!["E-KIND-GONE".into()])
            }
            emath_rust_backend::constructor_crate::ConstructorEmitRefusal::ImportsRefused {
                e_code,
                ..
            } => BuildError::AdmittedWithErrors(vec![e_code]),
        })
}

fn write_constructor_crate(
    text: &str,
    target_dir: &Path,
    emission: ConstructorCrateEmission,
) -> Result<BuildReport, BuildError> {
    for (relative, contents) in emission.files() {
        let path = target_dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                BuildError::Io(format!("cannot create {}: {error}", parent.display()))
            })?;
        }
        write_generated_file(&path, contents.as_bytes())
            .map_err(|error| BuildError::Io(format!("cannot write {}: {error}", path.display())))?;
    }
    let artifact_id = content_id_of_str(&emission.lib);
    let package_id = content_id_of_str(text);
    Ok(BuildReport {
        artifact_dir: target_dir.to_path_buf(),
        artifact_id,
        package_id,
        crate_name: emission.package_name,
        plan_ids: Vec::new(),
        assumptions: emission.unresolved,
        exports: emission.functions,
        refusal_codes: Vec::new(),
        probe_binary: None,
    })
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
