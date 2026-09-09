//! Finite synthesis table-space and counterexample tests.

use emath_lab_core::calibration::FittedTable;
use emath_lab_core::holes::{HoleGraph, SynthesisLaw, check_laws, impossible_identity_laws, solve_op_hole, synthesize_tables};
use emath_term::SymbolId;
use emath_world_ir::WorldId;
use emath_test_harness::Probe;

fn op() -> SymbolId {
    SymbolId("op".to_string())
}

fn carrier(n: u64) -> Vec<String> {
    (0..n).map(|i| i.to_string()).collect()
}

fn total_cells(pairs: &[(&str, &str, &str)]) -> FittedTable {
    let mut cells = std::collections::BTreeMap::new();
    for (left, right, value) in pairs {
        cells.insert(vec![left.to_string(), right.to_string()], value.to_string());
    }
    FittedTable::from_cells(op(), 2, cells)
}

#[test]
fn finite_synthesis() {
    let mut p = Probe::new("synthesis exhausts honestly and counterexamples minimize");
    p.case("n3-exhaustive", |p| {
        let run = synthesize_tables(&op(), &["a".to_string(), "b".to_string(), "c".to_string()], &[SynthesisLaw::Commutative(op())], 3_u64.pow(9)).expect("commutative synthesis must run");
        p.eq("examined", run.examined, 3_u64.pow(9));
        p.demand("exhaustive", run.exhaustive, "full table space exhausted");
        p.eq("count", run.tables.len(), 729);
        let laws = [SynthesisLaw::Commutative(SymbolId("op".to_string()))];
        for table in &run.tables {
            let report = check_laws(WorldId(0), table, &laws).expect("table must be total");
            p.demand(format!("holds-{}", report.verdicts.len()), report.passed, "synthesized table satisfies the law");
        }
    });
    p.case("budget-honest", |p| {
        let run = synthesize_tables(&op(), &carrier(2), &[SynthesisLaw::Commutative(op())], 3).expect("budgeted synthesis must run");
        p.eq("examined", run.examined, 3);
        p.demand("not-exhaustive", !run.exhaustive, "partial search never claims exhaustive");
    });
    p.case("empty-laws-refused", |p| {
        let error = synthesize_tables(&op(), &carrier(2), &[], 100).expect_err("empty laws must be refused");
        p.eq("error", error, emath_lab_core::holes::SynthesisError::EmptyLaws);
        let graph = HoleGraph::new(Vec::new());
        p.demand("hole-refuses", solve_op_hole(&graph, 7, &SymbolId("op".to_string()), &carrier(2), &[], 100).is_err(), "empty-law solve refuses, never Contradictory");
    });
    p.case("impossible-identity", |p| {
        let run = synthesize_tables(&op(), &carrier(2), &impossible_identity_laws(&op()), 2_u64.pow(4)).expect("impossible-identity synthesis must run");
        p.eq("tables", run.tables.len(), 0);
        p.eq("examined", run.examined, 16);
        p.demand("exhaustive", run.exhaustive, "every table examined");
    });
    p.case("n8-honest", |p| {
        let run = synthesize_tables(&op(), &carrier(8), &[SynthesisLaw::Commutative(op())], 10_000).expect("budgeted synthesis must run");
        p.eq("examined", run.examined, 10_000);
        p.demand("not-exhaustive", !run.exhaustive, "8^64 space cannot exhaust in 10k");
    });
    p.case("counterexample-minimized", |p| {
        let table = total_cells(&[("0", "0", "0"), ("0", "1", "1"), ("1", "0", "0"), ("1", "1", "1")]);
        let report = check_laws(WorldId(0), &table, &[SynthesisLaw::Commutative(op())]).expect("total table must be checkable");
        p.demand("fails", !report.passed, "noncommutative table fails");
        let counterexample = report.verdicts[0].counterexample.as_ref().expect("failing law carries a counterexample");
        p.eq("inputs", counterexample.inputs.clone(), ["0", "1"]);
    });
    p.finish();
}
