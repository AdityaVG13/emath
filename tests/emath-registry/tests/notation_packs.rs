//! Tests for notation_packs.rs, migrated out of production code.

use emath_registry::notation_packs::*;
use emath_registry::symbol_catalog::*;
use emath_test_harness::Probe;

#[test]
fn notation_packs() {
    let mut p = Probe::new("notation packs seed required glyphs and stay quarantine-clean");
    p.case("glyphs", |p| {
        let packs = all_packs();
        let glyphs: Vec<&str> = packs.iter().flat_map(|pack| pack.entries.iter().map(|e| e.glyph.as_str())).collect();
        for required in ["∧", "∨", "¬", "==>", "<==>", "∈", "∉", "∪", "∩", "∖", "⊆", "∫", "∂", "d/dx", "lim", "∘", "⊗", "⊕"] {
            p.demand(format!("glyph/{required}"), glyphs.contains(&required), "pack seeds carry it");
        }
        for (path, alias) in [("core::logic::and", "and"), ("core::logic::or", "or"), ("core::logic::not", "not"), ("core::sets::subset", "subset"), ("core::algebra::compose", "compose")] {
            let found = all_packs().iter().flat_map(|pack| &pack.entries).find(|e| e.core_path == path).map(|e| e.aliases.contains(&alias.to_string())).unwrap_or(false);
            p.demand(format!("alias/{path}/{alias}"), found, "ASCII spelling present");
        }
    });
    p.case("spelling", |p| {
        for pack in all_packs() {
            for entry in &pack.entries {
                p.demand(format!("c6/{}", pack.path), !entry.aliases.iter().any(|a| a.contains('\\')), "no backslash");
                p.demand(format!("c7/{}", pack.path), !entry.aliases.iter().any(|a| a == "~"), "no tilde");
                for alias in &entry.aliases {
                    if alias.is_ascii() {
                        let ok = alias.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') && alias.chars().all(|c| c.is_alphanumeric() || c == '_');
                        p.demand(format!("n45/{alias}"), ok, "identifier spelling");
                    }
                }
            }
        }
        let composition = algebra_pack().entries.iter().find(|e| e.core_path == "core::algebra::compose").unwrap().clone();
        p.demand("no-o", !composition.aliases.iter().any(|a| a == "o"), "`o` is not a compose alias");
        p.demand("compose", composition.aliases.contains(&"compose".to_string()), "compose alias present");
    });
    p.case("union", |p| {
        p.demand("validates", catalog_from_packs().validate().is_ok(), "pack union passes gates");
        for pack in all_packs() {
            for entry in &pack.entries {
                p.eq(format!("status/{}", entry.glyph), entry.status.clone(), SymbolStatus::Proposed);
                p.eq(format!("authority/{}", entry.glyph), entry.authority.clone(), AuthorityRing::Catalog);
                p.eq(format!("pack/{}", entry.glyph), entry.pack.clone(), pack.path.to_string());
                p.demand(format!("prefix/{}", entry.glyph), pack.path.starts_with(CORE_NOTATION_PREFIX), "core prefix");
            }
        }
    });
    p.finish();
}
