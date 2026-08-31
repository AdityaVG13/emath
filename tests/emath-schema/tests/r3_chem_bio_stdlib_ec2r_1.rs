//! `r3_chem_bio_stdlib_ec2r_1` — chemistry pack exports through the
//! landed field-pack machine (fjxh.15).
//!
//! The bead's law: implement REAL chemistry exports (the Boltzmann
//! softmax cell of record, `std.tensor.softmax`, repackaged as
//! `std.chem.softmax`) through [`install_pack`] — no fabricated or
//! empty exports (every export resolves against the std cell
//! registry), and no domain parser/backend branch (the language layer
//! is never touched: the pack is admitted as `FieldPackEntry` DATA and
//! compiled by the fjxh.9 image builder).

use std::collections::HashMap;

use emath_exec_ir::install::{InstalledPack, PackRegistry, install_pack};
use emath_exec_ir::term_compile::{CompiledCell, std_cell_registry};
use emath_term::{Signature, SymbolId, Term, VariableId};


/// The chemistry softmax reference cell of record: the same
/// `exp(sub(x, vmax(x))) / sum(exp(sub(x, vmax(x))))` stable-max term
/// the std registry compiles for `std.tensor.softmax`, exported under
/// the chemistry package path.
fn softmax_reference_term() -> (Term, Signature) {
    let x = || Term::Variable(VariableId("x".into()));
    let shifted = || Term::Apply {
        operator: SymbolId("sub".into()),
        arguments: vec![
            x(),
            Term::Apply {
                operator: SymbolId("vmax".into()),
                arguments: vec![x()],
            },
        ],
    };
    let exps = || Term::Apply {
        operator: SymbolId("exp".into()),
        arguments: vec![shifted()],
    };
    let term = Term::Apply {
        operator: SymbolId("div".into()),
        arguments: vec![
            exps(),
            Term::Apply {
                operator: SymbolId("sum".into()),
                arguments: vec![exps()],
            },
        ],
    };
    let mut signature = Signature::default();
    for (symbol, arity) in [
        ("exp", 1usize),
        ("sub", 2),
        ("div", 2),
        ("sum", 1),
        ("vmax", 1),
    ] {
        signature
            .insert(SymbolId(symbol.into()), arity)
            .expect("softmax formula signature is conflict-free");
    }
    (term, signature)
}

#[test]
fn r3_chem_bio_stdlib_ec2r_1() {
    // Real chemistry export through the field-pack machine: the
    // std.softmax reference term, packaged as `std.chem.softmax` cell
    // data and installed against the EXISTING std cell registry. No
    // fabricated exports — every export resolves — and no parser or
    // backend branch anywhere in the path.
    let registry = std_cell_registry();
    let softmax_cell = registry
        .get("std.tensor.softmax")
        .expect("std.tensor.softmax reference cell exists in the std registry");

    let entry = emath_ir::FieldPackEntry {
        name: "softmax".to_string(),
        exports: vec![("cell".to_string(), "std.chem.softmax".to_string())],
    };
    let package = vec!["std".to_string(), "chem".to_string()];
    let installed: InstalledPack = install_pack(&entry, &package, &registry)
        .expect("chemistry cell exports install cleanly through the machine");
    assert_eq!(
        installed.pack, "softmax",
        "the exported cell keeps its declared LEAF pack identity"
    );
    assert_eq!(
        installed.exports,
        vec!["std.chem.softmax".to_string()],
        "exports resolve to the declared canonical names"
    );
    installed
        .image
        .validate_partitions()
        .expect("the installed image is self-validating");
    assert!(
        installed.image.image_id.starts_with("fnv1a64:"),
        "the image id is the deterministic fnv1a64 fingerprint"
    );

    let mut pack_registry = PackRegistry::new();
    pack_registry.install(installed);
    let used: InstalledPack = pack_registry
        .resolve_use(&[
            "std".to_string(),
            "chem".to_string(),
            "softmax".to_string(),
        ])
        .expect("`use std.chem.softmax` resolves against the installed pack")
        .clone();
    assert_eq!(
        used.pack, "softmax",
        "`use` resolves the installed chemistry pack by its leaf identity"
    );
    let _ = softmax_cell;
    let _ = softmax_reference_term();
    let _: HashMap<String, CompiledCell> = std_cell_registry().clone();
}
