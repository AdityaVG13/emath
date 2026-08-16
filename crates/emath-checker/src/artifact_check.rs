//!: independent artifact checker.
//!
//! Verifies a generated package purely from the retained artifacts: file
//! inventory and content identity, manifest identity recomputation,
//! evidence freshness/authority, claim-class support and provider locks.
//! No generator internals are invoked; every failure carries a stable
//! `E-EVID-*` code and checks run in deterministic order.

use std::collections::BTreeMap;

use emath_artifact::{
    required_artifact_paths, ArtifactManifest, EvidenceBundleRecord, PlanRecord, SourceMap,
};
use emath_core::{fnv1a64_bytes, ContentId};
use emath_ir::ClaimVerdict;

use crate::{identity_of, CheckerError};

/// Frozen provider lock as recorded by the build environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderLockRecord {
    /// Provider id.
    pub id: String,
    /// Pinned version.
    pub version: String,
    /// Pinned implementation identity.
    pub implementation: ContentId,
}

/// One flagged problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactCheckIssue {
    /// Stable code (`E-EVID-101`..`E-EVID-111`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

/// Deterministic checker report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactCheckReport {
    /// Files whose content identity matched the manifest.
    pub files_verified: u64,
    /// Issues found, in determinististic check order.
    pub issues: Vec<ArtifactCheckIssue>,
}

impl ArtifactCheckReport {
    /// Whether every check passed.
    #[must_use]
    pub fn valid(&self) -> bool {
        self.issues.is_empty()
    }

    /// First issue, if any.
    #[must_use]
    pub fn first(&self) -> Option<&ArtifactCheckIssue> {
        self.issues.first()
    }
}

/// Checker configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArtifactCheckConfig {
    /// Claim classes the checker can independently support
    /// (`correctness`, `equivalence`, `performance`, ...).
    pub supported_claim_classes: Vec<String>,
}

/// The retained artifact under independent verification.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactInput {
    /// Sealed artifact manifest.
    pub manifest: ArtifactManifest,
    /// Source map.
    pub source_map: SourceMap,
    /// Resolution plan.
    pub plan: PlanRecord,
    /// Evidence bundle.
    pub evidence: EvidenceBundleRecord,
    /// `path -> content` inventory of the generated package.
    pub files: BTreeMap<String, String>,
    /// Provider locks from the build environment.
    pub provider_locks: Vec<ProviderLockRecord>,
    /// Frozen goal/source identity the artifact claims to implement.
    pub goal_identity: ContentId,
}

/// Independent artifact checker.
#[must_use]
pub fn check_artifact(input: &ArtifactInput, config: &ArtifactCheckConfig) -> ArtifactCheckReport {
    let mut issues = Vec::new();
    let mut files_verified = 0_u64;

    // 1. Manifest schema PIN.
    if input.manifest.schema.0 != "emath.artifact.v1" {
        issues.push(issue(
            "E-EVID-108",
            format!(
                "manifest schema is {}, expected emath.artifact.v1",
                input.manifest.schema.0
            ),
        ));
    }

    // 2. Required artifact paths present (inventory completeness).
    for path in required_artifact_paths() {
        if !input.files.contains_key(*path) {
            issues.push(issue(
                "E-EVID-105",
                format!("required artifact file missing: {path}"),
            ));
        }
    }

    // 3. Declared manifest files exist and hash to their declared ids.
    let mut declared: Vec<(&String, &ContentId)> = input.manifest.files.iter().collect();
    declared.sort_by(|left, right| left.0.cmp(right.0));
    for (path, declared_id) in declared {
        match input.files.get(path) {
            None => issues.push(issue(
                "E-EVID-109",
                format!("manifest declares {path} but no such file exists"),
            )),
            Some(content) => {
                if identity_of(content) == *declared_id {
                    files_verified += 1;
                } else {
                    issues.push(issue(
                        "E-EVID-101",
                        format!("content of {path} does not hash to its declared id"),
                    ));
                }
            }
        }
    }

    // 4. Artifact identity recomputes from the manifest body.
    let recomputed = independent_manifest_identity(&input.manifest);
    if recomputed != input.manifest.artifact_id {
        issues.push(issue(
            "E-EVID-102",
            format!(
                "artifact identity does not recompute: manifest carries {}, independent check computes {}",
                input.manifest.artifact_id.0, recomputed.0
            ),
        ));
    }

    // 5. Evidence scope and authority.
    if input.evidence.source_package != input.goal_identity {
        issues.push(issue(
            "E-EVID-103",
            "evidence bundle is not scoped to the frozen goal/source",
        ));
    }
    let mut claims = input.evidence.claims.clone();
    claims.sort_by(|left, right| left.id.cmp(&right.id));
    for claim in &claims {
        if claim.verdict == ClaimVerdict::Pass {
            if claim.checker.is_none() {
                issues.push(issue(
                    "E-EVID-107",
                    format!("resolved claim {} has no checker", claim.id),
                ));
            }
            if let Some(fresh_until) = &claim.fresh_until {
                if !fresh_until.is_empty() && fresh_until.as_str() < "2030-01-01T00:00:00Z" {
                    issues.push(issue(
                        "E-EVID-104",
                        format!(
                            "certificate for claim {} is stale (fresh until {fresh_until})",
                            claim.id
                        ),
                    ));
                }
            }
            if !config.supported_claim_classes.is_empty()
                && !config
                    .supported_claim_classes
                    .iter()
                    .any(|known| known == &claim.class)
            {
                issues.push(issue(
                    "E-EVID-106",
                    format!(
                        "claim class {} is not supported by any checker",
                        claim.class
                    ),
                ));
            }
        }
    }

    // 6. Source-map consistency.
    if input.source_map.schema.0 != "emath.source-map.v1" {
        issues.push(issue(
            "E-EVID-110",
            format!(
                "source-map schema is {}, expected emath.source-map.v1",
                input.source_map.schema.0
            ),
        ));
    }
    if input.source_map.source_package != input.manifest.source_package {
        issues.push(issue(
            "E-EVID-110",
            "source map does not reference the manifest's source package",
        ));
    }

    // 7. Provider locks cover every provider the manifest depends on.
    let mut providers = input.manifest.providers.clone();
    providers.sort_by(|left, right| left.id.cmp(&right.id));
    for provider in &providers {
        let locked = input
            .provider_locks
            .iter()
            .find(|lock| lock.id == provider.id);
        match locked {
            None => issues.push(issue(
                "E-EVID-111",
                format!("provider {} has no lock record", provider.id),
            )),
            Some(lock) => {
                if lock.version != provider.version
                    || lock.implementation != provider.implementation
                {
                    issues.push(issue(
                        "E-EVID-111",
                        format!(
                            "provider {} lock does not match the manifest dependency",
                            provider.id
                        ),
                    ));
                }
            }
        }
    }

    ArtifactCheckReport {
        files_verified,
        issues,
    }
}

/// Deterministic manifest-body identity (excludes `artifact_id` itself).
#[must_use]
pub fn independent_manifest_identity(manifest: &ArtifactManifest) -> ContentId {
    let mut files: Vec<(String, &ContentId)> = manifest
        .files
        .iter()
        .map(|(p, id)| (p.clone(), id))
        .collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let file_token: Vec<String> = files
        .iter()
        .map(|(path, id)| format!("{path}={}", id.0))
        .collect();
    let mut providers = manifest.providers.clone();
    providers.sort_by(|left, right| left.id.cmp(&right.id));
    let provider_token: Vec<String> = providers
        .iter()
        .map(|p| {
            format!(
                "{}@{}:{}-{}",
                p.id, p.version, p.implementation.0, manifest.numeric_profile
            )
        })
        .collect();
    let body = format!(
        "artifact:v1:{}:{}:{}:{}:{}:{}:[{}]:[{}]:{}:{}:{}",
        manifest.schema.0,
        manifest.source_package.0,
        manifest.class.as_str(),
        manifest.compiler.0,
        manifest.target.family,
        manifest.target.triple.as_deref().unwrap_or("-"),
        file_token.join(";"),
        provider_token.join(";"),
        manifest.source_map.0,
        manifest.resolution_plan.0,
        manifest.evidence_bundle.0,
    );
    ContentId(format!("fnv1a64:{:016x}", fnv1a64_bytes(body.as_bytes())))
}

fn issue(code: &'static str, message: impl Into<String>) -> ArtifactCheckIssue {
    ArtifactCheckIssue {
        code,
        message: message.into(),
    }
}

/// Convenience: first issue as a `CheckerError` (`E-EVID-*` passthrough).
#[must_use]
pub fn first_error(report: &ArtifactCheckReport) -> Option<CheckerError> {
    report
        .first()
        .map(|found| CheckerError::new(found.code, found.message.clone()))
}
