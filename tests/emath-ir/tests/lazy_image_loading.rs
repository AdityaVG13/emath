//! emath-epic-machine-fjxh.10: Lazy image loading, prelude-only
//! startup, WASM chunks.
//!
//! The bead's law: startup must not compile (or load) all installed
//! fields. A lazy session boots with the nucleus + the prelude index
//! (each installed image's lock page); a file compile loads the
//! REACHABLE packs' pages on demand; the initialization receipt names
//! exactly the loaded pages; an UNUSED pack's pages never load, and any
//! attempt to serve one refuses typed (`E-LAZY-001` — the negative
//! seed's silent-success: an eager loader wearing a lazy label).
//! Unknown packs refuse typed (`E-LAZY-002`) at boot and at compile.
//! The packs that stay unloaded are the artifact's optional WASM
//! chunks, named deterministically.

use std::collections::BTreeMap;

use emath_exec_ir::image::{ImageLock, ImageWorld, SemanticImage};
use emath_exec_ir::lazy::{LazyError, LoadProfile, LazySession, optional_chunks};
use emath_exec_ir::term_compile::{ParamShape, compile_reference, std_cell_registry};
use emath_term::{Signature, SymbolId, Term, VariableId};

const FIELD_A: &str = "field-a";
const FIELD_B: &str = "field-b";

/// One minimal field pack image: a single registry cell, real fjxh.9
/// builder, deterministic.
fn field_pack(pack_name: &str, capability: &str) -> SemanticImage {
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("1.0".into()), 0usize)
        .expect("conflict-free");
    let term = Term::Constant(SymbolId("1.0".into()));
    let _ = (&term, &signature, VariableId("x".into()));
    let cell = compile_reference(
        &term,
        &signature,
        &[("x".to_string(), ParamShape::Scalar)],
        Vec::new(),
        capability,
    )
    .expect("pack cell compiles");
    let mut docs = BTreeMap::new();
    docs.insert(capability.to_string(), "pack cell".to_string());
    SemanticImage::build(
        pack_name,
        &[cell],
        &[ImageWorld {
            world: "reference-vm".to_string(),
            origin: "seed".to_string(),
            laws: vec!["prelude-only-startup".to_string()],
        }],
        &docs,
        ImageLock {
            prelude: vec!["std.prelude.core@1.0.0".to_string()],
            packs: vec![format!("{pack_name}@0.1.0")],
            images: vec![],
            toolchain: "emath-toolchain@0.1.0".to_string(),
        },
    )
    .expect("field pack builds")
}

fn installed() -> Vec<SemanticImage> {
    let registry = std_cell_registry();
    let a = registry
        .get("std.math.add")
        .expect("add")
        .capability
        .clone();
    let b = registry
        .get("std.tensor.sum")
        .expect("sum")
        .capability
        .clone();
    vec![field_pack(FIELD_A, &a), field_pack(FIELD_B, &b)]
}

#[test]
fn prelude_only_boot_receipt() {
    // Minimal profile: nucleus + each installed image's prelude index
    // (lock) page — NEVER a field page. The receipt is the proof.
    let images = installed();
    let session = LazySession::boot(&images, LoadProfile::Minimal).expect("boots");
    let receipt = session.receipt();
    assert!(receipt.names_root("nucleus"), "the nucleus root is loaded");
    assert!(receipt.names(FIELD_A, "lock"), "prelude index page loads");
    assert!(receipt.names(FIELD_B, "lock"), "prelude index page loads");
    assert!(
        !receipt.names(FIELD_A, "cells") && !receipt.names(FIELD_A, "worlds.bytecode"),
        "no field page loads at startup"
    );
    assert_eq!(receipt.loaded_pages().len(), 3);
}

#[test]
fn file_compile_loads_only_reachable_packs() {
    // A file compile reaching only field-a loads exactly field-a's
    // pages; field-b's pages never load (capstone lazy-field-loading).
    let images = installed();
    let mut session = LazySession::boot(&images, LoadProfile::Minimal).expect("boots");
    let receipt = session
        .load_for_compile(&[FIELD_A])
        .expect("reachable pack loads");
    assert!(receipt.names(FIELD_A, "cells"));
    assert!(receipt.names(FIELD_A, "worlds.bytecode"));
    assert!(
        !receipt.names(FIELD_B, "cells"),
        "the unused field never loads"
    );
    // The loaded page is servable; the unloaded one is not.
    assert!(session.page(FIELD_A, "cells").is_ok());
}

#[test]
fn unused_pack_access_is_detected() {
    // NEGATIVE (the seed's silent-success): serving a page from a pack
    // the session never loaded must refuse typed — never silently fall
    // back to eager loading.
    let images = installed();
    let mut session = LazySession::boot(&images, LoadProfile::Minimal).expect("boots");
    let _ = session.load_for_compile(&[FIELD_A]).expect("compiles");
    match session.page(FIELD_B, "cells") {
        Err(LazyError::UnloadedPackAccess { pack, page }) => {
            assert_eq!(pack, FIELD_B);
            assert_eq!(page, "cells");
        }
        other => panic!("unused pack access must refuse, got {other:?}"),
    }
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/lazy_image_loading.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-LAZY-001"),
        "seed expects the unused-pack refusal, found: {expect_line}"
    );
}

#[test]
fn unknown_pack_refused_typed() {
    // A compile that names a pack nobody installed refuses typed at the
    // loader boundary (never a silent no-op receipt).
    let images = installed();
    let mut session = LazySession::boot(&images, LoadProfile::Minimal).expect("boots");
    match session.load_for_compile(&["acme.missing"]) {
        Err(LazyError::UnknownPack { pack }) => assert_eq!(pack, "acme.missing"),
        other => panic!("unknown pack must refuse, got {other:?}"),
    }
    // The same law at boot: a custom profile naming an uninstalled pack.
    match LazySession::boot(&images, LoadProfile::Custom(vec!["acme.missing".to_string()])) {
        Err(LazyError::UnknownPack { pack }) => assert_eq!(pack, "acme.missing"),
        other => panic!("custom profile with unknown pack must refuse, got {other:?}"),
    }
}

#[test]
fn profiles_admit_distinct_sets() {
    // minimal: locks only. standard: every installed pack's pages.
    // custom: exactly the named packs' pages (plus the shared roots).
    let images = installed();
    let standard = LazySession::boot(&images, LoadProfile::Standard).expect("boots");
    assert!(standard.receipt().names(FIELD_A, "cells"));
    assert!(standard.receipt().names(FIELD_B, "worlds.bytecode"));
    let custom =
        LazySession::boot(&images, LoadProfile::Custom(vec![FIELD_B.to_string()]))
            .expect("boots");
    assert!(custom.receipt().names(FIELD_B, "cells"));
    assert!(
        !custom.receipt().names(FIELD_A, "cells"),
        "custom admits only the named packs"
    );
}

#[test]
fn unloaded_packs_are_wasm_optional_chunks() {
    // The unloaded packs ARE the optional WASM chunks, named
    // deterministically (sorted); loading one shrinks the chunk set.
    let images = installed();
    let mut session = LazySession::boot(&images, LoadProfile::Minimal).expect("boots");
    assert_eq!(optional_chunks(&images, &session.receipt()), vec![FIELD_A, FIELD_B]);
    let _ = session.load_for_compile(&[FIELD_A]).expect("compiles");
    assert_eq!(
        optional_chunks(&images, &session.receipt()),
        vec![FIELD_B],
        "a loaded pack is no longer an optional chunk"
    );

    struct LazyWorld;
    impl emath_genesis::FirstOrderWorld for LazyWorld {
        type Value = String;
        type Error = emath_genesis::EvalError;

        fn constant(&self, _symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            let images = installed();
            let mut session =
                LazySession::boot(&images, LoadProfile::Minimal).expect("boots");
            let before = optional_chunks(&images, &session.receipt()).len();
            let _ = session.load_for_compile(&[FIELD_A]).expect("compiles");
            let after = optional_chunks(&images, &session.receipt()).len();
            if before == 2 && after == 1 && !session.receipt().names(FIELD_B, "cells") {
                Ok("lazy-loaded".to_string())
            } else {
                Ok("eager-leak".to_string())
            }
        }

        fn apply(
            &self,
            operator: &SymbolId,
            _arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            Err(emath_genesis::EvalError::UnknownSymbol(operator.clone()))
        }

        fn evidence(&self) -> emath_genesis::WorldEvidence {
            emath_genesis::WorldEvidence::seed(
                "prelude-only-startup",
                &["unused-pages-never-load", "deterministic-receipt"],
            )
        }
    }

    let term = Term::Constant(SymbolId("lazy[fixture]".into()));
    let environment = emath_genesis::Environment::<String>::new();
    let result = emath_genesis::evaluate_labeled(
        &term,
        &LazyWorld,
        &environment,
        emath_genesis::WorldBudget { max_steps: 8 },
        |verdict: &String| verdict.clone(),
    );
    assert!(matches!(
        result.disposition,
        emath_genesis::Disposition::Answer { .. }
    ));
    assert_eq!(result.world, "prelude-only-startup");
    let bundle = emath_genesis::ResultBundle::new(vec![result]).expect("labeled result");
    assert!(bundle.bundle_id.starts_with("fnv1a64:"));
}
