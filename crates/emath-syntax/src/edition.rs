//! Grammar selection by manifest edition (05 section 1).
//!
//! The toolchain ships every historical edition's grammar, selected by the
//! package manifest. Within an edition the grammar is append-only; forms are
//! never removed, only hidden via the deprecation ladder
//! ([`emath_core::DeprecationStage`]). Replay of an artifact under its home
//! edition therefore always parses.
//!
//! Adding an edition adds a row to [`GRAMMAR_PROFILES`] and a variant to
//! `emath_core::Edition` — never a deletion.

#![forbid(unsafe_code)]

use emath_core::{DeprecationStage, Edition, EditionError};

/// One shipped grammar table row: which edition parses with which grammar
/// version, and which deprecation stages that edition's default grammar
/// still admits by default.
pub struct GrammarProfile {
    pub edition: Edition,
    /// Grammar version this edition's parser table ships.
    pub grammar_version: &'static str,
    /// Forms at or above this ladder stage are admitted by default in this
    /// edition; hidden forms stay parseable only under their home edition.
    pub min_default_stage: DeprecationStage,
}

/// Every shipped edition's grammar, oldest first. The parser selects the
/// row whose edition equals the manifest edition.
pub const GRAMMAR_PROFILES: [GrammarProfile; 2] = [
    GrammarProfile {
        edition: Edition::Ed2026,
        grammar_version: "2026.1",
        min_default_stage: DeprecationStage::Recognized,
    },
    GrammarProfile {
        edition: Edition::Ed2030,
        grammar_version: "2030.1",
        min_default_stage: DeprecationStage::Recognized,
    },
];

/// Error selecting a grammar for an edition: always
/// `E-PKG-EDITION-UNKNOWN` via [`emath_core::E_PKG_EDITION_UNKNOWN`].
pub type GrammarSelectError = EditionError;

/// Select the grammar profile for a manifest edition string. Unknown
/// editions are a typed refusal, never a guess.
#[must_use]
pub fn grammar_profile_for(edition: &str) -> Result<&'static GrammarProfile, GrammarSelectError> {
    let resolved = Edition::from_manifest_str(edition)?;
    Ok(GRAMMAR_PROFILES
        .iter()
        .find(|profile| profile.edition == resolved)
        .expect("every shipped edition has a grammar row"))
}

/// Whether a form at `stage` is admitted by default under `edition`'s
/// grammar. Deprecated/hidden/frozen forms keep parsing under their home
/// edition (replay), but only `Recognized` forms are admitted by the current
/// default table.
#[must_use]
pub fn admitted_by_default(stage: DeprecationStage, edition: &str) -> bool {
    grammar_profile_for(edition)
        .map(|profile| stage < profile.min_default_stage || stage == profile.min_default_stage)
        .unwrap_or(false)
}
