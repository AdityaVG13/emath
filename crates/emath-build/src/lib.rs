//! emath build pipeline: `.emath` → check → plan → EMIR → Rust backend →
//! staged artifact → atomic publish → independent verification.
//!
//! `compile_direct_module` keeps the V3 build-script contract: build an
//! artifact from a spec file into an output directory. `build_text` and
//! `build_file` expose the full report for hosts and the CLI.

#![forbid(unsafe_code)]

pub mod deps;
pub mod script;

pub use deps::{
    check_declared, plan_dependencies, requests_for, CargoDependency, DepError, DepPlan, DepPolicy,
    DepRequest, DepSource, RuntimeKind, TargetKind,
};
pub use script::{locked_build_script, ScriptError, ScriptLock, ScriptReport};

use emath_artifact::{
    plan_to_record, publish, required_artifact_paths, stage, verify_artifact,
    write_artifact_manifest, write_evidence_bundle, write_resolution_plan, write_source_map,
    ArtifactClass, ArtifactError, ArtifactManifest, EvidenceBundleRecord, PlanRecord, SourceMap,
    SourceMapEntry, StagedFile, ARTIFACT_MANIFEST_SCHEMA, EVIDENCE_BUNDLE_SCHEMA,
    SOURCE_MAP_SCHEMA,
};
use emath_core::{content_id_of_str, Diagnostics, SchemaId};
use emath_ir::{ClaimVerdict, EvidenceClaim, EvidenceLevel, ResolutionPlan};
use emath_rust_backend::{BackendInput, BackendOutput};
use emath_sema::session::CompilerSession;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const COMPILER_DESCRIPTOR: &str = concat!("emath-phase1/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Copy, Debug, Default)]
pub struct BuildOptions {
    /// Whether the staged crate is verified with `cargo test` before publish.
    pub verify_generated_crate: bool,
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
    let target_dir = target_dir.as_ref();
    std::fs::create_dir_all(target_dir).map_err(|error| {
        BuildError::Io(format!("cannot create {}: {error}", target_dir.display()))
    })?;

    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let file = session.load_text(name, text.to_string());
    let plan_result = session.plan(file);
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

    build_package(
        &package,
        name,
        &diagnostics,
        &plan_result.plans,
        target_dir,
        options,
    )
}

///: artifact pipeline over an already-elaborated package
/// (programmatic models and macro-expanded sources use this exact path:
/// same schema/sema/plan/artifact flow as `.emath` text).
pub fn build_package(
    package: &emath_ir::SemanticPackage,
    source_name: &str,
    diagnostics: &Diagnostics,
    plans: &[ResolutionPlan],
    target_dir: &Path,
    options: BuildOptions,
) -> Result<BuildReport, BuildError> {
    std::fs::create_dir_all(target_dir).map_err(|error| {
        BuildError::Io(format!("cannot create {}: {error}", target_dir.display()))
    })?;
    let package_id = package.content_id();
    let crate_name = package
        .identity
        .as_ref()
        .map_or_else(|| "package".to_string(), |id| id.name.clone());
    let backend = BackendInput {
        package,
        crate_name: crate_name.clone(),
        version: "0.1.0".to_string(),
    };
    let output = backend
        .generate()
        .map_err(|error| BuildError::Backend(error.to_string()))?;

    // --- verification lane -----------------------------------------------
    let verify_ok = if options.verify_generated_crate {
        match verify_crate(&target_dir.join("verify"), &output) {
            Ok(()) => true,
            Err(error) => return Err(BuildError::VerifyFailed(error)),
        }
    } else {
        false
    };

    let meta = ComposeMeta {
        source_name,
        crate_name: &crate_name,
        package_id: &package_id,
    };
    let mut artifact = compose_artifact(&meta, package, plans, &output, diagnostics, verify_ok)?;

    // Stage: provisional manifest, then final with the real artifact id.
    let mut provisional = artifact.clone();
    provisional.manifest.artifact_id = emath_core::ContentId(String::new());
    let provisional_files = stage_files(&output, &artifact, &provisional.manifest);
    let provisional_staging = stage(&provisional_files, None)?;
    let artifact_id = provisional_staging.artifact_id.clone();
    artifact.manifest.artifact_id = artifact_id.clone();
    let final_files = stage_files(&output, &artifact, &artifact.manifest);
    let final_staging = stage(&final_files, None)?;
    if final_staging.artifact_id != artifact_id {
        return Err(BuildError::Io(
            "internal: artifact identity changed across manifest staging".to_string(),
        ));
    }
    let destination = publish(target_dir, &artifact_id, &final_files)?;
    verify_artifact(&destination, &final_staging)?;

    Ok(BuildReport {
        artifact_dir: destination,
        package_id,
        artifact_id,
        crate_name,
        plan_ids: plans.iter().map(|p| p.plan_id.0.clone()).collect(),
        assumptions: output.assumptions.clone(),
        exports: artifact.manifest.public_exports.clone(),
        refusal_codes: Vec::new(),
    })
}

#[derive(Clone)]
struct ComposedArtifact {
    manifest: ArtifactManifest,
    source_map: SourceMap,
    plans: Vec<PlanRecord>,
    evidence: EvidenceBundleRecord,
}

/// Identity/name context shared by the artifact documents.
struct ComposeMeta<'a> {
    source_name: &'a str,
    crate_name: &'a str,
    package_id: &'a emath_core::ContentId,
}

/// Build the four durable JSON documents from the pipeline outputs.
fn compose_artifact(
    meta: &ComposeMeta<'_>,
    package: &emath_ir::SemanticPackage,
    plans: &[ResolutionPlan],
    output: &BackendOutput,
    diagnostics: &Diagnostics,
    verification_ran: bool,
) -> Result<ComposedArtifact, BuildError> {
    let declaration = package
        .declarations
        .first()
        .ok_or_else(|| BuildError::Backend("package has no declarations".to_string()))?;

    let target = package.goals.first().map_or_else(
        || emath_ir::TargetProfile {
            family: "rust-library".to_string(),
            triple: None,
            features: vec![],
        },
        |goal| goal.requirements.target.clone(),
    );
    let evidence_level = package
        .goals
        .first()
        .map_or(EvidenceLevel::E1, |goal| goal.requirements.evidence);
    let public_exports: Vec<String> = declaration.exports.iter().map(|e| e.name.clone()).collect();

    let source_map = SourceMap {
        schema: SchemaId(SOURCE_MAP_SCHEMA.to_string()),
        source_package: meta.package_id.clone(),
        entries: output
            .anchors
            .iter()
            .map(|anchor| SourceMapEntry {
                source_file: meta.source_name.to_string(),
                source_start: u64::from(declaration.source.start),
                source_end: u64::from(declaration.source.end),
                semantic_node: anchor.label.clone(),
                plan_node: None,
                generated_file: anchor.file.clone(),
                generated_start: u64::from(anchor.start),
                generated_end: u64::from(anchor.end),
                generated_symbol: anchor_label_symbol(&anchor.label),
            })
            .collect(),
    };
    let plans_recorded: Vec<PlanRecord> = plans.iter().map(plan_to_record).collect();

    // Claims: honest about what ran. Unverified steps are `not-run`.
    let mut claims = vec![EvidenceClaim {
        id: format!("{}.admitted", meta.package_id.0),
        statement: format!("`{0}` was admitted without errors", meta.crate_name),
        class: "static-semantics".to_string(),
        scope: meta.crate_name.to_string(),
        assumptions: vec!["strict-f64".to_string()],
        producer: COMPILER_DESCRIPTOR.to_string(),
        checker: Some("emath-sema/admit".to_string()),
        verdict: if diagnostics.has_errors() {
            ClaimVerdict::Fail
        } else {
            ClaimVerdict::Pass
        },
        level: EvidenceLevel::E1,
        falsifiers: vec![],
        artifacts: vec!["emath/resolution-plan.json".to_string()],
        fresh_until: None,
    }];
    let verification_claim = EvidenceClaim {
        id: format!("{}.generated-crate", meta.package_id.0),
        statement: format!(
            "the generated `{0}` crate verifies: deterministic content ids, forbidden unsafe",
            meta.crate_name,
        ),
        class: "codegen".to_string(),
        scope: meta.crate_name.to_string(),
        assumptions: output.assumptions.clone(),
        producer: COMPILER_DESCRIPTOR.to_string(),
        checker: Some(
            if verification_ran {
                "cargo-test"
            } else {
                "emath-artifact/verify"
            }
            .to_string(),
        ),
        verdict: if verification_ran {
            ClaimVerdict::Pass
        } else {
            ClaimVerdict::NotRun
        },
        level: EvidenceLevel::E3,
        falsifiers: vec![],
        artifacts: vec![
            "emath/artifact-manifest.json".to_string(),
            "emath/source-map.json".to_string(),
        ],
        fresh_until: None,
    };
    claims.push(verification_claim);

    Ok(ComposedArtifact {
        manifest: ArtifactManifest {
            schema: SchemaId(ARTIFACT_MANIFEST_SCHEMA.to_string()),
            artifact_id: emath_core::ContentId(String::new()), // filled by caller
            class: ArtifactClass::Native,
            source_package: meta.package_id.clone(),
            compiler: content_id_of_str(COMPILER_DESCRIPTOR),
            target,
            numeric_profile: "strict-f64".to_string(),
            providers: Vec::new(), // Phase 1: provider-free
            evidence_level,
            public_exports,
            assumptions: output.assumptions.clone(),
            files: BTreeMap::new(), // filled by stage_files
            source_map: emath_core::ContentId(String::new()),
            resolution_plan: emath_core::ContentId(String::new()),
            evidence_bundle: emath_core::ContentId(String::new()),
        },
        source_map,
        plans: plans_recorded,
        evidence: EvidenceBundleRecord {
            schema: SchemaId(EVIDENCE_BUNDLE_SCHEMA.to_string()),
            bundle_id: emath_core::ContentId(String::new()),
            source_package: meta.package_id.clone(),
            resolution_plan: emath_core::ContentId(String::new()),
            claims,
            artifact_paths: required_artifact_paths()
                .iter()
                .map(|p| (*p).to_string())
                .collect(),
            reproduction: vec![
                "emath build <spec> --out <dir>".to_string(),
                COMPILER_DESCRIPTOR.to_string(),
            ],
        },
    })
}

fn anchor_label_symbol(label: &str) -> Option<String> {
    let parts: Vec<&str> = label.split(' ').collect();
    if parts.len() == 2 && matches!(parts[0], "fn" | "test" | "struct" | "impl") {
        Some(parts[1].to_string())
    } else {
        None
    }
}

fn stage_files(
    output: &BackendOutput,
    artifact: &ComposedArtifact,
    manifest: &ArtifactManifest,
) -> Vec<StagedFile> {
    let mut files: Vec<StagedFile> = output
        .files
        .iter()
        .map(|(path, text)| StagedFile {
            relative_path: path.clone(),
            bytes: text.as_bytes().to_vec(),
        })
        .collect();

    let source_map_text = write_source_map(&artifact.source_map);
    files.push(StagedFile {
        relative_path: "emath/source-map.json".to_string(),
        bytes: source_map_text.as_bytes().to_vec(),
    });
    let plan_text = serialize_plans(&artifact.plans);
    files.push(StagedFile {
        relative_path: "emath/resolution-plan.json".to_string(),
        bytes: plan_text.as_bytes().to_vec(),
    });
    let evidence_text = write_evidence_bundle(&artifact.evidence);
    files.push(StagedFile {
        relative_path: "emath/evidence-bundle.json".to_string(),
        bytes: evidence_text.as_bytes().to_vec(),
    });

    // ids for the documents the manifest references
    let source_map_id = content_id_of_str(&source_map_text);
    let plan_id = content_id_of_str(&plan_text);
    let evidence_id = content_id_of_str(&evidence_text);

    // The manifest references every other file by fingerprint. The manifest
    // itself is excluded from its own `files` map (self-referential ids are
    // unstable), so this resolves in one pass.
    let mut resolved = manifest.clone();
    resolved.files = output
        .files
        .iter()
        .map(|(path, text)| (path.clone(), content_id_of_str(text)))
        .chain(std::iter::once((
            "emath/source-map.json".to_string(),
            source_map_id.clone(),
        )))
        .chain(std::iter::once((
            "emath/resolution-plan.json".to_string(),
            plan_id.clone(),
        )))
        .chain(std::iter::once((
            "emath/evidence-bundle.json".to_string(),
            evidence_id.clone(),
        )))
        .collect();
    resolved.source_map = source_map_id;
    resolved.resolution_plan = plan_id;
    resolved.evidence_bundle = evidence_id;
    let final_manifest_text = write_artifact_manifest(&resolved);
    files.push(StagedFile {
        relative_path: "emath/artifact-manifest.json".to_string(),
        bytes: final_manifest_text.as_bytes().to_vec(),
    });
    files
}

fn serialize_plans(plans: &[PlanRecord]) -> String {
    let mut out = String::new();
    for (index, plan) in plans.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&write_resolution_plan(plan));
    }
    out
}

/// Write the generated crate into `dir` and run `cargo test --quiet`.
fn verify_crate(dir: &Path, output: &BackendOutput) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    for (path, text) in &output.files {
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(target, text).map_err(|error| error.to_string())?;
    }
    let result = std::process::Command::new("cargo")
        .arg("test")
        .arg("--quiet")
        .current_dir(dir)
        .output()
        .map_err(|error| format!("cannot spawn cargo: {error}"))?;
    if !result.status.success() {
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!(
            "cargo test exited {:?}\n{stdout}\n{stderr}",
            result.status.code()
        ));
    }
    Ok(())
}

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
        },
    )?;
    println!("cargo:emath-artifact-id={}", report.artifact_id.0);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r"emath custom <Square> as function:
    inputs:
        x: Float64

    outputs:
        y: Float64

    definitions:
        y = x * x

    requests:
        evaluate <y>:
            produce rust.library

    tests:
        example <three_squared>:
            given x = 3
            expect y == 9

    compile:
        target rust
        profile library
        numeric strict-f64
";

    const STATEFUL: &str = r"emath custom <AffinePolicy> as policy:
    inputs:
        x: Float64

    outputs:
        score: Float64

    state:
        scale: Float64
        bias: Float64

    constructors:
        public fn new(scale: Float64, bias: Float64) -> Result<Self, ConfigError>:
            require scale >= 0
            require is_finite(scale)
            require is_finite(bias)

            Self:
                scale = scale
                bias = bias

    definitions:
        score = state.scale * x + state.bias

    requests:
        evaluate <score>:
            produce rust.library

    exports:
        public constructor new
        public function score

    compile:
        target rust
        profile library
        numeric strict-f64
        safety forbid-unsafe
";

    fn tmpdir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("emath-build-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn square_builds_deterministically() {
        let dir = tmpdir("square");
        let first = build_text(
            "minimal.emath",
            MINIMAL,
            dir.join("a"),
            BuildOptions::default(),
        )
        .unwrap();
        let second = build_text(
            "minimal.emath",
            MINIMAL,
            dir.join("b"),
            BuildOptions::default(),
        )
        .unwrap();
        assert_eq!(first.artifact_id, second.artifact_id);
        assert!(first
            .artifact_dir
            .join("emath/artifact-manifest.json")
            .is_file());
        // The generated crate is byte-identical across runs.
        let a = std::fs::read(first.artifact_dir.join("src/lib.rs")).unwrap();
        let b = std::fs::read(second.artifact_dir.join("src/lib.rs")).unwrap();
        assert_eq!(a, b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn affine_policy_builds_and_verifies() {
        let dir = tmpdir("affine");
        let report = build_text(
            "stateful.emath",
            STATEFUL,
            dir.join("out"),
            BuildOptions {
                verify_generated_crate: true,
            },
        )
        .unwrap();
        assert!(report.refusal_codes.is_empty());
        let lib = std::fs::read_to_string(report.artifact_dir.join("src/lib.rs")).unwrap();
        assert!(lib.contains("pub fn new(scale: f64, bias: f64)"));
        assert!(lib.contains("pub fn score(&self, x: f64) -> f64"));
        assert!(lib.contains("FailedPrecondition"));
        assert!(lib.contains("#![forbid(unsafe_code)]"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refused_package_returns_codes() {
        let dir = tmpdir("refused");
        let source = r"emath custom <Dup> as function:
    inputs:
        x: Float64
        x: Float64
    outputs:
        y: Float64
    definitions:
        y = x
";
        let error = build_text(
            "dup.emath",
            source,
            dir.join("out"),
            BuildOptions::default(),
        )
        .unwrap_err();
        match error {
            BuildError::AdmittedWithErrors(codes) => {
                assert!(codes.iter().any(|code| code == "E-NAME-020"));
            }
            other => panic!("expected refusal, got {other}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
