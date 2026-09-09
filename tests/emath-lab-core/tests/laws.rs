//! Deterministic property grids for the finite-world algebraic laws.

use std::collections::BTreeMap;

use emath_lab_core::calibration::FittedTable;
use emath_lab_core::law_check::{FiniteLawChecker, Law, WorldObligation};
use emath_term::SymbolId;
use emath_world_ir::{WorldId, fnv1a64};
use emath_test_harness::Probe;

const CARRIER: [&str; 3] = ["0", "1", "2"];

fn obligation(id_seed: &str, law: Law) -> WorldObligation {
    WorldObligation { id: fnv1a64(id_seed.as_bytes()), law }
}

fn total_table(operator: &str, combine: impl Fn(&str, &str) -> String) -> FittedTable {
    let mut cells = BTreeMap::new();
    for left in CARRIER {
        for right in CARRIER {
            cells.insert(vec![left.to_string(), right.to_string()], combine(left, right));
        }
    }
    FittedTable::from_cells(SymbolId(operator.to_string()), 2, cells)
}

fn max_cell(left: &str, right: &str) -> String {
    (if left >= right { left } else { right }).to_string()
}

#[test]
fn finite_world_laws() {
    let mut p = Probe::new("max is a commutative associative monoid; projections are not");
    p.case("commutative", |p| {
        let report = FiniteLawChecker.check(WorldId(1), &total_table("max", max_cell), &[obligation("max:commutative", Law::Commutative(SymbolId("max".into()))) ]).expect("max must be checkable");
        p.demand("max-commutes", report.passed, "max on CARRIER is commutative");
        let report = FiniteLawChecker.check(WorldId(2), &total_table("left", |l, _| l.to_string()), &[obligation("left:commutative", Law::Commutative(SymbolId("left".into()))) ]).expect("left projection must be checkable");
        p.demand("left-fails", !report.passed, "left projection is the negative control");
        let counter = report.verdicts[0].counterexample.as_ref().expect("failed commutativity minimizes a counterexample");
        p.eq("minimized", counter.inputs, vec!["0".to_string(), "1".to_string()]);
    });
    p.case("associative", |p| {
        let report = FiniteLawChecker.check(WorldId(3), &total_table("max", max_cell), &[obligation("max:associative", Law::Associative(SymbolId("max".into()))) ]).expect("max must be checkable");
        p.demand("max-assoc", report.passed, "max on CARRIER is associative");
    });
    p.case("identity", |p| {
        let table = total_table("max", max_cell);
        let report = FiniteLawChecker.check(WorldId(4), &table, &[obligation("max:identity", Law::Identity(SymbolId("max".into()), SymbolId("0".into()))) ]).expect("max must be checkable");
        p.demand("bottom", report.passed, "0 is the identity of max");
        let report = FiniteLawChecker.check(WorldId(5), &table, &[obligation("max:wrong-identity", Law::Identity(SymbolId("max".into()), SymbolId("1".into()))) ]).expect("max must be checkable");
        p.demand("wrong-fails", !report.passed, "1 is not the identity of max");
    });
    p.finish();
}
