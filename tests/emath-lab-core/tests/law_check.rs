//! Finite law-checker refusal tests: empty and untotal tables never pass.

use emath_lab_core::calibration::FittedTable;
use emath_lab_core::law_check::{CheckerError, FiniteLawChecker, Law, WorldObligation};
use emath_term::SymbolId;
use emath_world_ir::{WorldId, fnv1a64};
use emath_test_harness::Probe;

fn commutative_obligation(operator: &str) -> WorldObligation {
    WorldObligation { id: fnv1a64(format!("test:{operator}").as_bytes()), law: Law::Commutative(SymbolId(operator.to_string())) }
}

#[test]
fn law_check_refusals() {
    let mut p = Probe::new("empty and untotal tables refuse, never vacuous-pass");
    p.case("empty-refused", |p| {
        let table = FittedTable::from_cells(SymbolId("op".to_string()), 2, std::collections::BTreeMap::new());
        p.eq("error", FiniteLawChecker.check(WorldId(0), &table, &[commutative_obligation("op")]).expect_err("empty table cannot pass"), CheckerError::EmptyTable);
    });
    p.case("untotal-refused", |p| {
        let mut cells = std::collections::BTreeMap::new();
        for (left, right, value) in [("a", "a", "a"), ("a", "b", "a"), ("b", "a", "a")] {
            cells.insert(vec![left.to_string(), right.to_string()], value.to_string());
        }
        let table = FittedTable::from_cells(SymbolId("op".to_string()), 2, cells);
        let error = FiniteLawChecker.check(WorldId(0), &table, &[commutative_obligation("op")]).expect_err("untotal table cannot pass");
        p.demand("untotal", matches!(error, CheckerError::Untotal { .. }), "missing row refuses as Untotal");
    });
    p.finish();
}
