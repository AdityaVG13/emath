//! Tests for symbol_catalog.rs, migrated out of production code.

use emath_registry::notation_packs::*;
use emath_registry::symbol_catalog::*;
use emath_test_harness::Probe;

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

fn example_seed() -> SymbolCatalog {
    let entry = |glyph: &str, fixity: &str, precedence: u32, core_path: &str, aliases: &[&str]| SymbolEntry {
        glyph: glyph.to_string(),
        fixity: fixity.to_string(),
        precedence,
        default_world: None,
        core_path: core_path.to_string(),
        aliases: aliases.iter().map(|a| (*a).to_string()).collect(),
        confusable_class: None,
        pack: "tst.notation_ops".to_string(),
        status: SymbolStatus::Proposed,
        authority: AuthorityRing::Local,
        proposed_by: "tests/fixtures/language/intro/notation-ops.emath".to_string(),
        reviewed_by: None,
    };
    SymbolCatalog {
        entries: vec![
            entry("⊕", "infixl", 40, "core::math::pow", &["pw"]),
            entry("√", "prefix", 80, "core::math::sqrt", &[]),
            entry("inv", "postfix", 90, "core::math::recip", &[]),
        ],
    }
}

#[test]
fn symbol_catalog() {
    let mut p = Probe::new("symbol catalog admits clean seeds and refuses collisions by name");
    p.case("seed", |p| {
        let catalog = SymbolCatalog { entries: vec![entry("⊕", "core::math::pow"), entry("√", "core::math::sqrt"), entry("inv", "core::math::recip")] };
        for symbol in &catalog.entries {
            p.demand(format!("seed/{}", symbol.glyph), symbol.validate().is_ok(), "seed entry validates");
        }
        p.demand("union", catalog.validate().is_ok(), "no collisions");
        let mut aliased = SymbolCatalog { entries: vec![entry("⊕", "core::math::pow")] };
        aliased.entries[0].aliases = vec!["pw".to_string()];
        p.demand("alias", aliased.validate().is_ok(), "alias shares one path");
        let mut scoped = SymbolCatalog { entries: vec![entry("⊕", "core::math::pow")] };
        scoped.entries.push({ let mut o = entry("⊕", "core::math::add"); o.pack = "other.pack".to_string(); o });
        p.demand("scoped", scoped.validate().is_ok(), "distinct packs are disjoint");
        let mut proposed = SymbolCatalog { entries: vec![entry("⊕", "core::math::pow")] };
        proposed.entries[0].status = SymbolStatus::Proposed;
        proposed.entries[0].reviewed_by = None;
        p.demand("quarantine", proposed.validate().is_ok(), "proposed needs no reviewer");
        let mut clean = SymbolCatalog { entries: vec![entry("∧", "core::logic::and")] };
        clean.entries[0].aliases = vec!["and".to_string()];
        p.demand("identifier", clean.validate().is_ok(), "identifier alias passes");
    });
    p.case("refuse", |p| {
        let confusable = SymbolCatalog { entries: vec![{ let mut l = entry("⋅", "core::math::dot"); l.confusable_class = Some("middot-like".to_string()); l }, { let mut r = entry("·", "core::math::mul"); r.confusable_class = Some("middot-like".to_string()); r }] };
        let err = confusable.validate().unwrap_err();
        p.demand("confusable", err.starts_with(E_SYMBOL_CONFLUSABLE), "confusable refused");
        let mut ambiguous = SymbolCatalog { entries: vec![entry("⊕", "core::math::pow")] };
        ambiguous.entries.push({ let mut o = entry("⊕", "core::math::add"); o.status = SymbolStatus::Admitted; o });
        p.demand("ambiguous", ambiguous.validate().unwrap_err().starts_with(E_SYMBOL_AMBIGUOUS), "ambiguous refused");
        let mut self_cert = SymbolCatalog { entries: vec![entry("⊕", "core::math::pow")] };
        self_cert.entries[0].reviewed_by = Some("author-a".to_string());
        p.demand("self-cert", self_cert.validate().unwrap_err().starts_with(E_SYMBOL_SELF_CERTIFIED), "self-certified refused");
        let mut malformed = SymbolCatalog { entries: vec![entry("", "core::math::pow")] };
        p.demand("empty", malformed.validate().unwrap_err().starts_with(E_SYMBOL_MALFORMED), "empty glyph refused");
        malformed.entries[0].glyph = "⊕".to_string();
        malformed.entries[0].fixity = "side-ish".to_string();
        p.demand("fixity", malformed.validate().unwrap_err().starts_with(E_SYMBOL_MALFORMED), "bad fixity refused");
        let mut aliases = SymbolCatalog { entries: vec![entry("∧", "core::logic::and")] };
        aliases.entries[0].aliases = vec!["\\".to_string()];
        let err = aliases.validate().unwrap_err();
        p.demand("c6-code", err.starts_with(E_SYMBOL_ALIAS_FORBIDDEN), "backslash refused");
        p.contains("c6", &err, "C6");
        aliases.entries[0].aliases = vec!["~".to_string()];
        let err = aliases.validate().unwrap_err();
        p.demand("c7-code", err.starts_with(E_SYMBOL_ALIAS_FORBIDDEN), "tilde refused");
        p.contains("c7", &err, "C7");
        aliases.entries[0].aliases = vec!["!o!".to_string()];
        let err = aliases.validate().unwrap_err();
        p.demand("n45-code", err.starts_with(E_SYMBOL_ALIAS_FORBIDDEN), "non-identifier refused");
        p.contains("n45", &err, "N4.5");
    });
    p.case("determinism", |p| {
        let catalog = SymbolCatalog { entries: vec![entry("⊕", "core::math::pow"), entry("√", "core::math::sqrt")] };
        p.eq("stable", catalog.to_canonical_json(), catalog.to_canonical_json());
        let parsed = emath_artifact::parse_json_document(&catalog.to_canonical_json()).unwrap();
        p.eq("schema", parsed.string_field("schema").unwrap().to_string(), SYMBOL_CATALOG_SCHEMA.to_string());
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let committed = std::fs::read_to_string(workspace.join("language/notation/SYMBOL_CATALOG.json")).unwrap();
        p.eq("seed-bytes", committed, example_seed().to_canonical_json());
    });
    p.finish();
}
