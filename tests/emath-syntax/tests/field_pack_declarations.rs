//! `emath field_pack` / `genome` package
//! declaration — syntax + admission.
//!
//! The law: packs are how cells/theories/methods/worlds SHIP
//! one declaration kind whose exports are artifact data (the
//! semantic image / `.emlib` and the layout/install tooling
//! consume them), never a compiler rebuild and never runnable meaning.
//! Admission rules:
//! - A pack exports DATA through a closed section table (`exports:`
//!   command lines `cell|theory|method|world <name>`, `metadata:`
//!   description lines). Unknown sections refuse typed (`E-SYN-101`) —
//!   pack source cannot inject parser keywords, and there is no hidden
//!   desugar: the pack never lands in `package.declarations` (no
//!   silent custom→strict fallthrough).
//! - A minimal pack that exports nothing but metadata admits.
//! - `use <package>.<pack>` is already a language form via the
//!   r3-imports admission (in-package module imports resolve pack
//!   sources; no core rebuild).

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

fn session() -> CompilerSession {
    install_source_parser();
    CompilerSession::new(Limits::default())
}

fn error_codes(result: &emath_sema::CheckResult) -> Vec<String> {
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect()
}

fn spectral_pack_source() -> String {
    "package community\n\nemath field_pack spectral_style:\n    exports:\n        cell softmax\n        theory spectral\n    metadata:\n        description reference spectral pack\n".to_string()
}

#[test]
fn pack_admits_and_lists_exports() {
    // Happy path: `community::spectral-style` — the package line names
    // `community`, the declaration names the pack; the admission lists
    // the exports in source order as artifact data.
    let mut session = session();
    let result = session.check_owned("spectral-style", &spectral_pack_source());
    let codes = error_codes(&result);
    assert!(
        codes.is_empty(),
        "the pack fixture admits with no errors, got {codes:?}"
    );
    assert_eq!(result.package.field_packs.len(), 1, "one pack entry");
    let pack = &result.package.field_packs[0];
    assert_eq!(pack.name, "spectral_style");
    assert_eq!(
        pack.exports,
        vec![
            ("cell".to_string(), "softmax".to_string()),
            ("theory".to_string(), "spectral".to_string()),
        ],
        "exports list in source order"
    );
}

#[test]
fn metadata_only_pack_admits() {
    // Boundary: a minimal pack that exports nothing but metadata admits
    // (nothing blocks on rich exports).
    let source = "package community\n\nemath field_pack minimal:\n    metadata:\n        description exports nothing yet\n".to_string();
    let mut session = session();
    let result = session.check_owned("minimal-pack", &source);
    let codes = error_codes(&result);
    assert!(codes.is_empty(), "metadata-only pack admits, got {codes:?}");
    assert_eq!(result.package.field_packs.len(), 1);
    assert!(
        result.package.field_packs[0].exports.is_empty(),
        "no exports claimed"
    );
}

#[test]
fn pack_cannot_inject_parser_keywords() {
    // NEGATIVE (the seed's silent-success): a pack body section that is
    // not in the closed table — here a lexer/keyword injection — refuses
    // typed. The section table is closed: pack source cannot add a
    // lexer token through admission.
    let source = "package community\n\nemath field_pack injector:\n    exports:\n        cell softmax\n    keywords:\n        add match\n".to_string();
    let mut session = session();
    let result = session.check_owned("injector", &source);
    let codes = error_codes(&result);
    assert!(
        codes.iter().any(|code| code == "E-SYN-101"),
        "a `keywords:` injection section must refuse, got {codes:?}"
    );
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/field_pack_declarations.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-SYN-101"),
        "seed expects the injection-section refusal, found: {expect_line}"
    );
}

#[test]
fn no_custom_fallthrough() {
    // The pack is never lowered into runnable meaning: it must not
    // appear in `package.declarations` (no hidden desugar, no silent
    // custom→strict fallthrough — the pack is artifact data only).
    let mut session = session();
    let result = session.check_owned("fallthrough", &spectral_pack_source());
    let codes = error_codes(&result);
    assert!(
        codes.is_empty(),
        "the admission itself is clean, got {codes:?}"
    );
    assert!(
        result.package.declarations.is_empty(),
        "a pack must not become a package declaration: {:?}",
        result
            .package
            .declarations
            .iter()
            .map(|declaration| declaration.name.leaf())
            .collect::<Vec<_>>()
    );
    assert!(
        !result
            .package
            .field_packs
            .iter()
            .any(|pack| pack.name == "spectral_style" && pack.exports.is_empty()),
        "the admitted pack keeps its declared exports"
    );
}

#[test]
fn field_pack_fixture_preserves_declared_exports() {
    let source =
        include_str!("../../../tests/fixtures/language/intro/field-pack-declarations.emath");
    let mut session = session();
    let result = session.check_owned("example", source);
    let codes = error_codes(&result);
    assert!(
        codes.is_empty(),
        "field-pack fixture must typecheck, got {codes:?} (messages: {:?})",
        result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(result.package.field_packs.len(), 1);
    assert_eq!(
        result.package.field_packs[0].exports,
        vec![
            ("cell".to_string(), "softmax".to_string()),
            ("theory".to_string(), "spectral".to_string()),
        ]
    );
}
