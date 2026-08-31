//! Deterministic property grids for the finite-world algebraic laws.
//!
//! CONTRACT.md (emath-law-check): `Law::Commutative`, `Law::Associative`,
//! and `Law::Identity` hold over a total candidate table on the sorted
//! carrier, or the checker returns a minimized counterexample.

use std::collections::BTreeMap;

use emath_lab_core::calibration::FittedTable;
use emath_law_check::{FiniteLawChecker, Law, WorldObligation};
use emath_term::SymbolId;
use emath_world_ir::{WorldId, fnv1a64};

const CARRIER: [&str; 3] = ["0", "1", "2"];

fn obligation(id_seed: &str, law: Law) -> WorldObligation {
    WorldObligation {
        id: fnv1a64(id_seed.as_bytes()),
        law,
    }
}

fn total_table(operator: &str, combine: impl Fn(&str, &str) -> String) -> FittedTable {
    let mut cells = BTreeMap::new();
    for left in CARRIER {
        for right in CARRIER {
            cells.insert(
                vec![left.to_string(), right.to_string()],
                combine(left, right),
            );
        }
    }
    FittedTable::from_cells(SymbolId(operator.to_string()), 2, cells)
}

fn max_cell(left: &str, right: &str) -> String {
    if left >= right { left } else { right }.to_string()
}

fn left_proj(left: &str, _right: &str) -> String {
    left.to_string()
}

#[test]
fn finite_max_is_commutative_over_seeded_carrier() {
    // CONTRACT.md (emath-law-check): Law::Commutative — op(x,y) == op(y,x)
    // for all carrier pairs.
    let table = total_table("max", max_cell);
    let report = FiniteLawChecker
        .check(
            WorldId(1),
            &table,
            &[obligation(
                "max:commutative",
                Law::Commutative(SymbolId("max".into())),
            )],
        )
        .expect("a total max table must be checkable");
    assert!(report.passed, "max on {CARRIER:?} is commutative");

    let left = total_table("left", left_proj);
    let report = FiniteLawChecker
        .check(
            WorldId(2),
            &left,
            &[obligation(
                "left:commutative",
                Law::Commutative(SymbolId("left".into())),
            )],
        )
        .expect("a total left-projection table must be checkable");
    assert!(
        !report.passed,
        "left projection is the negative control: not commutative"
    );
    let counter = report.verdicts[0]
        .counterexample
        .as_ref()
        .expect("failed commutativity must minimize a counterexample");
    assert_eq!(counter.inputs, vec!["0".to_string(), "1".to_string()]);
}

#[test]
fn finite_max_is_associative_over_seeded_carrier() {
    // CONTRACT.md (emath-law-check): Law::Associative —
    // op(op(x,y),z) == op(x,op(y,z)) for all carrier triples.
    let table = total_table("max", max_cell);
    let report = FiniteLawChecker
        .check(
            WorldId(3),
            &table,
            &[obligation(
                "max:associative",
                Law::Associative(SymbolId("max".into())),
            )],
        )
        .expect("a total max table must be checkable");
    assert!(report.passed, "max on {CARRIER:?} is associative");
}

#[test]
fn finite_max_has_bottom_identity_over_seeded_carrier() {
    // CONTRACT.md (emath-law-check): Law::Identity — declared e satisfies
    // op(x,e) == x and op(e,x) == x for every carrier element.
    let table = total_table("max", max_cell);
    let report = FiniteLawChecker
        .check(
            WorldId(4),
            &table,
            &[obligation(
                "max:identity",
                Law::Identity(SymbolId("max".into()), SymbolId("0".into())),
            )],
        )
        .expect("a total max table must be checkable");
    assert!(report.passed, "0 is the identity of max on {CARRIER:?}");

    let report = FiniteLawChecker
        .check(
            WorldId(5),
            &table,
            &[obligation(
                "max:wrong-identity",
                Law::Identity(SymbolId("max".into()), SymbolId("1".into())),
            )],
        )
        .expect("a total max table must be checkable");
    assert!(
        !report.passed,
        "1 is not the identity of max on {CARRIER:?}"
    );
}
