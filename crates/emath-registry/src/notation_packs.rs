//! N6: Standard notation library seeds (bead
//! emath-r3-n6-notation-stdlib-n5hp, 04 N6 + review fixes C6/C7).
//!
//! The four `core::notation` packs formalized as catalog data: `logic`,
//! `sets`, `calculus`, `algebra`. Every entry is a [`SymbolEntry`] at
//! `Proposed` (quarantine) — the packs are seed data, promoted through the
//! SSC lifecycle (producer-distinct G4 audit), never grandfathered.
//!
//! Review fixes encoded structurally:
//! - **C6**: no `\\`, `\/`, `/\` ASCII aliases — backslash is refused by
//!   the catalog alias gate (`E-SYMBOL-CATALOG-ALIAS-FORBIDDEN`); the packs
//!   carry no such spellings.
//! - **C7**: `~` is the distribution tag, never a negation alias; logic
//!   negation is the existing prefix `!`.
//! - **N4.5**: ASCII aliases are identifier spellings only (`and`, `or`,
//!   `compose`), never operator-lookalikes (`o` for composition is not
//!   carried; `compose(f, g)` + the Unicode glyph `∘` suffice).
//!
//! Precedence: pack numbers are seed proposals. Verification against the
//! parser's actual tiers is a separate gate (parser precedence is
//! tier-structured, not a numeric table), tracked on the notation-core bead.
//!
//! Admission is validated through [`SymbolCatalog::validate`], so the same
//! collision/confusable/alias gates apply to pack data as to hand-authored
//! entries.

#![forbid(unsafe_code)]

use crate::symbol_catalog::{AuthorityRing, SymbolCatalog, SymbolEntry, SymbolStatus};

/// Pack namespace prefix for core notation packs.
pub const CORE_NOTATION_PREFIX: &str = "core::notation";

/// One notation pack: named set of entries proposed to the SSC.
pub struct NotationPack {
    /// Full pack path, e.g. `core::notation::logic`.
    pub path: &'static str,
    pub entries: Vec<SymbolEntry>,
}

fn seed_entry(
    glyph: &str,
    fixity: &str,
    precedence: u32,
    core_path: &str,
    aliases: &[&str],
    pack: &str,
) -> SymbolEntry {
    SymbolEntry {
        glyph: glyph.to_string(),
        fixity: fixity.to_string(),
        precedence,
        default_world: None,
        core_path: core_path.to_string(),
        aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
        confusable_class: None,
        pack: pack.to_string(),
        status: SymbolStatus::Proposed,
        authority: AuthorityRing::Catalog,
        proposed_by: "n6-notation-stdlib".to_string(),
        reviewed_by: None,
    }
}

/// `core::notation::logic`: ∧ ∨ ¬ ==> <==> (ASCII `&&` `||` `!` `==>`
/// `<==>`). Negation is the existing prefix `!`, never `~` (C7).
#[must_use]
pub fn logic_pack() -> NotationPack {
    let path = "core::notation::logic";
    NotationPack {
        path,
        entries: vec![
            seed_entry("∧", "infixl", 60, "core::logic::and", &["and"], path),
            seed_entry("∨", "infixl", 55, "core::logic::or", &["or"], path),
            seed_entry("¬", "prefix", 90, "core::logic::not", &["not"], path),
            seed_entry("==>", "infixr", 30, "core::logic::implies", &[], path),
            seed_entry("<==>", "infix", 25, "core::logic::iff", &[], path),
        ],
    }
}

/// `core::notation::sets`: ∈ ∉ ∪ ∩ ∖ ⊆. ASCII aliases are identifier
/// spellings; no backslash forms (C6).
#[must_use]
pub fn sets_pack() -> NotationPack {
    let path = "core::notation::sets";
    NotationPack {
        path,
        entries: vec![
            seed_entry("∈", "infix", 70, "core::sets::contains", &["in"], path),
            seed_entry("∉", "infix", 70, "core::sets::not_contains", &[], path),
            seed_entry("∪", "infixl", 50, "core::sets::union", &["union"], path),
            seed_entry("∩", "infixl", 65, "core::sets::intersection", &["inter"], path),
            seed_entry("∖", "infixl", 65, "core::sets::difference", &["setdiff"], path),
            seed_entry("⊆", "infix", 70, "core::sets::subset", &["subset"], path),
        ],
    }
}

/// `core::notation::calculus`: ∫ ∂ d/dx lim. `d/dx` and `lim` are named
/// forms; the glyph seeds carry no slash-alias spellings (C6).
#[must_use]
pub fn calculus_pack() -> NotationPack {
    let path = "core::notation::calculus";
    NotationPack {
        path,
        entries: vec![
            seed_entry("∫", "prefix", 95, "core::calculus::integral", &["integral"], path),
            seed_entry("∂", "prefix", 95, "core::calculus::partial", &["partial"], path),
            seed_entry("d/dx", "prefix", 95, "core::calculus::derivative", &["ddx"], path),
            seed_entry("lim", "prefix", 85, "core::calculus::limit", &["limit"], path),
        ],
    }
}

/// `core::notation::algebra`: ∘ ⊗ ⊕. No `o` alias for composition (N4.5);
/// `compose(f, g)` + the Unicode glyph suffice.
#[must_use]
pub fn algebra_pack() -> NotationPack {
    let path = "core::notation::algebra";
    NotationPack {
        path,
        entries: vec![
            seed_entry("∘", "infixr", 75, "core::algebra::compose", &["compose"], path),
            seed_entry("⊗", "infixl", 65, "core::algebra::tensor", &["tensor"], path),
            seed_entry("⊕", "infixl", 55, "core::algebra::direct_sum", &["direct_sum"], path),
        ],
    }
}

/// All four seed packs in canonical order.
#[must_use]
pub fn all_packs() -> [NotationPack; 4] {
    [logic_pack(), sets_pack(), calculus_pack(), algebra_pack()]
}

/// Build one catalog from all packs (packs are disjoint namespaces; the
/// catalog's collision gates must pass over the union).
#[must_use]
pub fn catalog_from_packs() -> SymbolCatalog {
    let mut entries = Vec::new();
    for pack in all_packs() {
        entries.extend(pack.entries);
    }
    SymbolCatalog { entries }
}

