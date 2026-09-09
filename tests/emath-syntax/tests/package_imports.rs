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

const GEOMETRY: &str = "geometry.emath";
const MAIN: &str = "main.emath";

fn geometry_source() -> String {
    "emath function dist:\n    inputs:\n        x: Float64\n    outputs:\n        d: Float64\n    definitions:\n        d = x\n".to_string()
}

fn main_source() -> String {
    "package demo\n\nuse demo.geometry\n\nemath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x * x\n".to_string()
}

fn session_with(main: &str, geometry: Option<&str>) -> (CompilerSession, emath_core::FileId) {
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

use emath_test_harness::{Probe, boot};

#[test]
fn package_imports() {
    boot();
    let mut probe = Probe::new("Packages and imports: the `use` section resolves names across files in a package. The law: `use <package>.<module>` (the path prefix matching the");
    probe.case("use_import_resolves_across_files", |p| {
    let f0 = p.failures().len();

    // The file import admits: the merged admission contains BOTH files'
    // declarations under the main file's package identity, with zero
    // diagnostics (the merged tree proves the sibling's declarations
    // entered the checked tree; the duplicate test proves the merge is
    // real, not a silent drop).
    let (mut session, main) = session_with(&main_source(), Some(&geometry_source()));
    let result = session.check_package(main);
    let codes = error_codes(&result);
    p.demand("1",codes.is_empty(), format!(
        "merged package check admits with no errors, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }
    p.eq("2", result.package.package_path, Some(vec!["demo".to_string()]));
    let names: Vec<&str> = result
        .package
        .declarations
        .iter()
        .map(|declaration| declaration.name.leaf())
        .collect();
    p.demand("3",names.contains(&"dist"), format!(
        "the sibling file's declaration merged into the package: {names:?}"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("4",names.contains(&"P"), format!(
        "the main file's declaration is still admitted: {names:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("duplicate_imported_declarations_refuse", |p| {
    let f0 = p.failures().len();

    // Package-level merging with duplicate detection: the SAME
    // declaration name in the sibling and the main file refuses typed
    // through the normal lane — the merge cannot silently shadow.
    let duplicate = "emath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n";
    let (mut session, main) = session_with(&main_source(), Some(duplicate));
    let result = session.check_package(main);
    let codes = error_codes(&result);
    p.demand("1",codes.contains(&"E-NAME-022".to_string()), format!(
        "cross-file duplicate declaration must refuse E-NAME-022, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("unresolved_file_import_refuses", |p| {
    let f0 = p.failures().len();

    // NEGATIVE (the seed's silent-success): `use demo.missing` with no
    // such source loaded must refuse typed E-PKG-050 — never a silent
    // inert entry wearing an admitted label.
    let source = "package demo\n\nuse demo.missing\n\nemath function P:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n";
    let (mut session, main) = session_with(source, None);
    let result = session.check_package(main);
    let codes = error_codes(&result);
    p.demand("1",codes.contains(&"E-PKG-050".to_string()), format!(
        "unresolved file import must refuse E-PKG-050, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/package_import_missing.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    p.demand("2",expect_line.contains("E-PKG-050"), format!(
        "seed expects the unresolved file-import refusal, found: {expect_line}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("sibling_syntax_errors_surface", |p| {
    let f0 = p.failures().len();

    // A loaded sibling that does not parse surfaces its parse
    // diagnostics in the merged check (never a silent partial merge).
    let broken = "emath function dist:\n    inputs:\n        x: Float64\n    outputs:\n        d: Float64\n    definitions\n".to_string();
    let (mut session, main) = session_with(&main_source(), Some(&broken));
    let result = session.check_package(main);
    p.demand("1",!result.diagnostics.errors().next().is_none(), format!(
        "a broken sibling must surface errors"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("plain_file_check_is_unchanged", |p| {
    let f0 = p.failures().len();

    // Boundary: the plain single-file `check` keeps its existing
    // behavior — file imports are inert entries there, no E-PKG-050,
    // no merged declarations (the session method is purely additive).
    let (mut session, main) = session_with(&main_source(), Some(&geometry_source()));
    let result = session.check(main);
    let codes = error_codes(&result);
    p.demand("1",!codes.contains(&"E-PKG-050".to_string()), format!(
        "plain check keeps its lane: no new refusal, got {codes:?}"
    ));
    if p.failures().len() != f0 { return; }
    let names: Vec<&str> = result
        .package
        .declarations
        .iter()
        .map(|declaration| declaration.name.leaf())
        .collect();
    p.demand("2",!names.contains(&"dist"), format!(
        "plain check does not merge siblings: {names:?}"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
