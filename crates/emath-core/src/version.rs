//! The emath version stack (bead emath-r3-version-stack-9z1a, 05 section 1).
//!
//! Five version lines govern the language. Only edition and kind schema
//! version enter semantic identity; grammar and encoding versions are replay
//! versions. emath's promise is byte-deterministic forever: an artifact
//! admitted today must be re-executable decades from now, so versions are
//! explicit constants, never guesses, and the deprecation ladder hides forms
//! instead of removing them.
//!
//! 1. Reference version — the 16-chapter normative text as a whole; bumped on
//!    any normative sentence change.
//! 2. Grammar version — surface.ebnf + genesis.ebnf; append-only within an
//!    edition.
//! 3. Edition — package-level `emath.toml` field; parse/resolve epoch.
//!    Editions may admit new grammar, new sections/kinds/notations, upgraded
//!    warnings, and formatter changes. They may NOT change name-resolution
//!    semantics for existing packages.
//! 4. Canonical encoding version — length-framed deterministic binary/JSON
//!    encoding; decoders accept all prior versions, encoders emit exactly one.
//! 5. Kind schema version — per-kind, already carried by identity elsewhere.
//!
//! Std-only, deterministic, hashable: all values are fixed string constants
//! so content identity over them is stable across toolchain builds.

#![forbid(unsafe_code)]

/// Version line 1: the 16-chapter normative reference as a whole. Bump on
/// any normative sentence change in `language/reference/`.
pub const EMATH_REFERENCE_VERSION: &str = "1.0.0";

/// Version line 2: surface.ebnf + genesis.ebnf as a whole. Append-only
/// within an edition; bumped when the shipped grammar text changes.
pub const EMATH_GRAMMAR_VERSION: &str = "2026.1";

/// Version line 4: the length-framed deterministic encoding. Decoders accept
/// all prior versions; encoders emit exactly one.
pub const EMATH_CANON_ENCODING_VERSION: &str = "1";

/// The five version lines as `(name, value)` in stable display order. Kind
/// schema version is per-kind and therefore not listed here; it travels with
/// identity (see `emath-core` ids).
pub const VERSION_STACK: [(&str, &str); 4] = [
    ("reference", EMATH_REFERENCE_VERSION),
    ("grammar", EMATH_GRAMMAR_VERSION),
    ("canon_encoding", EMATH_CANON_ENCODING_VERSION),
    ("kind_schema", "per-kind (identity)"),
];

/// Diagnostic code for a manifest edition string no shipped edition defines.
pub const E_PKG_EDITION_UNKNOWN: &str = "E-PKG-EDITION-UNKNOWN";

/// Package-level parse/resolve epoch. Manifest-scoped, never per-file: one
/// package cannot mix parse epochs across files.
///
/// Editions may (a) admit new grammar, (b) admit new section names, kinds,
/// and notations, (c) upgrade warnings to deny-by-default, (d) change
/// formatter output. Editions may not change name-resolution semantics for
/// existing packages. Edition enters identity as provenance, not as a
/// meaning parameter: two packages differing only in edition with identical
/// lowered declarations share semantic identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Edition {
    /// The founding edition (`"2026"`). Grammar `2026.1`.
    Ed2026,
    /// The second edition (`"2030"`). Grammar `2030.1`.
    Ed2030,
}

impl Edition {
    /// Every shipped edition, oldest first. Append-only: new editions are
    /// added at the end and prior editions stay parseable forever (replay).
    pub const ALL: [Edition; 2] = [Edition::Ed2026, Edition::Ed2030];

    /// Manifest spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Edition::Ed2026 => "2026",
            Edition::Ed2030 => "2030",
        }
    }

    /// Grammar version this edition's parser table ships.
    #[must_use]
    pub fn grammar_version(self) -> &'static str {
        match self {
            Edition::Ed2026 => "2026.1",
            Edition::Ed2030 => "2030.1",
        }
    }

    /// Resolve a manifest `edition = "..."` value. Unknown editions are a
    /// typed refusal ([`E_PKG_EDITION_UNKNOWN`]), never a guess.
    pub fn from_manifest_str(value: &str) -> Result<Self, EditionError> {
        Edition::ALL
            .iter()
            .copied()
            .find(|edition| edition.as_str() == value)
            .ok_or_else(|| EditionError {
                code: E_PKG_EDITION_UNKNOWN,
                value: value.to_string(),
            })
    }
}

impl std::fmt::Display for Edition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Typed refusal for an edition the toolchain does not ship.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditionError {
    /// Always [`E_PKG_EDITION_UNKNOWN`].
    pub code: &'static str,
    /// The offending manifest value.
    pub value: String,
}

impl std::fmt::Display for EditionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: unknown edition `{}`; shipped editions: {}",
            self.code,
            self.value,
            Edition::ALL
                .iter()
                .map(|edition| edition.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// The deprecation ladder: no stage removes, only hides (C++ lesson applied
/// with discipline). Forms travel Recognized -> Deprecated -> Hidden ->
/// Frozen and remain parseable under their home edition forever.
///
/// - `Recognized`: admitted in the current edition's default grammar.
/// - `Deprecated`: parses with a migrate-able warning; absent from new
///   editions' default grammar.
/// - `Hidden`: absent from new editions; still parses under its home edition.
/// - `Frozen`: replay only — parsed by replay tooling, refused by admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeprecationStage {
    Recognized,
    Deprecated,
    Hidden,
    Frozen,
}

impl DeprecationStage {
    /// Stages in ladder order (weakest to most hidden).
    pub const ALL: [DeprecationStage; 4] = [
        DeprecationStage::Recognized,
        DeprecationStage::Deprecated,
        DeprecationStage::Hidden,
        DeprecationStage::Frozen,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DeprecationStage::Recognized => "recognized",
            DeprecationStage::Deprecated => "deprecated",
            DeprecationStage::Hidden => "hidden",
            DeprecationStage::Frozen => "frozen",
        }
    }
}
