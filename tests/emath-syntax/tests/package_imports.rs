//! Packages and imports: the `use`
//! section resolves names across files in a package.
//!
//! The law: `use <package>.<module>` (the path prefix matching
//! the file's own `package` line) resolves the sibling file's
//! declarations into the importing file's admission (package-level
//! declaration merging), duplicate declaration names across files
//! refuse through the normal lane (`E-NAME-022`), and an in-package
//! module import that does not resolve to a loaded source refuses
//! typed (`E-PKG-050` — never a silent inert entry, the negative
//! seed's silent-success). The whole path is session-level: the plain
//! single-file `check` keeps its existing behavior (module imports
//! stay inert entries there — this slice changes no existing lane).

use emath_core::limits::Limits;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

const GEOMETRY: &str = "geometry.emath";
const MAIN: &str = "main.emath";

fn geometry_source() -> String {
    "emath function dist:\n    inputs:\n        x: Float64\n    outputs:\n        d: Float64\n    definitions:\n        d = x\n".to_string()
}

fn main_source() -> String {
    "package demo\n\nuse demo.geometry\n\nemath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n".to_string()
}

fn session_with(main: &str, geometry: Option<&str>) -> (CompilerSession, emath_core::FileId) {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let main_id = session.load_text(MAIN, main);
    if let Some(geometry) = geometry {
        session.load_text(GEOMETRY, geometry);
    }
    (session, main_id)
}

fn error_codes(result: &emath_sema::CheckResult) -> Vec<String> {
    result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect()
}

#[test]
fn use_import_resolves_across_files() {
    // The file import admits: the merged admission contains BOTH files'
    // declarations under the main file's package identity, with zero
    // diagnostics (the merged tree proves the sibling's declarations
    // entered the checked tree; the duplicate test proves the merge is
    // real, not a silent drop).
    let (mut session, main) = session_with(&main_source(), Some(&geometry_source()));
    let result = session.check_package(main);
    let codes = error_codes(&result);
    assert!(
        codes.is_empty(),
        "merged package check admits with no errors, got {codes:?}"
    );
    assert_eq!(
        result.package.package_path,
        Some(vec!["demo".to_string()]),
        "identity comes from the main file's package line"
    );
    let names: Vec<&str> = result
        .package
        .declarations
        .iter()
        .map(|declaration| declaration.name.leaf())
        .collect();
    assert!(
        names.contains(&"dist"),
        "the sibling file's declaration merged into the package: {names:?}"
    );
    assert!(
        names.contains(&"P"),
        "the main file's declaration is still admitted: {names:?}"
    );
}

#[test]
fn duplicate_imported_declarations_refuse() {
    // Package-level merging with duplicate detection: the SAME
    // declaration name in the sibling and the main file refuses typed
    // through the normal lane — the merge cannot silently shadow.
    let duplicate = "emath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n";
    let (mut session, main) = session_with(&main_source(), Some(duplicate));
    let result = session.check_package(main);
    let codes = error_codes(&result);
    assert!(
        codes.contains(&"E-NAME-022".to_string()),
        "cross-file duplicate declaration must refuse E-NAME-022, got {codes:?}"
    );
}

#[test]
fn unresolved_file_import_refuses() {
    // NEGATIVE (the seed's silent-success): `use demo.missing` with no
    // such source loaded must refuse typed E-PKG-050 — never a silent
    // inert entry wearing an admitted label.
    let source = "package demo\n\nuse demo.missing\n\nemath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n";
    let (mut session, main) = session_with(source, None);
    let result = session.check_package(main);
    let codes = error_codes(&result);
    assert!(
        codes.contains(&"E-PKG-050".to_string()),
        "unresolved file import must refuse E-PKG-050, got {codes:?}"
    );
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/package_import_missing.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-PKG-050"),
        "seed expects the unresolved file-import refusal, found: {expect_line}"
    );
}

#[test]
fn sibling_syntax_errors_surface() {
    // A loaded sibling that does not parse surfaces its parse
    // diagnostics in the merged check (never a silent partial merge).
    let broken = "emath function dist:\n    inputs:\n        x: Float64\n    outputs:\n        d: Float64\n    definitions\n".to_string();
    let (mut session, main) = session_with(&main_source(), Some(&broken));
    let result = session.check_package(main);
    assert!(
        !result.diagnostics.errors().next().is_none(),
        "a broken sibling must surface errors"
    );
}

#[test]
fn plain_file_check_is_unchanged() {
    // Boundary: the plain single-file `check` keeps its existing
    // behavior — file imports are inert entries there, no E-PKG-050,
    // no merged declarations (the session method is purely additive).
    let (mut session, main) = session_with(&main_source(), Some(&geometry_source()));
    let result = session.check(main);
    let codes = error_codes(&result);
    assert!(
        !codes.contains(&"E-PKG-050".to_string()),
        "plain check keeps its lane: no new refusal, got {codes:?}"
    );
    let names: Vec<&str> = result
        .package
        .declarations
        .iter()
        .map(|declaration| declaration.name.leaf())
        .collect();
    assert!(
        !names.contains(&"dist"),
        "plain check does not merge siblings: {names:?}"
    );
}
