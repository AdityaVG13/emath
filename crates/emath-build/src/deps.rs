//!: Cargo dependency planner.
//!
//! Maps provider/runtime/features/targets to exact Cargo dependencies,
//! detects version conflicts (`E-CODEGEN-007`), forbidden sources/names
//! (`E-CODEGEN-005`) and undeclared uses (`E-CODEGEN-006`). The manifest
//! section renders deterministically; absolute path dependencies are an
//! absolute-path-leak refusal (`E-CODEGEN-009`).

use emath_ir::Goal;
use std::collections::BTreeMap;

/// Where a dependency comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepSource {
    /// Relative path dependency (`path = "../x"`).
    Path(String),
    /// Git dependency.
    Git { url: String, rev: String },
    /// Registry dependency.
    Registry,
}

/// One planned Cargo dependency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CargoDependency {
    /// Crate name.
    pub name: String,
    /// Version requirement (registry only).
    pub version_req: String,
    /// Enabled features (sorted).
    pub features: Vec<String>,
    /// Optional dependency.
    pub optional: bool,
    /// Source.
    pub source: DepSource,
}

/// A dependency request before planning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepRequest {
    /// Crate name.
    pub name: String,
    /// Version requirement.
    pub version_req: String,
    /// Enabled features.
    pub features: Vec<String>,
    /// Optional flag.
    pub optional: bool,
    /// Source.
    pub source: DepSource,
}

/// Dependency policy for the build.
#[derive(Clone, Debug)]
pub struct DepPolicy {
    /// Allow git dependencies.
    pub allow_git: bool,
    /// Allow registry dependencies.
    pub allow_registry: bool,
    /// Allow relative path dependencies.
    pub allow_path: bool,
    /// Crate names that are forbidden (e.g. upstream provider impls).
    pub forbidden_names: Vec<String>,
}

impl DepPolicy {
    /// Strict policy: relative paths only.
    #[must_use]
    pub fn strict_local() -> Self {
        Self {
            allow_git: false,
            allow_registry: false,
            allow_path: true,
            forbidden_names: vec![],
        }
    }
}

/// The planned dependency set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DepPlan {
    /// Dependencies sorted by name.
    pub dependencies: Vec<CargoDependency>,
}

impl DepPlan {
    /// Canonical manifest section.
    #[must_use]
    pub fn render_manifest(&self) -> String {
        let mut out = String::from("[dependencies]\n");
        for dependency in &self.dependencies {
            out.push_str(&render_dependency(dependency));
        }
        out
    }

    /// Whether a crate name appears in the plan.
    #[must_use]
    pub fn declares(&self, name: &str) -> bool {
        self.dependencies.iter().any(|dep| dep.name == name)
    }
}

/// Dependency planning failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepError {
    /// Stable code (`E-CODEGEN-005`..`E-CODEGEN-007`, `E-CODEGEN-009`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

impl std::fmt::Display for DepError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for DepError {}

/// Plans the dependency set from requests under the policy.
pub fn plan_dependencies(requests: &[DepRequest], policy: &DepPolicy) -> Result<DepPlan, DepError> {
    let mut by_name: BTreeMap<String, CargoDependency> = BTreeMap::new();
    for request in requests {
        match &request.source {
            DepSource::Git { .. } if !policy.allow_git => {
                return Err(DepError {
                    code: "E-CODEGEN-005",
                    message: format!("git dependency `{}` denied by policy", request.name),
                });
            }
            DepSource::Registry if !policy.allow_registry => {
                return Err(DepError {
                    code: "E-CODEGEN-005",
                    message: format!("registry dependency `{}` denied by policy", request.name),
                });
            }
            DepSource::Path(path) if !policy.allow_path => {
                return Err(DepError {
                    code: "E-CODEGEN-005",
                    message: format!("path dependency `{}` denied by policy", request.name),
                });
            }
            DepSource::Path(path) if std::path::Path::new(path).is_absolute() => {
                return Err(DepError {
                    code: "E-CODEGEN-009",
                    message: format!(
                        "absolute path dependency `{path}` for `{}` leaks host layout",
                        request.name
                    ),
                });
            }
            _ => {}
        }
        if policy
            .forbidden_names
            .iter()
            .any(|forbidden| forbidden == &request.name)
        {
            return Err(DepError {
                code: "E-CODEGEN-005",
                message: format!("crate `{}` is forbidden by policy", request.name),
            });
        }
        if let Some(existing) = by_name.get(&request.name) {
            // Conflict: different version requirements for the same crate.
            if existing.version_req != request.version_req {
                return Err(DepError {
                    code: "E-CODEGEN-007",
                    message: format!(
                        "version conflict for `{}`: {} vs {}",
                        request.name, existing.version_req, request.version_req
                    ),
                });
            }
        } else {
            let mut features = request.features.clone();
            features.sort();
            features.dedup();
            by_name.insert(
                request.name.clone(),
                CargoDependency {
                    name: request.name.clone(),
                    version_req: request.version_req.clone(),
                    features,
                    optional: request.optional,
                    source: request.source.clone(),
                },
            );
        }
    }
    Ok(DepPlan {
        dependencies: by_name.into_values().collect(),
    })
}

/// Checks that every name used by generated code is declared in the plan
/// (`E-CODEGEN-006` undeclared dependency).
pub fn check_declared(plan: &DepPlan, used_names: &[&str]) -> Result<(), DepError> {
    let mut missing: Vec<&str> = used_names
        .iter()
        .copied()
        .filter(|name| !plan.declares(name))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(DepError {
            code: "E-CODEGEN-006",
            message: format!("undeclared dependency: {}", missing.join(", ")),
        })
    }
}

/// Runtime kinds the planner maps to dependency sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeKind {
    /// Native strict-f64, std only.
    Native,
    /// Simulation runtime (needs the emath runtime crate).
    Simulation,
}

/// Target families the planner maps to features.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    /// Plain rust library.
    RustLibrary,
    /// WebAssembly component.
    WasmComponent,
    /// Provider plugin.
    ProviderPlugin,
}

impl TargetKind {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RustLibrary => "rust.library",
            Self::WasmComponent => "wasm/component",
            Self::ProviderPlugin => "provider-plugin",
        }
    }
}

/// Deterministic mapping: goal + runtime + target → dependency requests.
#[must_use]
pub fn requests_for(_goal: &Goal, runtime: RuntimeKind, target: TargetKind) -> Vec<DepRequest> {
    let mut requests = Vec::new();
    match runtime {
        RuntimeKind::Native => {}
        RuntimeKind::Simulation => requests.push(DepRequest {
            name: "emath-runtime".into(),
            version_req: String::new(),
            features: vec![],
            optional: false,
            source: DepSource::Path("crates/emath-runtime".into()),
        }),
    }
    match target {
        TargetKind::RustLibrary => {}
        TargetKind::WasmComponent => requests.push(DepRequest {
            name: "emath-wasm-bridge".into(),
            version_req: String::new(),
            features: vec!["component".into()],
            optional: false,
            source: DepSource::Path("crates/emath-wasm-bridge".into()),
        }),
        TargetKind::ProviderPlugin => requests.push(DepRequest {
            name: "emath-provider-api".into(),
            version_req: "0.1".into(),
            features: vec![],
            optional: false,
            source: DepSource::Registry,
        }),
    }
    requests.sort_by(|left, right| left.name.cmp(&right.name));
    requests
}

/// One rendered manifest line.
fn render_dependency(dependency: &CargoDependency) -> String {
    let mut attributes: Vec<String> = Vec::new();
    match &dependency.source {
        DepSource::Path(path) => attributes.push(format!("path = {path:?}")),
        DepSource::Git { url, rev } => {
            attributes.push(format!("git = {url:?}"));
            attributes.push(format!("rev = {rev:?}"));
        }
        DepSource::Registry => {
            if !dependency.version_req.is_empty() {
                attributes.push(format!("version = {:?}", dependency.version_req));
            }
        }
    }
    if !dependency.features.is_empty() {
        let features = dependency
            .features
            .iter()
            .map(|f| format!("{f:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        attributes.push(format!("features = [{features}]"));
    }
    if dependency.optional {
        attributes.push("optional = true".to_string());
    }
    if attributes.is_empty() {
        format!("{} = \"*\"\n", dependency.name)
    } else {
        format!("{} = {{ {} }}\n", dependency.name, attributes.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RuntimeKind, TargetKind};
    use emath_core::Span;
    use emath_ir::{
        DeterminismPolicy, EvidenceLevel, ExactnessPolicy, FallbackPolicy, Goal, GoalId, GoalKind,
        GoalRequirements, TargetProfile,
    };

    fn goal() -> Goal {
        Goal {
            id: GoalId(0),
            kind: GoalKind::Evaluate,
            target: "y".into(),
            expression: None,
            requirements: GoalRequirements {
                evidence: EvidenceLevel::E1,
                exactness: ExactnessPolicy::Exact,
                determinism: DeterminismPolicy::Required,
                target: TargetProfile {
                    family: "rust-library".into(),
                    triple: None,
                    features: vec![],
                },
                fallback: FallbackPolicy::NativeOnly,
                produce: "rust.library".into(),
            },
            source: Span::default(),
        }
    }

    #[test]
    fn native_runtime_plans_std_only() {
        let plan = plan_dependencies(
            &requests_for(&goal(), RuntimeKind::Native, TargetKind::RustLibrary),
            &DepPolicy::strict_local(),
        )
        .unwrap();
        assert!(plan.dependencies.is_empty());
        assert_eq!(plan.render_manifest(), "[dependencies]\n");
    }

    #[test]
    fn registry_dependency_refused_under_strict_policy() {
        let requests = requests_for(&goal(), RuntimeKind::Simulation, TargetKind::ProviderPlugin);
        let error = plan_dependencies(&requests, &DepPolicy::strict_local()).unwrap_err();
        assert_eq!(error.code, "E-CODEGEN-005");
    }

    #[test]
    fn version_conflict_is_typed_refusal() {
        let policy = DepPolicy {
            allow_git: true,
            allow_registry: true,
            allow_path: true,
            forbidden_names: vec![],
        };
        let requests = vec![
            DepRequest {
                name: "serde".into(),
                version_req: "1.0".into(),
                features: vec![],
                optional: false,
                source: DepSource::Registry,
            },
            DepRequest {
                name: "serde".into(),
                version_req: "2.0".into(),
                features: vec![],
                optional: false,
                source: DepSource::Registry,
            },
        ];
        let error = plan_dependencies(&requests, &policy).unwrap_err();
        assert_eq!(error.code, "E-CODEGEN-007");
    }

    #[test]
    fn forbidden_crate_name_refused() {
        let policy = DepPolicy {
            allow_git: true,
            allow_registry: true,
            allow_path: true,
            forbidden_names: vec!["frankenjax".into()],
        };
        let error = plan_dependencies(
            &[DepRequest {
                name: "frankenjax".into(),
                version_req: String::new(),
                features: vec![],
                optional: false,
                source: DepSource::Path("forks/frankenjax".into()),
            }],
            &policy,
        )
        .unwrap_err();
        assert_eq!(error.code, "E-CODEGEN-005");
    }

    #[test]
    fn undeclared_use_is_typed_refusal() {
        let plan = plan_dependencies(&[], &DepPolicy::strict_local()).unwrap();
        let error = check_declared(&plan, &["emath-core"]).unwrap_err();
        assert_eq!(error.code, "E-CODEGEN-006");
        let plan = plan_dependencies(
            &[DepRequest {
                name: "emath-core".into(),
                version_req: String::new(),
                features: vec![],
                optional: false,
                source: DepSource::Path("crates/emath-core".into()),
            }],
            &DepPolicy::strict_local(),
        )
        .unwrap();
        assert!(check_declared(&plan, &["emath-core"]).is_ok());
    }

    #[test]
    fn absolute_path_dep_is_an_absolute_path_leak_refusal() {
        let error = plan_dependencies(
            &[DepRequest {
                name: "x".into(),
                version_req: String::new(),
                features: vec![],
                optional: false,
                source: DepSource::Path("/Users/me/crates/x".into()),
            }],
            &DepPolicy::strict_local(),
        )
        .unwrap_err();
        assert_eq!(error.code, "E-CODEGEN-009");
    }

    #[test]
    fn manifest_renders_deterministically() {
        let requests = requests_for(&goal(), RuntimeKind::Simulation, TargetKind::WasmComponent);
        let policy = DepPolicy {
            allow_git: true,
            allow_registry: true,
            allow_path: true,
            forbidden_names: vec![],
        };
        let plan = plan_dependencies(&requests, &policy).unwrap();
        let first = plan.render_manifest();
        assert_eq!(plan.render_manifest(), first);
        assert!(first.contains("emath-runtime"));
        assert!(first.contains("emath-wasm-bridge"));
        assert!(first.contains("features = [\"component\"]"));
    }
}
