//! Tests for notation_packs.rs, migrated out of production code.
//! The module under test is fully public, so these exercise the
//! same API an external consumer sees.

use emath_registry::notation_packs::*;
use emath_registry::symbol_catalog::*;

#[test]
fn packs_admit_required_glyphs() {
    let packs = all_packs();
    let glyphs: Vec<&str> = packs
        .iter()
        .flat_map(|pack| pack.entries.iter().map(|entry| entry.glyph.as_str()))
        .collect();
    for required in [
        "∧", "∨", "¬", "==>", "<==>", // logic
        "∈", "∉", "∪", "∩", "∖", "⊆", // sets
        "∫", "∂", "d/dx", "lim", // calculus
        "∘", "⊗", "⊕", // algebra
    ] {
        assert!(
            glyphs.contains(&required),
            "pack seeds missing required glyph {required}"
        );
    }
    // Required identifier-aliases are part of the pack contract (the
    // ASCII spellings users actually type); a dropped alias is a hole
    // the glyph check cannot see.
    let by_core_path: Vec<(&str, &Vec<String>)> = packs
        .iter()
        .flat_map(|pack| {
            pack.entries
                .iter()
                .map(|entry| (entry.core_path.as_str(), &entry.aliases))
        })
        .collect();
    for (core_path, required_alias) in [
        ("core::logic::and", "and"),
        ("core::logic::or", "or"),
        ("core::logic::not", "not"),
        ("core::sets::subset", "subset"),
        ("core::algebra::compose", "compose"),
    ] {
        let empty: Vec<String> = Vec::new();
        let aliases = by_core_path
            .iter()
            .find(|(path, _)| *path == core_path)
            .map(|(_, aliases)| *aliases)
            .unwrap_or(&empty);
        assert!(
            aliases.contains(&required_alias.to_string()),
            "pack seeds missing required alias `{required_alias}` for {core_path}"
        );
    }
}

#[test]
fn no_backslash_or_tilde_aliases_anywhere() {
    for pack in all_packs() {
        for entry in &pack.entries {
            assert!(
                !entry.aliases.iter().any(|alias| alias.contains('\\')),
                "C6 violation in {}: {:?}",
                pack.path,
                entry.aliases
            );
            assert!(
                !entry.aliases.iter().any(|alias| alias == "~"),
                "C7 violation in {}: {:?}",
                pack.path,
                entry.aliases
            );
            // N4.5: ASCII aliases are identifier spellings.
            for alias in &entry.aliases {
                if alias.is_ascii() {
                    let identifier_ok = alias
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_')
                        && alias.chars().all(|c| c.is_alphanumeric() || c == '_');
                    assert!(
                        identifier_ok,
                        "N4.5 violation: alias `{alias}` in {}",
                        pack.path
                    );
                }
            }
        }
    }
}

#[test]
fn composition_has_no_o_alias() {
    let pack = algebra_pack();
    let composition = pack
        .entries
        .iter()
        .find(|entry| entry.core_path == "core::algebra::compose")
        .expect("compose entry");
    assert!(
        !composition.aliases.iter().any(|alias| alias == "o"),
        "N6 C6-fix: `o` must not be a composition alias"
    );
    assert!(composition.aliases.contains(&"compose".to_string()));
}

#[test]
fn union_catalog_validates() {
    let catalog = catalog_from_packs();
    catalog.validate().expect("pack union passes all gates");
}

#[test]
fn all_entries_are_quarantine_proposed() {
    for pack in all_packs() {
        for entry in &pack.entries {
            assert_eq!(entry.status, SymbolStatus::Proposed);
            assert_eq!(entry.authority, AuthorityRing::Catalog);
            assert_eq!(entry.pack, pack.path);
            assert!(pack.path.starts_with(CORE_NOTATION_PREFIX));
        }
    }
}
