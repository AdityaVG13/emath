//! edition tests migrated from the in-crate `#[cfg(test)]` module.

use emath_build::edition::*;
use emath_core::version::{DeprecationStage, Edition};
use emath_core::{E_PKG_EDITION_UNKNOWN, EditionError};
use std::path::Path;

#[test]
fn edition_2026_parses() {
    assert_eq!(
        parse_edition_field("edition = \"2026\"\n"),
        Ok(Edition::Ed2026)
    );
    assert_eq!(
        parse_edition_field("[package]\n  edition=\"2026\"  # founding epoch\n"),
        Ok(Edition::Ed2026)
    );
}

#[test]
fn first_edition_declaration_wins() {
    let manifest = "edition = \"2026\"\nedition = \"2030\"\n";
    assert_eq!(parse_edition_field(manifest), Ok(Edition::Ed2026));
}

#[test]
fn unknown_edition_refused_with_code() {
    let error = parse_edition_field("edition = \"2099\"\n").expect_err("2099");
    assert_eq!(error.code(), E_PKG_EDITION_UNKNOWN);
}

#[test]
fn missing_or_malformed_refused() {
    assert_eq!(
        parse_edition_field("[package]\nname = \"x\"\n"),
        Err(ManifestEditionError::Missing)
    );
    // Unquoted value is not a valid edition spelling.
    let error = parse_edition_field("edition = 2026\n").expect_err("unquoted");
    assert_eq!(error.code(), E_PKG_EDITION_UNKNOWN);
}

#[test]
fn unreadable_manifest_reports_code() {
    let error = manifest_edition(Path::new("/nonexistent/emath.toml")).expect_err("missing file");
    assert_eq!(error.code(), "E-PKG-MANIFEST-UNREADABLE");
}
