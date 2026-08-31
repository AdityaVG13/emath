//! Finite synthesis tests (origin `crates/emath-holes/src/synth.rs`).

use emath_lab_core::calibration::FittedTable;
use emath_holes::{
    HoleGraph, SynthesisLaw, check_laws, impossible_identity_laws, solve_op_hole,
    synthesize_tables,
};
use emath_term::SymbolId;
use emath_world_ir::WorldId;

#[test]
fn n3_commutative_synthesis_is_exhaustive_over_the_full_table_space() {
    // The binary-table space over 3 elements is 3^(3²) = 19683
    // tables (a previous n^(2n) = 729 undershot it and could mark a
    // partial search exhaustive). With the full budget the search
    // must examine 19683 tables, report exhaustive, and find the
    // 3^6 = 729 commutative tables.
    let op = SymbolId("op".to_string());
    let carrier = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let run = synthesize_tables(
        &op,
        &carrier,
        &[SynthesisLaw::Commutative(op.clone())],
        3_u64.pow(9),
    )
    .expect("commutative synthesis over 3 elements must run");
    assert_eq!(run.examined, 3_u64.pow(9));
    assert!(run.exhaustive, "the full table space must be exhausted");
    assert_eq!(
        run.tables.len(),
        729,
        "commutative tables over 3 elements number 3^6"
    );
    // Every synthesized table must actually satisfy the law: the
    // checker backstops the count.
    let laws = [SynthesisLaw::Commutative(SymbolId("op".to_string()))];
    for table in &run.tables {
        let report = check_laws(WorldId(0), table, &laws).expect("table must be total");
        assert!(report.passed, "synthesized table must satisfy the laws");
    }
}

#[test]
fn budget_cut_reports_not_exhaustive() {
    // A budget below the table space must report exhaustive=false;
    // it may never claim a complete search or a Contradictory.
    let op = SymbolId("op".to_string());
    let carrier = vec!["a".to_string(), "b".to_string()];
    let run = synthesize_tables(
        &op,
        &carrier,
        &[SynthesisLaw::Commutative(op.clone())],
        3, // 4-table space, budget 3
    )
    .expect("budgeted synthesis must run");
    assert_eq!(run.examined, 3);
    assert!(!run.exhaustive);
}

#[test]
fn empty_laws_are_refused_not_contradictory() {
    // An empty law set would make every table vacuous; the honest
    // outcome is a typed refusal, not an invented Contradictory.
    let op = SymbolId("op".to_string());
    let carrier = vec!["a".to_string(), "b".to_string()];
    let error =
        synthesize_tables(&op, &carrier, &[], 100).expect_err("empty laws must be refused");
    assert_eq!(error, emath_holes::SynthesisError::EmptyLaws);
    // And through the hole solver: the hole must report an error,
    // never HoleState::Contradictory.
    let graph = HoleGraph::new(Vec::new());
    let result = solve_op_hole(&graph, 7, &SymbolId("op".to_string()), &carrier, &[], 100);
    assert!(
        result.is_err(),
        "empty-law solve must refuse instead of returning a Contradictory continuation"
    );
}

#[test]
fn impossible_identity_laws_are_rejected_exhaustively() {
    // Two distinct identities on the same operator cannot hold on any
    // table. Over n=2 the space is 2^(2²)=16; the run must examine
    // every table, report exhaustive=true, and return zero candidates.
    let op = SymbolId("op".to_string());
    let carrier = vec!["0".to_string(), "1".to_string()];
    let run = synthesize_tables(&op, &carrier, &impossible_identity_laws(&op), 2_u64.pow(4))
        .expect("impossible-identity synthesis must run");
    assert_eq!(run.tables.len(), 0, "two identities must yield no table");
    assert_eq!(run.examined, 16);
    assert!(
        run.exhaustive,
        "API reports exhaustive only when every table was examined"
    );
}

#[test]
fn carrier8_table_space_is_honestly_not_exhaustive() {
    // n=8 bounds the space at 8^64 (saturating to u64::MAX): a
    // budgeted search must report exhaustive=false, never a
    // truncated-space exhaustive=true from overflowed arithmetic.
    let op = SymbolId("op".to_string());
    let carrier = (0..8).map(|i| i.to_string()).collect::<Vec<_>>();
    let run = synthesize_tables(
        &op,
        &carrier,
        &[SynthesisLaw::Commutative(op.clone())],
        10_000,
    )
    .expect("budgeted synthesis over 8 elements must run");
    assert_eq!(run.examined, 10_000);
    assert!(
        !run.exhaustive,
        "8^64 space cannot be exhausted in 10k tables"
    );
}

#[test]
fn noncommutative_table_is_rejected_with_minimized_counterexample() {
    // A total 2-element table that is not commutative: the G6 exit
    // gate requires a minimized counterexample, not a silent drop.
    let op = SymbolId("op".to_string());
    let mut cells = std::collections::BTreeMap::new();
    for (left, right, value) in [
        ("0", "0", "0"),
        ("0", "1", "1"),
        ("1", "0", "0"),
        ("1", "1", "1"),
    ] {
        cells.insert(vec![left.to_string(), right.to_string()], value.to_string());
    }
    let table = FittedTable::from_cells(op.clone(), 2, cells);
    let report = check_laws(WorldId(0), &table, &[SynthesisLaw::Commutative(op)])
        .expect("a total table must be checkable");
    assert!(!report.passed, "wrong commutativity must fail");
    let counterexample = report.verdicts[0]
        .counterexample
        .as_ref()
        .expect("a failing law must carry a minimized counterexample");
    assert_eq!(counterexample.inputs, ["0", "1"]);
}
