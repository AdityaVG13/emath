//! Tests for symbol_catalog.rs, migrated out of production code.
//! The module under test is fully public, so these exercise the
//! same API an external consumer sees.

use emath_registry::notation_packs::*;
use emath_registry::symbol_catalog::*;

fn entry(glyph: &str, core_path: &str) -> SymbolEntry {
    SymbolEntry {
        glyph: glyph.to_string(),
        fixity: "infixl".to_string(),
        precedence: 40,
        default_world: None,
        core_path: core_path.to_string(),
        aliases: vec![],
        confusable_class: None,
        pack: "tst.notation_ops".to_string(),
        status: SymbolStatus::Admitted,
        authority: AuthorityRing::Local,
        proposed_by: "author-a".to_string(),
        reviewed_by: Some("author-b".to_string()),
    }
}

#[test]
fn seeded_ops_validate() {
    let catalog = SymbolCatalog {
        entries: vec![
            entry("⊕", "core::math::pow"),
            entry("√", "core::math::sqrt"),
            entry("inv", "core::math::recip"),
        ],
    };
    for symbol in &catalog.entries {
        symbol.validate().expect("seed entry");
    }
    catalog.validate().expect("no collisions");
}

#[test]
fn alias_spelling_is_not_a_collision() {
    let mut catalog = SymbolCatalog {
        entries: vec![entry("⊕", "core::math::pow")],
    };
    catalog.entries[0].aliases = vec!["pw".to_string()];
    catalog.validate().expect("alias shares one canonical path");
}

#[test]
fn confusable_pair_is_refused() {
    let mut left = entry("⋅", "core::math::dot");
    left.confusable_class = Some("middot-like".to_string());
    let mut right = entry("·", "core::math::mul");
    right.confusable_class = Some("middot-like".to_string());
    let catalog = SymbolCatalog {
        entries: vec![left, right],
    };
    let error = catalog.validate().expect_err("confusable pair");
    assert!(error.starts_with(E_SYMBOL_CONFLUSABLE), "{error}");
}

#[test]
fn same_glyph_different_meaning_is_refused_in_one_pack() {
    let mut catalog = SymbolCatalog {
        entries: vec![entry("⊕", "core::math::pow")],
    };
    let mut other = entry("⊕", "core::math::add");
    other.status = SymbolStatus::Admitted;
    catalog.entries.push(other);
    let error = catalog.validate().expect_err("ambiguous glyph");
    assert!(error.starts_with(E_SYMBOL_AMBIGUOUS), "{error}");
}

#[test]
fn same_glyph_across_packs_is_scoped_ok() {
    let mut catalog = SymbolCatalog {
        entries: vec![entry("⊕", "core::math::pow")],
    };
    let mut other = entry("⊕", "core::math::add");
    other.pack = "other.pack".to_string();
    catalog.entries.push(other);
    catalog
        .validate()
        .expect("distinct packs are disjoint namespaces");
}

#[test]
fn self_certified_promotion_is_refused() {
    let mut catalog = SymbolCatalog {
        entries: vec![entry("⊕", "core::math::pow")],
    };
    catalog.entries[0].reviewed_by = Some("author-a".to_string());
    let error = catalog.validate().expect_err("self-certified");
    assert!(error.starts_with(E_SYMBOL_SELF_CERTIFIED), "{error}");
}

#[test]
fn proposed_entries_need_no_reviewer() {
    let mut catalog = SymbolCatalog {
        entries: vec![entry("⊕", "core::math::pow")],
    };
    catalog.entries[0].status = SymbolStatus::Proposed;
    catalog.entries[0].reviewed_by = None;
    catalog.validate().expect("proposed is quarantine");
}

#[test]
fn malformed_entry_is_refused() {
    let mut catalog = SymbolCatalog {
        entries: vec![entry("", "core::math::pow")],
    };
    let error = catalog.validate().expect_err("empty glyph");
    assert!(error.starts_with(E_SYMBOL_MALFORMED), "{error}");
    catalog.entries[0].glyph = "⊕".to_string();
    catalog.entries[0].fixity = "side-ish".to_string();
    let error = catalog.validate().expect_err("bad fixity");
    assert!(error.starts_with(E_SYMBOL_MALFORMED), "{error}");
}

#[test]
fn alias_clauses_c6_c7_n45() {
    // C6: backslash aliases refused.
    let mut catalog = SymbolCatalog {
        entries: vec![entry("∧", "core::logic::and")],
    };
    catalog.entries[0].aliases = vec!["\\".to_string()];
    let error = catalog.validate().expect_err("backslash alias");
    assert!(error.starts_with(E_SYMBOL_ALIAS_FORBIDDEN), "{error}");
    assert!(error.contains("C6"), "{error}");
    // C7: `~` refused as alias.
    catalog.entries[0].aliases = vec!["~".to_string()];
    let error = catalog.validate().expect_err("tilde alias");
    assert!(error.starts_with(E_SYMBOL_ALIAS_FORBIDDEN), "{error}");
    assert!(error.contains("C7"), "{error}");
    // N4.5: non-identifier ASCII (`o` is fine as ident, but `∘-ish`
    // operator spellings are not) — refused.
    catalog.entries[0].aliases = vec!["∘".to_string()];
    // Unicode glyph aliases are fine (identifier_ok check only applies
    // to ASCII operator spellings); exercise the actual refusal case:
    // an ASCII operator alias like `/\` without backslash is caught by
    // C6; `!o!` by N4.5.
    catalog.entries[0].aliases = vec!["!o!".to_string()];
    let error = catalog.validate().expect_err("non-identifier alias");
    assert!(error.starts_with(E_SYMBOL_ALIAS_FORBIDDEN), "{error}");
    assert!(error.contains("N4.5"), "{error}");
    // Clean identifier alias passes.
    catalog.entries[0].aliases = vec!["and".to_string()];
    catalog.validate().expect("identifier alias ok");
}

#[test]
fn canonical_json_is_deterministic() {
    let catalog = SymbolCatalog {
        entries: vec![
            entry("⊕", "core::math::pow"),
            entry("√", "core::math::sqrt"),
        ],
    };
    let first = catalog.to_canonical_json();
    let second = catalog.to_canonical_json();
    assert_eq!(first, second);
    let parsed = emath_artifact::parse_json_document(&first).expect("valid JSON");
    assert_eq!(
        parsed.string_field("schema").expect("schema"),
        SYMBOL_CATALOG_SCHEMA
    );
}

#[test]
fn committed_seed_artifact_matches_generator() {
    // The on-disk SSC JSON must be byte-identical to regeneration (the
    // coverage-ledger --check discipline applied to the catalog).
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = workspace.join("language/notation/SYMBOL_CATALOG.json");
    let committed =
        std::fs::read_to_string(&path).expect("SYMBOL_CATALOG.json exists at repo root");
    let seed = emath_registry_example_seed();
    assert_eq!(committed, seed.to_canonical_json());
}

/// Mirror of the example generator's seed; kept in sync by the
/// byte-equality test above.
fn emath_registry_example_seed() -> SymbolCatalog {
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
    SymbolCatalog {
        entries: vec![
            entry("⊕", "infixl", 40, "core::math::pow", &["pw"]),
            entry("√", "prefix", 80, "core::math::sqrt", &[]),
            entry("inv", "postfix", 90, "core::math::recip", &[]),
        ],
    }
}
