//! Field-pack layout, install, and `use`
//! without rebuilding core.
//!
//! The capstone's law: add a toy pack, `use` it, NO core branches. The
//! pack's admission (`emath field_pack`) yields export
//! DATA; this tooling compiles the exported cells to a semantic
//! image (`.emlib`) from the EXISTING registry — no compiler rebuild,
//! no core branches — and `use <package>.<pack>` resolves against the
//! installed pack registry. Layout is a closed directory set: a pack
//! cannot smuggle new surface (e.g. a `keywords/` dir) into an install,
//! and the parser-keyword injection inside pack SOURCE is refused at
//! admission (E-SYN-101) before any install consumes it.

use emath_core::limits::Limits;
use emath_exec_ir::install::{
    InstalledPack, PackError, PackRegistry, install_pack, validate_layout,
};
use emath_exec_ir::term_compile::std_cell_registry;
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

const TOY_PACK: &str = "package community\n\nemath field_pack spectral_style:\n    exports:\n        cell softmax\n    metadata:\n        description reference spectral pack\n";

/// The composition seam: admission (`emath field_pack`) → exports →
/// install tooling. Returns the admitted pack entry.
fn admitted_pack(source: &str) -> emath_ir::FieldPackEntry {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("toy-pack", source);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    assert!(
        codes.is_empty(),
        "the toy pack admits at the language layer, got {codes:?}"
    );
    let mut packs = result.package.field_packs;
    assert_eq!(packs.len(), 1, "one field_pack admitted");
    packs.remove(0)
}

#[test]
fn toy_pack_installs_and_uses() {
    // Capstone happy path: add a toy pack (language admission), install
    // it (exports → existing registry → semantic image), `use` it —
    // no core branches anywhere in the path.
    let entry = admitted_pack(TOY_PACK);
    let installed: InstalledPack =
        install_pack(&entry, &["community".to_string()], &std_cell_registry()).expect("installs");
    assert_eq!(installed.package, vec!["community".to_string()]);
    assert_eq!(installed.pack, "spectral_style");
    assert_eq!(installed.exports, vec!["std.tensor.softmax".to_string()]);
    installed
        .image
        .validate_partitions()
        .expect("the installed image is self-validating");
    assert!(installed.image.image_id.starts_with("fnv1a64:"));
    let cells = installed.image.load("cells").expect("cells page");
    assert!(
        cells.contains("cell:std.tensor.softmax"),
        "the exported cell landed in the installed image: {cells}"
    );

    let mut registry = PackRegistry::new();
    registry.install(installed);
    let used: &InstalledPack = registry
        .resolve_use(&["community".to_string(), "spectral_style".to_string()])
        .expect("use resolves the installed pack");
    assert_eq!(used.pack, "spectral_style");
    match registry.resolve_use(&["community".to_string(), "missing".to_string()]) {
        Err(PackError::UnknownPack { use_path }) => {
            assert_eq!(use_path, "community.missing");
        }
        other => panic!("unknown pack use must refuse, got {other:?}"),
    }
}

#[test]
fn unknown_export_refuses() {
    // Install never fabricates: an export the registry does not provide
    // refuses typed (the installed image would otherwise claim a cell
    // nobody compiled — the silent-success shape).
    let source =
        "package community\n\nemath field_pack ghost:\n    exports:\n        cell acme.magic\n"
            .to_string();
    let entry = admitted_pack(&source);
    match install_pack(&entry, &["community".to_string()], &std_cell_registry()) {
        Err(PackError::UnknownExport { export }) => {
            assert_eq!(export, "acme.magic");
        }
        other => panic!("unknown export must refuse at install, got {other:?}"),
    }
}

#[test]
fn layout_is_closed() {
    // The pack layout is a CLOSED directory set (the fixed
    // layout); a directory outside it — e.g. a `keywords/` injection —
    // refuses typed at the tooling boundary.
    validate_layout(&[
        "src",
        "worlds",
        "methods",
        "examples",
        "providers",
        "migrations",
    ])
    .expect("the fixed layout admits");
    match validate_layout(&["src", "keywords"]) {
        Err(PackError::UnknownLayoutDir { dir }) => assert_eq!(dir, "keywords"),
        other => panic!("layout injection must refuse, got {other:?}"),
    }
}

#[test]
fn keyword_injection_refused_before_install() {
    // NEGATIVE (the seed's silent-success): pack source that injects
    // parser keywords refuses at ADMISSION (E-SYN-101, the
    // closed section table) — install only ever consumes admitted
    // FieldPackEntry data, so the injection never reaches tooling.
    let source = "package community\n\nemath field_pack injector:\n    exports:\n        cell softmax\n    keywords:\n        add match\n".to_string();
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("injector", &source);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    assert!(
        codes.contains(&"E-SYN-101".to_string()),
        "keyword injection refuses at admission, got {codes:?}"
    );
    assert!(
        result.package.field_packs.is_empty(),
        "a refused pack yields no installable data"
    );
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/field_pack_installation.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-SYN-101"),
        "seed expects the injection refusal, found: {expect_line}"
    );
}

#[test]
fn no_core_rebuild_bundle() {
    // WorldResultBundle fixture (e2e clause; the cell path is touched:
    // install compiles cells to an image). The labeled world verdict
    // records install-without-rebuild: the installed image's cells page
    // is the EXISTING registry cell, unchanged.
    struct InstallWorld;
    impl emath_genesis::FirstOrderWorld for InstallWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &emath_term::SymbolId) -> Result<Self::Value, Self::Error> {
            let entry = admitted_pack(TOY_PACK);
            let installed = install_pack(&entry, &["community".to_string()], &std_cell_registry())
                .expect("installs");
            let cells = installed.image.load("cells").expect("cells page");
            if cells.contains("cell:std.tensor.softmax")
                && installed.image.validate_partitions().is_ok()
                && installed.package == vec!["community".to_string()]
            {
                Ok("installed-without-rebuild".to_string())
            } else {
                Ok("install-diverged".to_string())
            }
        }

        fn apply(
            &self,
            operator: &emath_term::SymbolId,
            _arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            Err(emath_genesis::EvalError::UnknownSymbol(operator.clone()))
        }

        fn evidence(&self) -> emath_genesis::WorldEvidence {
            emath_genesis::WorldEvidence::seed(
                "field-pack-install",
                &["no-core-branches", "closed-layout"],
            )
        }
    }

    let term = emath_term::Term::Constant(emath_term::SymbolId("install[toy]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &InstallWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "field-pack-install");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
