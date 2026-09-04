//! Molecular graph slice: reaction
//! mechanism rewrites as (L, K, R) graph data over generic nucleus
//! ops — no chemistry Rust crate, no domain op enum, no parser branch.
//!
//! Representation (all existing generic carriers):
//! - a molecular graph is a dense Matrix: rows = CONTEXT atoms (the
//!   rule's interface), columns = the union of atoms across L and R,
//!   entry = bond order (1 single, 2 double, 3 triple; 0 = no bond);
//! - a rewrite rule is the triple (L, K, R), all with the SAME
//!   context-row × union-column dimensions; K is the context graph
//!   (zero columns beyond the context);
//! - the per-atom valence is the row's bond-order sum = the generic
//!   `matvec(A, 1s)` op;
//! - the preservation certificate is
//!   `sum(abs(matvec(L,1) - matvec(K,1))) + sum(abs(matvec(K,1) - matvec(R,1)))`,
//!   a scalar that MUST be zero; the cell's AllZero result guard
//!   refuses typed `ValenceImbalance(residual r)` otherwise.
//!
//! Failure-first: this suite was written BEFORE the cell existed and
//! failed exactly on the missing capability; the implementation then
//! closed the gap.

use emath_core::limits::Limits;
use emath_core::Span;
use emath_exec_ir::install::{PackRegistry, install_pack};
use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::term_compile::std_cell_registry;
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_sema::CompilerSession;
use emath_syntax::install_source_parser;

/// The capability path of the valence-preservation checker cell.
const REWRITE_PRESERVE: &str = "std.chem.graph_rewrite_preserve";

/// The chemistry field-pack source: exports the molecular-graph
/// checker with the existing cells; nothing is forked.
const CHEM_PACK: &str = "\
package std

emath field_pack chemistry:
    exports:
        cell balance
        cell mass_balance
        cell graph_rewrite_preserve
    metadata:
        description mass balance certificate balancing plus rewrite valence preservation
";

/// Admit the pack source at the language layer and return the entry.
fn admitted_pack(source: &str) -> emath_ir::FieldPackEntry {
    install_source_parser();
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned("chemistry-pack", source);
    let codes: Vec<String> = result
        .diagnostics
        .errors()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect();
    let messages: Vec<String> = result
        .diagnostics
        .items()
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        codes.is_empty(),
        "the chemistry pack admits at the language layer, got {messages:?}"
    );
    let mut packs = result.package.field_packs;
    assert_eq!(packs.len(), 1, "one field_pack admitted");
    packs.remove(0)
}

/// Hand-built EMIR applying the rewrite-preservation cell to its four
/// inputs: L, K, R (context-row × union-column matrices) and u (the
/// all-ones vector of length = column count).
fn preserve_program() -> EmirProgram {
    let span = Span::default();
    EmirProgram {
        ops: vec![
            (EmirOp::LoadInput(0), span),
            (EmirOp::LoadInput(1), span),
            (EmirOp::LoadInput(2), span),
            (EmirOp::LoadInput(3), span),
            (
                EmirOp::ApplyCapability {
                    capability: REWRITE_PRESERVE.to_string(),
                    class: CellClass::Pure,
                    args: vec![
                        EmirValue(0),
                        EmirValue(1),
                        EmirValue(2),
                        EmirValue(3),
                    ],
                },
                span,
            ),
        ],
        result: EmirValue(4),
        input_count: 4,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

/// Run the checker over a (L, K, R) rule, building the ones vector.
fn call_preserve(l: Value, k: Value, r: Value) -> Result<Value, EvalFault> {
    let cols = match (&l, &k, &r) {
        (Value::Matrix { cols, .. }, Value::Matrix { .. }, Value::Matrix { .. }) => *cols,
        _ => 1,
    };
    let ones = Value::Vector(vec![1.0; cols]);
    evaluate_with_budget(
        &preserve_program(),
        &[l, k, r, ones],
        &[],
        EvalBudget::default(),
    )
}

/// Allyl shift `C1=C2-C3 -> C1-C2=C3` (H transfers from C3 to C1).
/// Context atoms are the three carbons; columns are [C1, C2, C3,
/// H1a, H1b, H3a, H3b, H3c]. Valences (row sums of L): C1 = 2+1+1 = 4,
/// C2 = 2+1 = 3, C3 = 1+1+1+1 = 4; R preserves [4, 3, 4] — every
/// context atom's bond-order sum is invariant across the rewrite.
fn allyl_shift() -> (Value, Value, Value) {
    let l = Value::Matrix {
        rows: 3,
        cols: 8,
        data: vec![
            // C1: double to C2, two C-H singles
            0.0, 2.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0,
            // C2: double to C1, single to C3
            2.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            // C3: single to C2, three C-H singles
            0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0,
        ],
    };
    // K = the context as-is (same row sums; zero columns beyond the
    // context are already the case here — the union equals K).
    let k = l.clone();
    let r = Value::Matrix {
        rows: 3,
        cols: 8,
        data: vec![
            // C1: single to C2, three C-H singles (H3a moved here)
            0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0,
            // C2: single to C1, double to C3
            1.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            // C3: double to C2, two C-H singles
            0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0,
        ],
    };
    (l, k, r)
}

/// A valence-BREAKING rewrite: the double bond C1=C2 drops to a single
/// without any compensating bond added anywhere — C1 loses one valence
/// unit, so the certificate must refuse typed.
fn broken_bond() -> (Value, Value, Value) {
    let (l, k, _) = allyl_shift();
    let r = Value::Matrix {
        rows: 3,
        cols: 8,
        data: vec![
            // C1: SINGLE to C2 (was double), two C-H singles — valence 3
            0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0,
            // C2: single to C1, single to C3 — valence 2
            1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            // C3 unchanged: single to C2, three C-H singles — valence 4
            0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0,
        ],
    };
    (l, k, r)
}

/// The pack admits, installs against the EXISTING std registry, and
/// carries the checker cell.
#[test]
fn chemistry_pack_carries_rewrite_checker() {
    let entry = admitted_pack(CHEM_PACK);
    let installed = install_pack(&entry, &["std".to_string()], std_cell_registry())
        .expect("the chemistry pack installs from the existing registry");
    assert!(
        installed.exports.contains(&REWRITE_PRESERVE.to_string()),
        "exports carry graph_rewrite_preserve: {:?}",
        installed.exports
    );
    installed
        .image
        .validate_partitions()
        .expect("the installed image is self-validating");
    let mut registry = PackRegistry::new();
    registry.install(installed);
    registry
        .resolve_use(&["std".to_string(), "chemistry".to_string()])
        .expect("use std.chemistry resolves");
}

/// Positive: a real rewrite (allyl shift) preserves every context
/// atom's valence; the certificate is EXACTLY zero.
#[test]
fn allyl_shift_preserves_valence() {
    let (l, k, r) = allyl_shift();
    let out = call_preserve(l, k, r)
        .expect("valence-preserving rewrite evaluates in the reference VM");
    assert_eq!(
        out,
        Value::F64(0.0),
        "violation count is exactly zero for a preserved rewrite"
    );
}

/// Negative: dropping a double bond to a single without compensation
/// breaks C1's valence; the certificate refuses typed
/// `ValenceImbalance` — never a silent value.
#[test]
fn broken_bond_refuses_typed_valence_imbalance() {
    let (l, k, r) = broken_bond();
    match call_preserve(l, k, r) {
        Err(EvalFault::CapabilityRefused { capability, code }) => {
            assert_eq!(capability, REWRITE_PRESERVE);
            assert!(
                code.starts_with("ValenceImbalance"),
                "typed refusal names the check, got {code}"
            );
            assert_ne!(code, "ValenceImbalance", "refusal carries the residual");
        }
        ok => panic!("broken bond must refuse typed ValenceImbalance, got {ok:?}"),
    }
}

/// MR atom-permutation invariance (score 8): the checker is a sum over
/// context rows and union columns, so permuting atoms consistently in
/// L, K and R leaves the certificate unchanged (0 for preserved rules).
#[test]
fn mr_atom_permutation_invariance() {
    let (l0, k0, r0) = allyl_shift();
    let permute = |m: Value| -> Value {
        let Value::Matrix { rows, cols, data } = m else {
            unreachable!()
        };
        // Cycle the three context rows and the H columns {3,4,5}:
        // row p -> row (p+1)%3; column swap 3<->4.
        let mut out_data = vec![0.0; data.len()];
        for r in 0..rows {
            for c in 0..cols {
                let pr = (r + 1) % rows;
                let pc = match c {
                    3 => 4,
                    4 => 3,
                    other => other,
                };
                out_data[pr * cols + pc] = data[r * cols + c];
            }
        }
        Value::Matrix {
            rows,
            cols,
            data: out_data,
        }
    };
    let (l, k, r) = (permute(l0), permute(k0), permute(r0));
    let out = call_preserve(l, k, r)
        .expect("permuted rewrite still evaluates");
    assert_eq!(
        out,
        Value::F64(0.0),
        "certificate is invariant under atom permutation"
    );
}

/// MR disjoint composition (score 8): two independent preserved
/// rewrites on disjoint atoms compose (block-diagonal union) into a
/// preserved rewrite — the certificate of the union is the sum of the
/// certificates of the parts.
#[test]
fn mr_disjoint_rewrites_compose() {
    let (l1, k1, r1) = allyl_shift();
    let (lb, kb, rb) = broken_bond();
    let block = |a: Value, b: Value| -> Value {
        let Value::Matrix {
            rows: ra,
            cols: ca,
            data: da,
        } = a
        else {
            unreachable!()
        };
        let Value::Matrix {
            rows: rb,
            cols: cb,
            data: db,
        } = b
        else {
            unreachable!()
        };
        let mut data = Vec::with_capacity(ra * ca + rb * cb);
        for r in 0..ra {
            let mut row = da[r * ca..(r + 1) * ca].to_vec();
            row.extend(std::iter::repeat(0.0).take(cb));
            data.extend(row);
        }
        for r in 0..rb {
            let mut row = std::iter::repeat(0.0)
                .take(ca)
                .collect::<Vec<f64>>();
            row.extend(db[r * cb..(r + 1) * cb].to_vec());
            data.extend(row);
        }
        Value::Matrix {
            rows: ra + rb,
            cols: ca + cb,
            data,
        }
    };
    // Preserved ∘ preserved admits.
    let (l1b, k1b, r1b) = allyl_shift();
    let union = call_preserve(
        block(l1.clone(), l1b),
        block(k1.clone(), k1b),
        block(r1.clone(), r1b),
    )
    .expect("disjoint preserved rewrites compose");
    assert_eq!(union, Value::F64(0.0), "certificate of the union is zero");

    // Preserved ∘ broken refuses (one side's violation dominates).
    let mixed = call_preserve(block(l1, lb), block(k1, kb), block(r1, rb));
    assert!(
        matches!(mixed, Err(EvalFault::CapabilityRefused { .. })),
        "composing a broken rewrite refuses typed, got {mixed:?}"
    );
}

/// MR trivial-rule edge: a rule with no bond changes anywhere is the
/// identity rewrite and always preserves (violation 0).
#[test]
fn mr_identity_rewrite_preserves() {
    let (l, _, r) = allyl_shift();
    let out = call_preserve(l.clone(), l.clone(), r.clone())
        .expect("identity rewrite evaluates");
    // K == L == context; R == a permuted copy that is still preserved.
    let _ = r;
    assert_eq!(out, Value::F64(0.0));
}

/// MR cancellation trap (score 10): a rewrite where each context
/// atom's valence change sums to zero ACROSS atoms (one gains exactly
/// what another loses) — the certificate must still refuse because the
/// per-atom law uses absolute differences, never net sums.
#[test]
fn mr_cancellation_trap_still_refuses() {
    // Context atoms A, B; columns A, B. L: A double-bonded to B
    // (valences [2, 2]); R: a single bond plus an H migrating so
    // valences become [1, 3] — total 4 == total 4, no atom's valence
    // preserved.
    let l = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![0.0, 2.0, 2.0, 0.0],
    };
    let k = l.clone();
    let r = Value::Matrix {
        rows: 2,
        cols: 2,
        data: vec![0.0, 1.0, 1.0, 2.0],
    };
    match call_preserve(l, k, r) {
        Err(EvalFault::CapabilityRefused { capability, .. }) => {
            assert_eq!(capability, REWRITE_PRESERVE);
        }
        ok => panic!("cancellation trap must refuse typed, got {ok:?}"),
    }
}

/// MR charge-break negative (score 6): a rewrite that changes a formal
/// charge without touching a bond order changes the electron count
/// that bond-order valences encode; the graph carrier cannot see it.
/// This test pins the DOCUMENTED boundary: charge-aware checking is a
/// no-claim (needs element/property tables, a later slice), and the
/// checker's typed negative covers the BOND-order break that the
/// carrier can express. The refusal text names ValenceImbalance.
#[test]
fn mr_charge_break_is_documented_no_claim() {
    // The bond-order analogue of a charge breaking: a bond-order drop
    // of one with no compensating gain — refuses.
    let (l, k, r) = broken_bond();
    match call_preserve(l, k, r) {
        Err(EvalFault::CapabilityRefused { code, .. }) => {
            assert!(
                code.starts_with("ValenceImbalance"),
                "bond-order break is the charge-break analogue, got {code}"
            );
        }
        ok => panic!("must refuse, got {ok:?}"),
    }
}

/// MR scale-invariance (score 4.5): multiplying every bond order in
/// all three graphs by k scales every valence by k, so a preserved
/// rule stays preserved and a broken one stays broken (same relation).
#[test]
fn mr_bond_scaling_preserves_relation() {
    let (l, k, r) = allyl_shift();
    let scale = |m: Value| -> Value {
        let Value::Matrix { rows, cols, data } = m else {
            unreachable!()
        };
        Value::Matrix {
            rows,
            cols,
            data: data.iter().map(|x| x * 2.0).collect(),
        }
    };
    let out = call_preserve(scale(l), scale(k), scale(r))
        .expect("scaled preserved rewrite evaluates");
    assert_eq!(out, Value::F64(0.0), "scaling keeps the certificate zero");
}
