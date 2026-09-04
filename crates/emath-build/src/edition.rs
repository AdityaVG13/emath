//! Manifest-scoped editions (05 section 1).
//!
//! `emath.toml` carries one package-level `edition = "2026"` field. Editions
//! are manifest-scoped, never per-file: a package cannot mix parse epochs
//! across files. This module reads that field from a manifest with a
//! deterministic line scan (std-only; no TOML dependency) and refuses
//! unknown editions with `E-PKG-EDITION-UNKNOWN`.
//!
//! Determinism class: pure function of file bytes; no environment reads.

#![forbid(unsafe_code)]

use emath_core::{E_PKG_EDITION_UNKNOWN, Edition, EditionError};
use std::path::Path;

/// Error reading the edition out of an `emath.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestEditionError {
    /// Manifest missing or unreadable.
    Unreadable(String),
    /// No `edition` field present.
    Missing,
    /// `edition` present but not a shipped value (`E-PKG-EDITION-UNKNOWN`).
    Unknown(EditionError),
}

impl ManifestEditionError {
    /// Diagnostic code for the error, for structured reporting.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            ManifestEditionError::Unreadable(_) => "E-PKG-MANIFEST-UNREADABLE",
            ManifestEditionError::Missing => "E-PKG-EDITION-MISSING",
            ManifestEditionError::Unknown(_) => E_PKG_EDITION_UNKNOWN,
        }
    }
}

impl std::fmt::Display for ManifestEditionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestEditionError::Unreadable(detail) => {
                write!(formatter, "E-PKG-MANIFEST-UNREADABLE: {detail}")
            }
            ManifestEditionError::Missing => {
                write!(
                    formatter,
                    "E-PKG-EDITION-MISSING: manifest has no `edition` field; \
                     expected `edition = \"{}\"`",
                    Edition::ALL[0]
                )
            }
            ManifestEditionError::Unknown(error) => write!(formatter, "{error}"),
        }
    }
}

/// Extract one `edition = "..."` value from manifest text.
///
/// Deterministic line scan: the first line whose key is exactly `edition`
/// (whitespace-tolerant, `#` comment stripped) wins; later duplicates are
/// ignored so the first declaration is the package's declared epoch. The
/// value must be a double-quoted string.
pub fn parse_edition_field(manifest: &str) -> Result<Edition, ManifestEditionError> {
    for line in manifest.lines() {
        let content = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = content.split_once('=') else {
            continue;
        };
        if key.trim() != "edition" {
            continue;
        }
        let value = value.trim();
        let Some(quoted) = value
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
        else {
            return Err(ManifestEditionError::Unknown(EditionError {
                code: E_PKG_EDITION_UNKNOWN,
                value: value.to_string(),
            }));
        };
        return Edition::from_manifest_str(quoted).map_err(ManifestEditionError::Unknown);
    }
    Err(ManifestEditionError::Missing)
}

/// Read the edition from an `emath.toml` on disk.
pub fn manifest_edition(manifest_path: &Path) -> Result<Edition, ManifestEditionError> {
    let text = std::fs::read_to_string(manifest_path)
        .map_err(|error| ManifestEditionError::Unreadable(error.to_string()))?;
    parse_edition_field(&text)
}
