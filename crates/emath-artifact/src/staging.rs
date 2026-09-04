//! Artifact staging, verification, and publishing.

use super::*;

/// One staged file: relative path + bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedFile {
    pub relative_path: String,
    pub bytes: Vec<u8>,
}

/// Result of staging: per-file bootstrap content ids plus the artifact id
/// (bootstrap fingerprint over the required set, in required order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staging {
    pub files: BTreeMap<String, ContentId>,
    pub artifact_id: ContentId,
}

impl Staging {
    #[must_use]
    pub fn content_id(&self, relative_path: &str) -> Option<&ContentId> {
        self.files.get(relative_path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactError {
    MissingRequiredPath(String),
    UnstagedFile(String),
    StateDirMissing(PathBuf),
    VerificationMismatch(String),
    ManifestMalformed(String),
    InvalidStagedPath(String),
}

impl std::fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRequiredPath(path) => write!(f, "missing required artifact path `{path}`"),
            Self::UnstagedFile(path) => write!(f, "file was not staged: `{path}`"),
            Self::StateDirMissing(path) => write!(
                f,
                "artifact state directory is missing: `{}`",
                path.display()
            ),
            Self::VerificationMismatch(detail) => {
                write!(f, "artifact verification failed: {detail}")
            }
            Self::ManifestMalformed(detail) => {
                write!(f, "artifact manifest is malformed: {detail}")
            }
            Self::InvalidStagedPath(path) => {
                write!(f, "refusing unsafe staged path `{path}`")
            }
        }
    }
}

impl std::error::Error for ArtifactError {}

/// Whether `path` exists and is a symlink (publish and verify
/// refuse to follow links, so a link cannot smuggle files in or out of
/// the artifact destination).
pub(super) fn path_is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

/// Refuses absolute staged paths and `..` traversal components: every
/// staged path must stay inside the artifact destination.
pub(super) fn check_relative_path(relative_path: &str) -> Result<(), ArtifactError> {
    let path = std::path::Path::new(relative_path);
    if path.is_absolute() {
        return Err(ArtifactError::InvalidStagedPath(relative_path.to_string()));
    }
    for component in path.components() {
        if component == std::path::Component::ParentDir {
            return Err(ArtifactError::InvalidStagedPath(relative_path.to_string()));
        }
    }
    Ok(())
}

/// Stage files: compute ids, check the required set, derive the artifact id.
pub fn stage(files: &[StagedFile], path_filter: Option<&Path>) -> Result<Staging, ArtifactError> {
    let mut ids = BTreeMap::new();
    for file in files {
        check_relative_path(&file.relative_path)?;
        if let Some(filter) = path_filter {
            if Path::new(&file.relative_path).starts_with(filter) {
                ids.insert(
                    file.relative_path.clone(),
                    bootstrap_content_id(&file.bytes),
                );
            }
        } else {
            ids.insert(
                file.relative_path.clone(),
                bootstrap_content_id(&file.bytes),
            );
        }
    }
    for required in required_artifact_paths() {
        if !ids.contains_key(*required) {
            return Err(ArtifactError::MissingRequiredPath((*required).to_string()));
        }
    }
    // Artifact identity: fingerprint of the required paths in fixed order,
    // excluding the manifest itself (the manifest records that identity).
    let mut canonical = Vec::new();
    for required in required_artifact_paths() {
        if *required == "emath/artifact-manifest.json" {
            continue;
        }
        let id = &ids[*required];
        canonical.extend_from_slice(format!("{required}={}\n", id.0).as_bytes());
    }
    let artifact_id = bootstrap_content_id(&canonical);
    Ok(Staging {
        files: ids,
        artifact_id,
    })
}

/// Verify a staged artifact on disk: every required path exists and matches
/// its staged fingerprint. The checker is independent: it only reads the
/// files and the ids, never generator internals.
pub fn verify_artifact(root: &Path, expected: &Staging) -> Result<(), ArtifactError> {
    for required in required_artifact_paths() {
        let path = root.join(required);
        if path_is_symlink(&path) {
            return Err(ArtifactError::VerificationMismatch(format!(
                "`{required}` is a symlink"
            )));
        }
        if !path.is_file() {
            return Err(ArtifactError::MissingRequiredPath((*required).to_string()));
        }
        let bytes = std::fs::read(&path).map_err(|error| {
            ArtifactError::VerificationMismatch(format!(
                "cannot read `{}`: {error}",
                path.display()
            ))
        })?;
        let actual = bootstrap_content_id(&bytes);
        let expected_id = expected
            .content_id(required)
            .ok_or_else(|| ArtifactError::UnstagedFile((*required).to_string()))?;
        if actual != *expected_id {
            return Err(ArtifactError::VerificationMismatch(format!(
                "`{required}` fingerprint changed (content-identity mismatch)"
            )));
        }
    }
    Ok(())
}

/// Publish: create `target/emath/<artifact-id>` and write the staged files.
/// The destination is created atomically (temporary sibling dir, post-write
/// verification, rename); verification runs before and after the write.
pub fn publish(
    target_dir: &Path,
    artifact_id: &ContentId,
    files: &[StagedFile],
) -> Result<PathBuf, ArtifactError> {
    if !target_dir.is_dir() {
        return Err(ArtifactError::StateDirMissing(target_dir.to_path_buf()));
    }
    let staging = stage(files, None)?;
    let destination = target_dir.join("emath").join(&artifact_id.0);
    if path_is_symlink(target_dir) || path_is_symlink(&target_dir.join("emath")) {
        return Err(ArtifactError::VerificationMismatch(
            "refusing to publish through a symlinked state directory".to_string(),
        ));
    }
    if destination.exists() {
        // Idempotent republish: same artifact id means the content identity
        // is fixed; re-verify instead of overwriting. A verification
        // failure here is tamper/corruption, not a rebuild collision: the
        // typed mismatch is returned, never collapsed into "target exists".
        if verify_artifact(&destination, &staging).is_ok() {
            return Ok(destination);
        }
        return Err(ArtifactError::VerificationMismatch(format!(
            "existing artifact at `{}` failed content-identity verification (tampered or corrupted; not a rebuild)",
            destination.display()
        )));
    }
    // Atomic publish: everything is written under a temporary sibling
    // directory and renamed into place only after post-write verification
    // succeeds. A failure or crash leaves no destination directory and a
    // retry starts from a clean slate.
    let emath_root = target_dir.join("emath");
    let staging_dir = emath_root.join(format!(".tmp-{}", artifact_id.0));
    let _ = std::fs::remove_dir_all(&staging_dir);
    if let Err(error) = std::fs::create_dir_all(&staging_dir) {
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(ArtifactError::VerificationMismatch(format!(
            "cannot create staging `{}`: {error}",
            staging_dir.display(),
        )));
    }
    for file in files {
        // We stage the union; only write files that belong to the artifact.
        let Some(id) = staging.content_id(&file.relative_path) else {
            let _ = std::fs::remove_dir_all(&staging_dir);
            return Err(ArtifactError::UnstagedFile(file.relative_path.clone()));
        };
        let _ = id;
        let path = staging_dir.join(&file.relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                let _ = std::fs::remove_dir_all(&staging_dir);
                ArtifactError::VerificationMismatch(format!(
                    "cannot create `{}`: {error}",
                    parent.display(),
                ))
            })?;
        }
        if path_is_symlink(&path) {
            let _ = std::fs::remove_dir_all(&staging_dir);
            return Err(ArtifactError::VerificationMismatch(format!(
                "refusing to write through symlink `{}`",
                path.display(),
            )));
        }
        std::fs::write(&path, &file.bytes).map_err(|error| {
            let _ = std::fs::remove_dir_all(&staging_dir);
            ArtifactError::VerificationMismatch(format!(
                "cannot write `{}`: {error}",
                path.display(),
            ))
        })?;
    }
    // Post-write verification: a mismatched intermediate cannot slip
    // through to the published tree.
    if let Err(error) = verify_artifact(&staging_dir, &staging) {
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&staging_dir, &destination) {
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(ArtifactError::VerificationMismatch(format!(
            "cannot commit `{}`: {error}",
            destination.display(),
        )));
    }
    Ok(destination)
}

/// Convenience: content identity of a text file.
#[must_use]
pub fn content_id_of_text(text: &str) -> ContentId {
    content_id_of_str(text)
}

// Artifact-class protocol tests moved to `tests/emath-artifact/tests/artifact_class.rs`.
