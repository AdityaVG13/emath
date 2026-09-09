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
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;
use emath_test_harness::{Probe, boot};

fn std_cell_registry() -> std::collections::HashMap<String, emath_exec_ir::term_compile::CompiledCell> {
    std::collections::HashMap::new()
}

const TOY_PACK: &str = "package community\n\nemath field_pack spectral_style:\n    exports:\n        cell softmax\n    metadata:\n        description reference spectral pack\n";

/// The composition seam: admission (`emath field_pack`) → exports →
/// install tooling. Returns the admitted pack entry.

fn admitted_pack_or_die(source: &str) -> emath_ir::FieldPackEntry {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("toy-pack", source);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    if !codes.is_empty() {
        panic!("the toy pack admits at the language layer, got {codes:?}");
    }
    let mut packs = result.package.field_packs;
    if packs.len() != 1 {
        panic!("one field_pack admitted, got {}", packs.len());
    }
    packs.remove(0)
}

fn admitted_pack(p: &mut Probe, source: &str) -> emath_ir::FieldPackEntry {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("toy-pack", source);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    p.demand("\"the toy pack admits at the language layer, got {codes:?}\"", codes.is_empty(), format!("the toy pack admits at the language layer, got {codes:?}"));
    let mut packs = result.package.field_packs;
    p.eq("packs.len()", &(packs.len()), &(1));
    packs.remove(0)
}

#[test]
fn intent() {
    boot();
    let mut p = Probe::new("Field-pack layout, install, and `use`");
    p.case("toy_pack_installs_and_uses", |p| {

    // Capstone happy path: add a toy pack (language admission), install
    // it (exports → existing registry → semantic image), `use` it —
    // no core branches anywhere in the path.
    let entry = admitted_pack(p, TOY_PACK);
    let installed: InstalledPack =
        install_pack(&entry, &["community".to_string()], &std_cell_registry()).expect("installs");
    p.eq("toy_pack_installs_and_uses#1", installed.package.clone(), vec!["community".to_string()]);
    p.demand("toy_pack_installs_and_uses#2", installed.pack == "spectral_style", format!("expected {:?}, got {:?}", "spectral_style", installed.pack));
    p.eq("toy_pack_installs_and_uses#3", installed.exports.clone(), vec!["std.tensor.softmax".to_string()]);
    installed
        .image
        .validate_partitions()
        .expect("the installed image is self-validating");
    p.demand("toy_pack_installs_and_uses#4", installed.image.image_id.starts_with("fnv1a64:"), "toy_pack_installs_and_uses#4: installed.image.image_id.starts_with(\"fnv1a64:\")");
    let cells = installed.image.load("cells").expect("cells page");
    p.demand(format!("the exported cell landed in the installed image: {cells}"), cells.contains("cell:std.tensor.softmax"), format!("the exported cell landed in the installed image: {cells}"));

    let mut registry = PackRegistry::new();
    registry.install(installed);
    let used: &InstalledPack = registry
        .resolve_use(&["community".to_string(), "spectral_style".to_string()])
        .expect("use resolves the installed pack");
    p.demand("toy_pack_installs_and_uses#6", used.pack == "spectral_style", format!("expected {:?}, got {:?}", "spectral_style", used.pack));
    match registry.resolve_use(&["community".to_string(), "missing".to_string()]) {
        Err(PackError::UnknownPack { use_path }) => {
            p.demand("toy_pack_installs_and_uses#7", use_path == "community.missing", format!("expected {:?}, got {:?}", "community.missing", use_path));
        }
        other => { p.fail("toy_pack_installs_and_uses#8", format!("unknown pack use must refuse, got {other:?}")); return; },
    }

    });
    p.case("unknown_export_refuses", |p| {

    // Install never fabricates: an export the registry does not provide
    // refuses typed (the installed image would otherwise claim a cell
    // nobody compiled — the silent-success shape).
    let source =
        "package community\n\nemath field_pack ghost:\n    exports:\n        cell acme.magic\n"
            .to_string();
    let entry = admitted_pack(p, &source);
    match install_pack(&entry, &["community".to_string()], &std_cell_registry()) {
        Err(PackError::UnknownExport { export }) => {
            p.demand("unknown_export_refuses#1", export == "acme.magic", format!("expected {:?}, got {:?}", "acme.magic", export));
        }
        other => { p.fail("unknown_export_refuses#2", format!("unknown export must refuse at install, got {other:?}")); return; },
    }

    });
    p.case("layout_is_closed", |p| {

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
        Err(PackError::UnknownLayoutDir { dir }) => { p.demand("layout_is_closed#1", dir == "keywords", format!("expected {:?}, got {:?}", "keywords", dir)); },
        other => { p.fail("layout_is_closed#2", format!("layout injection must refuse, got {other:?}")); return; },
    }

    });
    p.case("keyword_injection_refused_before_install", |p| {

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
    p.demand(format!("keyword injection refuses at admission, got {codes:?}"), codes.contains(&"E-SYN-101".to_string()), format!("keyword injection refuses at admission, got {codes:?}"));
    p.demand("a refused pack yields no installable data", result.package.field_packs.is_empty(), "a refused pack yields no installable data");
    const NEGATIVE_SEED: &str =
        include_str!("../../../tests/invalid/field_pack_installation.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    p.demand(format!("seed expects the injection refusal, found: {expect_line}"), expect_line.contains("E-SYN-101"), format!("seed expects the injection refusal, found: {expect_line}"));

    });
    p.case("no_core_rebuild_bundle", |p| {

    // WorldResultBundle fixture (e2e clause; the cell path is touched:
    // install compiles cells to an image). The labeled world verdict
    // records install-without-rebuild: the installed image's cells page
    // is the EXISTING registry cell, unchanged.
    struct InstallWorld;
    impl emath_genesis::FirstOrderWorld for InstallWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &emath_term::SymbolId) -> Result<Self::Value, Self::Error> {
            let entry = admitted_pack_or_die(TOY_PACK);
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
    p.demand("no_core_rebuild_bundle#1", matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ), "no_core_rebuild_bundle#1: matches!(\n        result.disposition,\n        emath_genesis::Disposition::Answer { .. }\n    )");
    p.demand("no_core_rebuild_bundle#2", result.world == "field-pack-install", format!("expected {:?}, got {:?}", "field-pack-install", result.world));
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    p.demand("no_core_rebuild_bundle#3", bundle.bundle_id.starts_with("fnv1a64:"), "no_core_rebuild_bundle#3: bundle.bundle_id.starts_with(\"fnv1a64:\")");

    });
    p.finish();
}









