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
    use emath_exec_ir::term_compile::std_cell_registry;
    use emath_sema::CompilerSession;
    use emath_syntax::install_source_parser;

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
    fn admitted_pack(source: &str) -> emath_ir::FieldPackEntry {
        install_source_parser();
        let mut session = CompilerSession::new(Limits::default());
        let result = session.check_owned("physics-pack", source);
        let codes: Vec<String> = result
            .diagnostics
            .errors()
            .map(|diagnostic| diagnostic.code.to_string())
            .collect();
        assert!(
            codes.is_empty(),
            "the pack admits at the language layer, got {codes:?}"
        );
        let mut packs = result.package.field_packs;
        assert_eq!(packs.len(), 1, "one field_pack admitted");
        packs.remove(0)
    }

    /// Happy path: std::physics installs from the EXISTING registry (no
    /// rebuild), and its image LOCK lists every composed package plus
    /// itself — the lock is the composition record.
    #[test]
    fn physics_pack_composes_and_lock_lists_packages() {
        let entry = admitted_pack(PHYSICS_PACK);
        let installed = install_pack_composing(
            &entry,
            &["std".to_string()],
            &std_cell_registry(),
            &composed(),
        )
        .expect("the composing pack installs without any core rebuild");
        assert_eq!(installed.package, vec!["std".to_string()]);
        assert_eq!(installed.pack, "physics");
        // Exports are the EXISTING canonical registry cells, source order.
        assert_eq!(
            installed.exports,
            vec![
                "std.math.add".to_string(),
                "std.math.exp".to_string(),
                "std.math.sqrt".to_string(),
                "std.linalg.norm".to_string(),
                "std.linalg.inner_product".to_string(),
            ]
        );
        installed
            .image
            .validate_partitions()
            .expect("the installed image is self-validating");
        // The invariant: the lock lists the composed existing packages.
        let lock = installed.image.load("lock").expect("lock page");
        for composed in COMPOSED {
            assert!(
                lock.contains(composed),
                "lock must list composed package {composed}: {lock}"
            );
        }
        assert!(lock.contains("physics@0.1.0"), "own identity in lock: {lock}");
        // The cells page holds the existing registry cells, unchanged.
        let cells = installed.image.load("cells").expect("cells page");
        assert!(cells.contains("cell:std.linalg.norm"), "{cells}");
        assert!(cells.contains("cell:std.math.add"), "{cells}");
    }

    /// Boundary: a composed list that already contains the pack's own
    /// identity dedups — the lock lists it exactly once. `use` then
    /// resolves the installed pack against the registry.
    #[test]
    fn lock_dedups_own_identity_and_use_resolves() {
        let entry = admitted_pack(PHYSICS_PACK);
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
        assert_eq!(
            lock.matches("physics@0.1.0").count(),
            1,
            "own identity appears exactly once: {lock}"
        );
        let mut registry = PackRegistry::new();
        registry.install(installed);
        let used = registry
            .resolve_use(&["std".to_string(), "physics".to_string()])
            .expect("use std.physics resolves the installed pack");
        assert_eq!(used.pack, "physics");
    }

    /// NEGATIVE (the fork shape): a "physics" pack that forks notation
    /// by injecting parser keywords refuses at ADMISSION (E-SYN-101,
    /// closed section table) and yields no installable data. The
    /// tests/invalid seed declares the same expectation.
    #[test]
    fn forked_notation_refuses_at_admission() {
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
        assert!(
            codes.contains(&"E-SYN-101".to_string()),
            "keyword injection refuses at admission, got {codes:?}"
        );
        assert!(
            result.package.field_packs.is_empty(),
            "a refused pack yields no installable data"
        );
        const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/physics_pack.emath");
        let expect_line = NEGATIVE_SEED
            .lines()
            .find(|l| l.trim_start().starts_with("# expect:"))
            .expect("seed declares its diagnostic");
        assert!(
            expect_line.contains("E-SYN-101"),
            "seed expects the fork refusal, found: {expect_line}"
        );
    }

    /// NEGATIVE (silent-success shape): a pack exporting a FORKED nabla
    /// cell the registry does not provide refuses typed at install —
    /// install never fabricates a notation cell nobody compiled.
    #[test]
    fn forked_export_refuses_at_install() {
        let source = "package std\n\nemath field_pack physics_fork:\n    exports:\n        cell nabla_stencil\n"
            .to_string();
        let entry = admitted_pack(&source);
        match install_pack_composing(&entry, &["std".to_string()], &std_cell_registry(), &composed())
        {
            Err(PackError::UnknownExport { export }) => assert_eq!(export, "nabla_stencil"),
            other => panic!("forked export must refuse at install, got {other:?}"),
        }
        // And the plain (non-composing) install path still refuses the
        // same way — composition extends install, it does not fork it.
        match install_pack(&entry, &["std".to_string()], &std_cell_registry()) {
            Err(PackError::UnknownExport { export }) => assert_eq!(export, "nabla_stencil"),
            other => panic!("plain install must refuse identically, got {other:?}"),
        }
    }
}
