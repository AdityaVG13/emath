//! Standard Symbol Catalog (SSC): the registry artifact governing which
//! glyphs exist, what they map to, and how collisions are resolved
//! (05 section 4).
//!
//! The same glyphs legitimately mean many things; worlds make interpretation
//! data, chosen deterministically, never silently. The SSC records each
//! glyph's fixity, precedence, default world binding, canonical core path,
//! aliases, confusable class, and lifecycle status.
//!
//! Three rings govern who may add entries, each with an authority cap:
//! Local (any author, cap `structural`), Registry (published packages, cap
//! `tested`), Catalog (SSC/core-prelude namespace, cap `certified`).
//!
//! Lifecycle: Proposed (quarantine: usable only via explicit
//! `use notation <pack>::<glyph>`) -> Checked (producer-distinct G4 audit;
//! promotion caps at `structural-checked`) -> Admitted (full ELP; part of an
//! edition's default notation set) -> Frozen (hidden from new editions per
//! the deprecation ladder, replayable forever).
//!
//! Collision policy:
//! - same glyph + same core path = alias (one canonical meaning, both
//!   spellings recorded; canonical rendering picks one);
//! - same glyph + different core path = permitted only as distinct scoped
//!   notation packs; the catalog records the pair so import resolution can
//!   emit the typed ambiguity refusal instead of precedence luck;
//! - different glyphs in the same confusable class may not both be Admitted
//!   in the same default namespace.
//!
//! No-claim boundary: full Unicode NFC verification is not implemented in
//! std; glyphs are authored NFC and the loader checks structural well-formed
//!ness only. A dedicated normalization gate is future work in the
//! notation-core governance area.

#![forbid(unsafe_code)]

/// Catalog document schema id.
pub const SYMBOL_CATALOG_SCHEMA: &str = "emath.symbol-catalog";
/// Catalog document version. Bump on any layout change; consumers refuse
/// versions they do not know.
pub const SYMBOL_CATALOG_VERSION: u32 = 1;

/// Typed refusal: two Admitted entries share a confusable class.
pub const E_SYMBOL_CONFLUSABLE: &str = "E-SYMBOL-CATALOG-CONFLUSABLE";
/// Typed refusal: one glyph maps to distinct core paths with no pack split.
pub const E_SYMBOL_AMBIGUOUS: &str = "E-SYMBOL-CATALOG-AMBIGUOUS";
/// Typed refusal: an entry's promotion was self-certified.
pub const E_SYMBOL_SELF_CERTIFIED: &str = "E-SYMBOL-CATALOG-SELF-CERTIFIED";
/// Typed refusal: entry missing a required field or malformed glyph.
pub const E_SYMBOL_MALFORMED: &str = "E-SYMBOL-CATALOG-MALFORMED";
/// Typed refusal: an alias spelling violates the N2/N4.5 clause set
/// (C6 backslash, C7 tilde, non-identifier ASCII).
pub const E_SYMBOL_ALIAS_FORBIDDEN: &str = "E-SYMBOL-CATALOG-ALIAS-FORBIDDEN";

/// Lifecycle of a catalog entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymbolStatus {
    /// Quarantine: explicit import only, authority `none`.
    Proposed,
    /// Producer-distinct G4 audit passed; cap `structural-checked`.
    Checked,
    /// Full ELP; part of an edition's default notation set.
    Admitted,
    /// Hidden from new editions; replayable forever.
    Frozen,
}

impl SymbolStatus {
    /// Statuses in lifecycle order (weakest to most frozen).
    pub const ALL: [SymbolStatus; 4] = [
        SymbolStatus::Proposed,
        SymbolStatus::Checked,
        SymbolStatus::Admitted,
        SymbolStatus::Frozen,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SymbolStatus::Proposed => "proposed",
            SymbolStatus::Checked => "checked",
            SymbolStatus::Admitted => "admitted",
            SymbolStatus::Frozen => "frozen",
        }
    }
}

/// Ring that submitted the entry, with its authority cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuthorityRing {
    /// Any author, in their own package. Cap: `structural` (self-declared).
    Local,
    /// Published packages. Cap `tested` (CI suite passed).
    Registry,
    /// SSC/core-prelude namespace. Cap `certified` (full ELP).
    Catalog,
}

impl AuthorityRing {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AuthorityRing::Local => "local",
            AuthorityRing::Registry => "registry",
            AuthorityRing::Catalog => "catalog",
        }
    }

    /// Maximum authority this ring may assert.
    #[must_use]
    pub fn authority_cap(self) -> &'static str {
        match self {
            AuthorityRing::Local => "structural",
            AuthorityRing::Registry => "tested",
            AuthorityRing::Catalog => "certified",
        }
    }
}

/// One catalog row: a glyph's contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolEntry {
    /// NFC codepoint sequence as authored (see module no-claim on NFC).
    pub glyph: String,
    /// Fixity spelling: `prefix | postfix | infixl | infixr | infix`.
    pub fixity: String,
    /// Explicit per-pack precedence.
    pub precedence: u32,
    /// Default world binding, if any (worlds thesis: interpretation is data).
    pub default_world: Option<String>,
    /// Canonical core path the glyph maps to, e.g. `core::math::pow`.
    pub core_path: String,
    /// Alternative spellings resolving to the same canonical path.
    pub aliases: Vec<String>,
    /// Confusable class shared with visually confusable glyphs, if any.
    pub confusable_class: Option<String>,
    /// Notation pack that declares the glyph (scoped namespaces).
    pub pack: String,
    /// Lifecycle status.
    pub status: SymbolStatus,
    /// Ring that submitted the entry.
    pub authority: AuthorityRing,
    /// Producer (proposer) of the entry; must differ from the reviewer of a
    /// promotion (negative control against self-certification).
    pub proposed_by: String,
    /// Reviewer of the last promotion; `None` while `Proposed`.
    pub reviewed_by: Option<String>,
}

impl SymbolEntry {
    /// Structural validation: required fields present, glyph non-empty, no
    /// control characters, fixity in the closed set.
    pub fn validate(&self) -> Result<(), String> {
        if self.glyph.is_empty()
            || self
                .glyph
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(format!(
                "{E_SYMBOL_MALFORMED}: glyph must be a non-empty NFC codepoint sequence, got `{}`",
                self.glyph
            ));
        }
        match self.fixity.as_str() {
            "prefix" | "postfix" | "infixl" | "infixr" | "infix" => {}
            other => {
                return Err(format!(
                    "{E_SYMBOL_MALFORMED}: unknown fixity `{other}` for glyph `{}`",
                    self.glyph
                ));
            }
        }
        if self.core_path.is_empty() {
            return Err(format!(
                "{E_SYMBOL_MALFORMED}: glyph {} has no canonical core path",
                self.glyph
            ));
        }
        if self.pack.is_empty() {
            return Err(format!(
                "{E_SYMBOL_MALFORMED}: glyph {} has no notation pack",
                self.glyph
            ));
        }
        self.validate_alias_clauses()?;
        Ok(())
    }

    /// N2/N4.5 alias-clause gates (C6/C7 fixes, N6):
    /// - ASCII aliases containing a backslash are refused (C6: `\\`, `\/`,
    ///   `/\` are not alias spellings; use Unicode or named functions);
    /// - ASCII aliases must be valid identifier spellings (N4.5 XID rule:
    ///   `o` as an alias for composition is refused; use `compose(f, g)` or
    ///   the Unicode glyph);
    /// - `~` is never a negation alias (C7: `~` is the distribution tag;
    ///   negation is the existing prefix `!`).
    fn validate_alias_clauses(&self) -> Result<(), String> {
        for alias in &self.aliases {
            if alias.contains('\\') {
                return Err(format!(
                    "{E_SYMBOL_ALIAS_FORBIDDEN}: alias `{alias}` for glyph {} contains a backslash (C6); \
                     use the Unicode glyph or a named function",
                    self.glyph
                ));
            }
            if alias == "~" {
                return Err(format!(
                    "{E_SYMBOL_ALIAS_FORBIDDEN}: `~` is not an alias spelling (C7 distribution tag); \
                     negation uses the existing prefix `!`",
                ));
            }
            let identifier_ok = !alias.is_empty()
                && alias
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '_')
                && alias.chars().all(|c| c.is_alphanumeric() || c == '_');
            if !identifier_ok {
                return Err(format!(
                    "{E_SYMBOL_ALIAS_FORBIDDEN}: alias `{alias}` for glyph {} is not a valid \
                     identifier spelling (N4.5 XID); use the Unicode glyph or a named function",
                    self.glyph
                ));
            }
        }
        Ok(())
    }
}

/// The Standard Symbol Catalog: an ordered set of entries with deterministic
/// collision checking. Entry order is preserved (seed order is canonical).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SymbolCatalog {
    pub entries: Vec<SymbolEntry>,
}

impl SymbolCatalog {
    /// Validate every entry, then run the collision gates:
    /// confusable exclusion among Admitted entries and same-glyph/different-
    /// meaning detection within one pack.
    pub fn validate(&self) -> Result<(), String> {
        for entry in &self.entries {
            entry.validate()?;
            // Promotion gate: a promotion (anything past Proposed) must have
            // a producer-distinct reviewer.
            if entry.status != SymbolStatus::Proposed {
                match &entry.reviewed_by {
                    Some(reviewer) if reviewer != &entry.proposed_by => {}
                    _ => {
                        return Err(format!(
                            "{E_SYMBOL_SELF_CERTIFIED}: glyph {} promoted by `{}` without a producer-distinct reviewer",
                            entry.glyph, entry.proposed_by
                        ));
                    }
                }
            }
        }

        // Confusable exclusion among Admitted entries in the same namespace
        // (default namespaces are per-pack).
        for (index, left) in self.entries.iter().enumerate() {
            if left.status != SymbolStatus::Admitted {
                continue;
            }
            for right in self.entries.iter().skip(index + 1) {
                if right.status != SymbolStatus::Admitted || right.pack != left.pack {
                    continue;
                }
                let class_conflict = left.confusable_class.is_some()
                    && left.confusable_class == right.confusable_class;
                let same_glyph = left.glyph == right.glyph;
                let different_meaning = left.core_path != right.core_path;
                if class_conflict {
                    return Err(format!(
                        "{E_SYMBOL_CONFLUSABLE}: glyphs {} and {} share confusable class {:?} in pack {}",
                        left.glyph,
                        right.glyph,
                        left.confusable_class.as_deref().unwrap_or(""),
                        left.pack
                    ));
                }
                if same_glyph && different_meaning {
                    return Err(format!(
                        "{E_SYMBOL_AMBIGUOUS}: glyph {} maps to both {} and {} in pack {}; \
                         split into distinct scoped notation packs",
                        left.glyph, left.core_path, right.core_path, left.pack
                    ));
                }
            }
        }
        Ok(())
    }

    /// Canonical JSON document. Byte-identical on regeneration for identical
    /// entries (entry order is seed order; all fields deterministic).
    #[must_use]
    pub fn to_canonical_json(&self) -> String {
        let mut entry_docs: Vec<String> = Vec::new();
        for entry in &self.entries {
            let mut object = emath_artifact::JsonWriter::object();
            object.string("glyph", &entry.glyph);
            object.string("fixity", &entry.fixity);
            object.int("precedence", u64::from(entry.precedence));
            if let Some(world) = &entry.default_world {
                object.string("default_world", world);
            }
            object.string("core_path", &entry.core_path);
            object.strings("aliases", &entry.aliases);
            if let Some(class) = &entry.confusable_class {
                object.string("confusable_class", class);
            }
            object.string("pack", &entry.pack);
            object.string("status", entry.status.as_str());
            object.string("authority", entry.authority.as_str());
            object.string("proposed_by", &entry.proposed_by);
            if let Some(reviewer) = &entry.reviewed_by {
                object.string("reviewed_by", reviewer);
            }
            entry_docs.push(object.finish());
        }
        let mut root = emath_artifact::JsonWriter::object();
        root.string("schema", SYMBOL_CATALOG_SCHEMA);
        root.string("schema_version", "v1");
        root.int("entries", self.entries.len() as u64);
        root.objects("catalog", &entry_docs);
        root.finish()
    }
}
