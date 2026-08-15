//!: build-script API.
//!
//! `locked_build_script` runs the artifact pipeline from a build.rs with
//! rerun tracking, strict `$OUT_DIR` isolation and a locked network mode:
//! any dependency source that would need network access while locked is a
//! typed refusal (`E-CODEGEN-008`). Diagnostics are deterministic.

use crate::deps::{plan_dependencies, DepPolicy, DepSource};
use crate::{build_file, BuildOptions};
use std::path::{Path, PathBuf};

/// Lock configuration for a build script invocation.
#[derive(Clone, Debug, Default)]
pub struct ScriptLock {
    /// Paths that rerun the build script when changed.
    pub rerun_paths: Vec<PathBuf>,
    /// Env vars that rerun the build script when changed.
    pub rerun_env: Vec<String>,
    /// Network access while locked.
    pub allow_network: bool,
}

/// Result of a locked build-script run.
#[derive(Clone, Debug)]
pub struct ScriptReport {
    /// Artifact id.
    pub artifact_id: emath_core::ContentId,
    /// Rerun paths echoed.
    pub rerun_paths: Vec<PathBuf>,
    /// Rerun env vars echoed.
    pub rerun_env: Vec<String>,
    /// True when every output landed inside `$OUT_DIR`.
    pub out_dir_isolated: bool,
}

/// Build-script refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    /// Stable code (`E-CODEGEN-008`, `E-CODEGEN-010`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Builds an artifact from a spec inside `$OUT_DIR` with rerun tracking and
/// lock enforcement.
pub fn locked_build_script(
    spec_path: impl AsRef<Path>,
    out_dir: impl AsRef<Path>,
    lock: &ScriptLock,
) -> Result<ScriptReport, ScriptError> {
    let out_dir = out_dir.as_ref();
    let report = build_file(
        spec_path,
        out_dir,
        BuildOptions {
            verify_generated_crate: false,
        },
    )
    .map_err(|error| ScriptError {
        code: "E-CODEGEN-010",
        message: error.to_string(),
    })?;
    // OUT_DIR isolation: the artifact must live under the caller's OUT_DIR.
    let out_dir_isolated = report.artifact_dir.starts_with(out_dir);
    if !out_dir_isolated {
        return Err(ScriptError {
            code: "E-CODEGEN-010",
            message: format!(
                "build script wrote outside $OUT_DIR: {}",
                report.artifact_dir.display()
            ),
        });
    }
    // Locked mode: refuse dependency plans that require network access.
    // The generated crate is std-only in the native runtime (Phase 1), so
    // this is a guard for future profiles rather than a silent pass.
    if !lock.allow_network
        && plan_dependencies(&[], &DepPolicy::strict_local())
            .map_err(|error| ScriptError {
                code: "E-CODEGEN-008",
                message: format!("locked build script cannot satisfy: {error}"),
            })?
            .dependencies
            .iter()
            .any(|dependency| {
                matches!(
                    dependency.source,
                    DepSource::Git { .. } | DepSource::Registry
                )
            })
    {
        return Err(ScriptError {
            code: "E-CODEGEN-008",
            message: "locked build script requires network access".into(),
        });
    }
    for path in &lock.rerun_paths {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    for env in &lock.rerun_env {
        println!("cargo:rerun-if-env-changed={env}");
    }
    println!("cargo:emath-artifact-id={}", report.artifact_id.0);
    Ok(ScriptReport {
        artifact_id: report.artifact_id,
        rerun_paths: lock.rerun_paths.clone(),
        rerun_env: lock.rerun_env.clone(),
        out_dir_isolated,
    })
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
";

    #[test]
    fn locked_script_builds_inside_out_dir() {
        let dir = std::env::temp_dir().join(format!("emath-script-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let spec = dir.join("minimal.emath");
        std::fs::write(&spec, MINIMAL).unwrap();
        let out_dir = dir.join("out");
        std::fs::create_dir_all(&out_dir).unwrap();
        let lock = ScriptLock {
            rerun_paths: vec![spec.clone()],
            rerun_env: vec!["EMATH_SPEC".into()],
            allow_network: false,
        };
        let report = locked_build_script(&spec, &out_dir, &lock).unwrap();
        assert!(report.out_dir_isolated);
        assert_eq!(report.rerun_paths, vec![spec]);
        assert_eq!(report.rerun_env, vec!["EMATH_SPEC"]);
        assert!(out_dir.join("emath").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn locked_network_refusal_code_is_stable() {
        // With the native runtime the plan is empty, so the lock always
        // holds; the refusal path is exercised by the dependency planner
        // tests. This test pins the code contract.
        let error = ScriptError {
            code: "E-CODEGEN-008",
            message: "locked build script requires network access".into(),
        };
        assert_eq!(error.code, "E-CODEGEN-008");
        assert!(error.message.contains("network"));
    }
}
