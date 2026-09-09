//! Physics units, nabla, and Einstein-notation packages compose into
//! the `std::physics` field pack.
//!
//! The `std::physics` pack exports existing registry cells (leaf spellings
//! resolve canonically; nothing is reimplemented), and its installed
//! image's LOCK lists the packages it composes — physics::classical
//! laws, sci::physics::notation::nabla, sci::physics::notation::einstein.
//! Install runs on the existing registry and semantic-image builder:
//! zero core rebuild, zero core branches. Forked notation is refused
//! twice: injected parser keywords refuse at ADMISSION (E-SYN-101) and
//! a forked export (a nabla cell nobody provides) refuses at INSTALL
//! (E-PACK-002) — composition, never forking; install never fabricates.

mod physics_pack {
    use emath_core::limits::Limits;
    use emath_exec_ir::install::{PackError, PackRegistry, install_pack, install_pack_composing};
        use emath_sema::CompilerSession;
    use emath_syntax::install_source_parser;
use emath_test_harness::Probe;

fn std_cell_registry() -> std::collections::HashMap<String, emath_exec_ir::term_compile::CompiledCell> {
    // Language image is the registry now. Empty here is the honest hole:
    // chemistry pack install must resolve real cells, never a fabricated table.
    std::collections::HashMap::new()
}

    /// The std::physics pack: existing registry cells only (leaf
    /// spellings resolve canonically: add→std.math.add,
    /// norm→std.linalg.norm, inner_product→std.linalg.inner_product).
    const PHYSICS_PACK: &str = "package std\n\nemath field_pack physics:\n    exports:\n        cell add\n        cell exp\n        cell sqrt\n        cell norm\n        cell inner_product\n    metadata:\n        description composes units nabla einstein notation packages\n";

    /// The packages std::physics composes, as lock identities. All are
    /// EXISTING language packages; nothing is forked for this pack.
    const COMPOSED: &[&str] = &[
        "physics.classical@1.0.0",
        "sci.physics.notation.nabla@1.0.0",
        "sci.physics.notation.einstein@1.0.0",
    ];

    fn composed() -> Vec<String> {
        COMPOSED.iter().map(|s| s.to_string()).collect()
    }

    /// The composition seam: admission (`emath field_pack`) → exports →
    /// install tooling. Returns the admitted pack entry.
    fn admitted_pack(p: &mut Probe, source: &str) -> emath_ir::FieldPackEntry {
        install_source_parser();
        let mut session = CompilerSession::new(Limits::default());
        let result = session.check_owned("physics-pack", source);
        let codes: Vec<String> = result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code.to_string())
            .collect();
        p.demand("\"the pack admits at the language layer, got {codes:?}\"", codes.is_empty(), format!("the pack admits at the language layer, got {codes:?}"));
        let mut packs = result.package.field_packs;
        p.eq("packs.len()", &(packs.len()), &(1));
        packs.remove(0)
    }

    /// Happy path: std::physics installs from the EXISTING registry (no
    /// rebuild), and its image LOCK lists every composed package plus
    /// itself — the lock is the composition record.


    /// Boundary: a composed list that already contains the pack's own
    /// identity dedups — the lock lists it exactly once. `use` then
    /// resolves the installed pack against the registry.


    /// NEGATIVE (the fork shape): a "physics" pack that forks notation
    /// by injecting parser keywords refuses at ADMISSION (E-SYN-101,
    /// closed section table) and yields no installable data. The
    /// tests/invalid seed declares the same expectation.


    /// NEGATIVE (silent-success shape): a pack exporting a FORKED nabla
    /// cell the registry does not provide refuses typed at install —
    /// install never fabricates a notation cell nobody compiled.
    #[test]
    fn probe() {
        let mut probe = Probe::new("physics pack composition: every check in one probe");
        probe.case("physics_pack_composes_and_lock_lists_packages", |probe| {

            let entry = admitted_pack(probe, PHYSICS_PACK);
            let installed = install_pack_composing(
                &entry,
                &["std".to_string()],
                &std_cell_registry(),
                &composed(),
            )
            .expect("the composing pack installs without any core rebuild");
            probe.eq("installed.package", &(installed.package), &(vec!["std".to_string()]));
            probe.eq("installed.pack", &(installed.pack), &("physics"));
            // Exports are the EXISTING canonical registry cells, source order.
            probe.eq("installed.exports", &(installed.exports), &(vec![
                    "std.math.add".to_string(),
                    "std.math.exp".to_string(),
                    "std.math.sqrt".to_string(),
                    "std.linalg.norm".to_string(),
                    "std.linalg.inner_product".to_string(),
                ]));
            installed
                .image
                .validate_partitions()
                .expect("the installed image is self-validating");
            // The invariant: the lock lists the composed existing packages.
            let lock = installed.image.load("lock").expect("lock page");
            for composed in COMPOSED {
                probe.demand("\"lock must list composed package {composed}: {lock}\"", lock.contains(composed), format!("lock must list composed package {composed}: {lock}"));
            }
            probe.demand("\"own identity in lock: {lock}\"", lock.contains("physics@0.1.0"), format!("own identity in lock: {lock}"));
            // The cells page holds the existing registry cells, unchanged.
            let cells = installed.image.load("cells").expect("cells page");
            probe.demand("\"{cells}\"", cells.contains("cell:std.linalg.norm"), format!("{cells}"));
            probe.demand("\"{cells}\"", cells.contains("cell:std.math.add"), format!("{cells}"));
        });
        probe.case("lock_dedups_own_identity_and_use_resolves", |probe| {

            let entry = admitted_pack(probe, PHYSICS_PACK);
            let mut composed: Vec<String> = COMPOSED.iter().map(|s| s.to_string()).collect();
            composed.push("physics@0.1.0".to_string());
            let installed = install_pack_composing(
                &entry,
                &["std".to_string()],
                &std_cell_registry(),
                &composed,
            )
            .expect("installs");
            let lock = installed.image.load("lock").expect("lock page");
            probe.eq("lock.matches(\"physics@0.1.0\").count()", &(lock.matches("physics@0.1.0").count()), &(1));
            let mut registry = PackRegistry::new();
            registry.install(installed);
            let used = registry
                .resolve_use(&["std".to_string(), "physics".to_string()])
                .expect("use std.physics resolves the installed pack");
            probe.eq("used.pack", &(used.pack), &("physics"));
        });
        probe.case("forked_notation_refuses_at_admission", |probe| {

            let source = "package std\n\nemath field_pack physics_fork:\n    exports:\n        cell nabla_stencil\n    keywords:\n        nabla div\n"
                .to_string();
            install_source_parser();
            let mut session = CompilerSession::new(Limits::default());
            let result = session.check_owned("physics-fork", &source);
            let codes: Vec<String> = result
                .diagnostics
                .errors()
                .map(|diagnostic| diagnostic.code.to_string())
                .collect();
            probe.demand("\"keyword injection refuses at admission, got {codes:?}\"", codes.contains(&"E-SYN-101".to_string()), format!("keyword injection refuses at admission, got {codes:?}"));
            probe.demand("\"a refused pack yields no installable data\"", result.package.field_packs.is_empty(), "a refused pack yields no installable data");
            const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/physics_pack.emath");
            let expect_line = NEGATIVE_SEED
                .lines()
                .find(|l| l.trim_start().starts_with("# expect:"))
                .expect("seed declares its diagnostic");
            probe.demand("\"seed expects the fork refusal, found: {expect_line}\"", expect_line.contains("E-SYN-101"), format!("seed expects the fork refusal, found: {expect_line}"));
        });
        probe.case("forked_export_refuses_at_install", |probe| {

            let source = "package std\n\nemath field_pack physics_fork:\n    exports:\n        cell nabla_stencil\n"
                .to_string();
            let entry = admitted_pack(probe, &source);
            match install_pack_composing(&entry, &["std".to_string()], &std_cell_registry(), &composed())
            {
                Err(PackError::UnknownExport { export }) => { probe.eq("export", &(export), &("nabla_stencil")); },
                other => panic!("forked export must refuse at install, got {other:?}"),
            }
            // And the plain (non-composing) install path still refuses the
            // same way — composition extends install, it does not fork it.
            match install_pack(&entry, &["std".to_string()], &std_cell_registry()) {
                Err(PackError::UnknownExport { export }) => { probe.eq("export", &(export), &("nabla_stencil")); },
                other => panic!("plain install must refuse identically, got {other:?}"),
            }
        });
        probe.finish();
    }

}
