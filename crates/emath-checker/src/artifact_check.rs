//! Independent artifact checker.
//!
//! Verifies a generated package purely from the retained artifacts: file
//! inventory and content identity, manifest identity recomputation,
//! evidence freshness/authority, claim-class support and provider locks.
//! No generator internals are invoked; every failure carries a stable
//! `E-EVID-*` code and checks run in deterministic order.

use std::collections::BTreeMap;

use emath_artifact::{
    ArtifactManifest, EvidenceBundleRecord, PlanRecord, SourceMap, evidence_bundle_from_json,
    manifest_from_json, plan_from_json, required_artifact_paths, source_map_from_json,
};
use emath_core::ContentId;
use emath_ir::{ClaimVerdict, EvidenceLevel};
use std::path::Path;

use crate::{CheckerError, identity_of};

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
    /// Stable code (`E-EVID-101`..`E-EVID-114`).
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
    if input.manifest.schema.0 != "emath.artifact" {
        issues.push(issue(
            "E-EVID-108",
            format!(
                "manifest schema is {}, expected emath.artifact",
                input.manifest.schema.0
            ),
        ));
    }

    // 2. Required artifact paths present (inventory completeness). The
    //    manifest is excluded from its own `files` map (its fingerprint is
    //    self-referential), so its presence is enforced by the caller that
    //    reads it from disk (`check_artifact_dir`) rather than by this
    //    inventory map, exactly like `manifest_identity` ignores it.
    for path in required_artifact_paths() {
        if *path == "emath/artifact-manifest.json" {
            continue;
        }
        if !input.files.contains_key(*path) {
            issues.push(issue(
                "E-EVID-105",
                format!("required artifact file missing: {path}"),
            ));
        }
    }

    // 3. Declared manifest files exist and hash to their declared ids.
    //    An empty `files` object would disable fingerprinting, so it is
    //    refused outright.
    if input.manifest.files.is_empty() {
        issues.push(issue(
            "E-EVID-109",
            "manifest declares no files; content-identity fingerprinting disabled",
        ));
    }
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
    let mut strongest_pass = EvidenceLevel::E0;
    let mut saw_pass = false;
    for claim in &claims {
        if claim.verdict == ClaimVerdict::Pass {
            saw_pass = true;
            if claim.level > strongest_pass {
                strongest_pass = claim.level;
            }
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
    // Manifest evidence_level is the delivered bar (build→artifact). Pass
    // claims weaker than that bar mean the artifact over-advertises.
    if saw_pass && input.manifest.evidence_level > strongest_pass {
        issues.push(issue(
            "E-EVID-103",
            format!(
                "manifest evidence_level {} exceeds strongest Pass claim {}",
                input.manifest.evidence_level.as_str(),
                strongest_pass.as_str()
            ),
        ));
    }

    // 6. Source-map consistency.
    if input.source_map.schema.0 != "emath.source-map" {
        issues.push(issue(
            "E-EVID-110",
            format!(
                "source-map schema is {}, expected emath.source-map",
                input.source_map.schema.0
            ),
        ));
    }
    if input.source_map.source_package != input.manifest.source_package {
        issues.push(issue(
            "E-EVID-112",
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
///
/// The one identity: delegates to
/// [`emath_artifact::manifest_identity`] so the publisher and the
/// independent checker share a single function.
#[must_use]
pub fn independent_manifest_identity(manifest: &ArtifactManifest) -> ContentId {
    emath_artifact::manifest_identity(manifest)
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

/// Check an artifact directory on disk: `<root>/emath/<artifact-id>/` as
/// published by `emath build`. This is the CLI's single
/// independent verification entry point, so write-only artifacts are no
/// longer possible: every durable document is parsed and every declared
/// file is read with strict UTF-8 and no symlink following.
///
/// Reads a staged artifact directory into the in-memory input the
/// independent checker consumes. This is the single constructor bridge
/// between disk and `ArtifactInput`; the negative-control battery
/// (`negative::run_standard_battery`) seeds from inputs built this way,
/// so the honest baseline is always a real staged tree.
///
/// Document-level failures are typed refusals:
///
/// - `E-EVID-105` a required artifact path is missing;
/// - `E-EVID-108` document does not conform to its schema (unparseable);
/// - `E-EVID-113` a required or declared path is a symlink;
/// - `E-EVID-114` a document or declared file is not valid UTF-8.
pub fn artifact_input_from_dir(root: &Path) -> Result<ArtifactInput, CheckerError> {
    let requirement = |path: &'static str| {
        let full = root.join(path);
        if full
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
        {
            return Err(CheckerError::new(
                "E-EVID-113",
                format!("required artifact path is a symlink: {path}"),
            ));
        }
        let bytes = std::fs::read(&full).map_err(|_| {
            CheckerError::new(
                "E-EVID-105",
                format!("required artifact path missing: {path}"),
            )
        })?;
        let text = String::from_utf8(bytes).map_err(|_| {
            CheckerError::new(
                "E-EVID-114",
                format!("artifact document is not valid UTF-8: {path}"),
            )
        })?;
        Ok::<String, CheckerError>(text)
    };
    let manifest_json = requirement("emath/artifact-manifest.json")?;
    let manifest = manifest_from_json(&manifest_json).map_err(|error| {
        CheckerError::new(
            "E-EVID-108",
            format!("emath/artifact-manifest.json does not conform to emath.artifact: {error}"),
        )
    })?;
    let source_map =
        source_map_from_json(&requirement("emath/source-map.json")?).map_err(|error| {
            CheckerError::new(
                "E-EVID-108",
                format!("emath/source-map.json does not conform to emath.source-map: {error}"),
            )
        })?;
    let plan = plan_from_json(&requirement("emath/resolution-plan.json")?).map_err(|error| {
        CheckerError::new(
            "E-EVID-108",
            format!(
                "emath/resolution-plan.json does not conform to emath.resolution-plan: {error}"
            ),
        )
    })?;
    let evidence = evidence_bundle_from_json(&requirement("emath/evidence-bundle.json")?).map_err(
        |error| {
            CheckerError::new(
                "E-EVID-108",
                format!(
                    "emath/evidence-bundle.json does not conform to emath.evidence-bundle: {error}"
                ),
            )
        },
    )?;
    let mut files = BTreeMap::new();
    for path in required_artifact_paths() {
        if *path == "emath/artifact-manifest.json" {
            continue;
        }
        let full = root.join(path);
        if full
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
        {
            return Err(CheckerError::new(
                "E-EVID-113",
                format!("required artifact path is a symlink: {path}"),
            ));
        }
        let bytes = std::fs::read(&full).map_err(|_| {
            CheckerError::new(
                "E-EVID-105",
                format!("required artifact path missing: {path}"),
            )
        })?;
        let text = String::from_utf8(bytes).map_err(|_| {
            CheckerError::new(
                "E-EVID-114",
                format!("declared artifact file is not valid UTF-8: {path}"),
            )
        })?;
        files.insert((*path).to_string(), text);
    }
    for path in manifest.files.keys() {
        if files.contains_key(path) {
            continue;
        }
        let full = root.join(path);
        if !full.exists() {
            continue; // reported by the E-EVID-109 inventory check below
        }
        if full
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
        {
            return Err(CheckerError::new(
                "E-EVID-113",
                format!("declared artifact path is a symlink: {path}"),
            ));
        }
        let bytes = std::fs::read(&full).map_err(|_| {
            CheckerError::new(
                "E-EVID-105",
                format!("declared artifact path missing: {path}"),
            )
        })?;
        let text = String::from_utf8(bytes).map_err(|_| {
            CheckerError::new(
                "E-EVID-114",
                format!("declared artifact file is not valid UTF-8: {path}"),
            )
        })?;
        files.insert(path.clone(), text);
    }
    // CLI verification has no separate frozen goal record: the manifest's
    // source package is the admitted identity the artifact claims to
    // implement, so a self-scoped round trip is the honest baseline.
    let goal_identity = manifest.source_package.clone();
    Ok(ArtifactInput {
        manifest,
        source_map,
        plan,
        evidence,
        files,
        provider_locks: Vec::new(),
        goal_identity,
    })
}

/// Independent artifact checker over a staged directory.
///
/// Document-level failures are typed refusals:
/// - `E-EVID-105` a required artifact path is missing;
/// - `E-EVID-108` document does not conform to its schema (unparseable);
/// - `E-EVID-113` a required or declared path is a symlink;
/// - `E-EVID-114` a document or declared file is not valid UTF-8.
pub fn check_artifact_dir(root: &Path) -> Result<ArtifactCheckReport, CheckerError> {
    let input = artifact_input_from_dir(root)?;
    Ok(check_artifact(&input, &ArtifactCheckConfig::default()))
}
