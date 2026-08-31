//! Emits the canonical machine-readable Symbol Catalog JSON for the seed
//! entries (the existing `notation` declarations in
//! `tests/fixtures/language/intro/notation-ops.emath`).
//!
//! Run from the repo root:
//!   cargo run -p emath-registry --example emit_symbol_catalog \
//!     > language/notation/SYMBOL_CATALOG.json
//!
//! Deterministic: identical seed = byte-identical output.

use emath_registry::{AuthorityRing, SymbolCatalog, SymbolEntry, SymbolStatus};

fn main() {
    let entry = |glyph: &str, fixity: &str, precedence: u32, core_path: &str, aliases: &[&str]| {
        SymbolEntry {
            glyph: glyph.to_string(),
            fixity: fixity.to_string(),
            precedence,
            default_world: None,
            core_path: core_path.to_string(),
            aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
            confusable_class: None,
            pack: "tst.notation_ops".to_string(),
            status: SymbolStatus::Proposed,
            authority: AuthorityRing::Local,
            proposed_by: "tests/fixtures/language/intro/notation-ops.emath".to_string(),
            reviewed_by: None,
        }
    };
    let catalog = SymbolCatalog {
        entries: vec![
            entry("⊕", "infixl", 40, "core::math::pow", &["pw"]),
            entry("√", "prefix", 80, "core::math::sqrt", &[]),
            entry("inv", "postfix", 90, "core::math::recip", &[]),
        ],
    };
    catalog.validate().expect("seed catalog validates");
    print!("{}", catalog.to_canonical_json());
}
