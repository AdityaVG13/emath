//! Finite synthesis of operator tables.
//!
//! Enumeration is deterministic over `carrier^(n²)` in lexicographic
//! input-pair order; every candidate table is validated against the
//! declared laws by the independent finite-law checker, so only tables
//! satisfying all declared laws are synthesized. An exhaustive search
//! that finds no table rejects the law set (seeded impossible set).

use emath_calibration::FittedTable;
use emath_law_check::{Law, WorldObligation, check_world};
use emath_term::SymbolId;
use emath_world_ir::{WorldId, fnv1a64};

use crate::graph::{HoleGraph, HoleState};

/// A declared finite law to synthesize against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SynthesisLaw {
    /// `op(x, y) == op(y, x)`.
    Commutative(SymbolId),
    /// `op(op(x, y), z) == op(x, op(y, z))`.
    Associative(SymbolId),
    /// `op(x, x) == x`.
    Idempotent(SymbolId),
    /// There is `e` with `op(x, e) == x == op(e, x)`.
    Identity(SymbolId, SymbolId),
}

impl SynthesisLaw {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Commutative(op) => format!("commutative:{}", op.0),
            Self::Associative(op) => format!("associative:{}", op.0),
            Self::Idempotent(op) => format!("idempotent:{}", op.0),
            Self::Identity(op, e) => format!("identity:{}:{}", op.0, e.0),
        }
    }

    /// The law-checker form.
    #[must_use]
    pub fn as_law(&self) -> Law {
        match self {
            Self::Commutative(op) => Law::Commutative(op.clone()),
            Self::Associative(op) => Law::Associative(op.clone()),
            Self::Idempotent(op) => Law::Idempotent(op.clone()),
            Self::Identity(op, e) => Law::Identity(op.clone(), e.clone()),
        }
    }
}

/// Why synthesis failed (all failures are typed refusals).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SynthesisError {
    /// The carrier must have at least one element.
    EmptyCarrier,
    /// Carrier exceeds [`MAX_CARRIER_SIZE`]; the table space would be
    /// unbounded (typed refusal `E-RES-110`).
    CarrierTooLarge(usize),
    /// An empty law set is refused: every table would vacuously
    /// "satisfy" it, so the outcome is a typed refusal (`E-RES-111`),
    /// never an invented `Contradictory` or a promised meaning.
    EmptyLaws,
}

/// A completed (or budget-cut) synthesis run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthesisRun {
    /// Synthesized tables, in deterministic enumeration order.
    pub tables: Vec<FittedTable>,
    /// Number of candidate tables examined.
    pub examined: u64,
    /// Whether the enumeration was exhaustive (all `carrier^(n²)`
    /// tables were examined).
    pub exhaustive: bool,
}

impl SynthesisRun {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "run:{}:{}:{}",
            self.examined,
            self.exhaustive,
            self.tables
                .iter()
                .map(FittedTable::canonical)
                .collect::<Vec<_>>()
                .join(";")
        )
    }
}

/// Deterministic receipt of one solver continuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolveReceipt {
    /// Hole being solved.
    pub hole_id: u64,
    /// Tables examined.
    pub examined: u64,
    /// Tables synthesized.
    pub found: usize,
    /// Whether the search was exhaustive.
    pub exhaustive: bool,
    /// Deterministic content identity.
    pub id: u64,
}

impl SolveReceipt {
    /// Deterministic canonical form (identity excluded).
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "solve:{}:examined={}:found={}:exhaustive={}",
            self.hole_id, self.examined, self.found, self.exhaustive
        )
    }
}

/// The continuation produced by solving a hole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuation {
    /// The new immutable problem state (original graph untouched).
    pub next_graph: HoleGraph,
    /// Synthesized tables.
    pub tables: Vec<FittedTable>,
    /// Deterministic receipt; failed proposals stay in the receipt and
    /// never mutate the authoritative graph.
    pub receipt: SolveReceipt,
}

/// Synthesizes all finite binary operator tables over `carrier` that
/// satisfy every declared law.
///
/// Enumeration is deterministic: table index `i` in
/// `0 .. carrier.len()^(carrier.len()²)` maps to the table whose cell
/// for input-pair `p` is `carrier[(i / n^p) % n]`, in lexicographic
/// input-pair order. Every candidate is validated by
/// `emath-law-check`; unspecified rows never occur because the
/// enumeration is total.
///
/// When the search is exhaustive and finds no table, the law set is
/// rejected: the run reports zero tables with `exhaustive == true`.
#[allow(clippy::cast_precision_loss)]
/// Maximum carrier size: enumeration space is `carrier^(carrier²)`, so an
/// oversized carrier is refused up front (E-RES-110) instead of spun up.
pub const MAX_CARRIER_SIZE: usize = 8;

/// Synthesizes all finite binary operator tables over `carrier` that
/// satisfy every declared law.
///
/// Enumeration is deterministic: table index `i` in
/// `0 .. carrier.len()^(carrier.len()²)` maps to the table whose cell
/// for input-pair `p` is `carrier[(i / n^p) % n]`, in lexicographic
/// input-pair order. Every candidate is validated by
/// `emath-law-check`; unspecified rows never occur because the
/// enumeration is total.
///
/// When the search is exhaustive and finds no table, the law set is
/// rejected: the run reports zero tables with `exhaustive == true`.
pub fn synthesize_tables(
    operator: &SymbolId,
    carrier: &[String],
    laws: &[SynthesisLaw],
    max_tables: u64,
) -> Result<SynthesisRun, SynthesisError> {
    if carrier.is_empty() {
        return Err(SynthesisError::EmptyCarrier);
    }
    if carrier.len() > MAX_CARRIER_SIZE {
        return Err(SynthesisError::CarrierTooLarge(carrier.len()));
    }
    if laws.is_empty() {
        return Err(SynthesisError::EmptyLaws);
    }
    let carrier_size = u32::try_from(carrier.len()).unwrap_or(u32::MAX);
    let n = carrier.len() as u64;
    // Binary operation table space: each of the n² input pairs picks one
    // of n values -> n^(n²), matching the documented formula (a previous
    // n^(2n) undershot the space and could claim exhaustive on a
    // partial search).
    let total = n.saturating_pow(carrier_size * carrier_size);
    let examined_limit = max_tables.min(total);

    let obligations = obligations_for(operator, laws);
    let mut tables = Vec::new();
    let mut examined = 0_u64;
    for index in 0..examined_limit {
        examined = index + 1;
        let cells = table_cells(carrier, index);
        let candidate = FittedTable::from_cells(operator.clone(), 2, cells);
        let report = check_world(WorldId(0), &candidate, &obligations);
        match report {
            Ok(report) if report.passed => tables.push(candidate),
            _ => {}
        }
    }
    let exhaustive = examined >= total;
    Ok(SynthesisRun {
        tables,
        examined,
        exhaustive,
    })
}

/// Deterministic obligation ids from laws.
fn obligations_for(operator: &SymbolId, laws: &[SynthesisLaw]) -> Vec<WorldObligation> {
    laws.iter()
        .map(|law| WorldObligation {
            id: fnv1a64(format!("{}:{}", operator.0, law.canonical()).as_bytes()),
            law: law.as_law(),
        })
        .collect()
}

/// Cells of the table for enumeration index `i`: input pairs in
/// lexicographic order get the mixed-radix digits of `i` (least
/// significant digit = first pair), so `i in 0 .. n^(n²)` decodes
/// without power overflow: `digit = index % n; index /= n` per cell.
#[allow(clippy::cast_possible_truncation)]
fn table_cells(carrier: &[String], index: u64) -> std::collections::BTreeMap<Vec<String>, String> {
    let n = carrier.len() as u64;
    let mut digits = index;
    let mut cells = std::collections::BTreeMap::new();
    for a in carrier {
        for b in carrier {
            let cell_index = digits % n;
            digits /= n;
            let cell = usize::try_from(cell_index).unwrap_or(usize::MAX);
            cells.insert(vec![a.clone(), b.clone()], carrier[cell].clone());
        }
    }
    cells
}

/// Solves one operator-definition hole by finite synthesis and returns a
/// continuation: the next immutable graph (hole marked `Solved` when
/// tables were found, `Contradictory` when the search was exhaustive
/// with no table, `BudgetExhausted` when the budget cut the search), the
/// synthesized tables, and the deterministic receipt. The input graph is
/// never mutated.
pub fn solve_op_hole(
    graph: &HoleGraph,
    hole_id: u64,
    operator: &SymbolId,
    carrier: &[String],
    laws: &[SynthesisLaw],
    max_tables: u64,
) -> Result<Continuation, SynthesisError> {
    let run = synthesize_tables(operator, carrier, laws, max_tables)?;
    let state = if !run.tables.is_empty() {
        HoleState::Solved
    } else if run.exhaustive {
        HoleState::Contradictory
    } else {
        HoleState::BudgetExhausted
    };
    let next_graph = graph.with_updated(
        hole_id,
        state,
        run.examined,
        format!(
            "synthesis examined {} tables, found {}; exhaustive={}",
            run.examined,
            run.tables.len(),
            run.exhaustive
        ),
    );
    let receipt = SolveReceipt {
        hole_id,
        examined: run.examined,
        found: run.tables.len(),
        exhaustive: run.exhaustive,
        id: 0,
    };
    let receipt = SolveReceipt {
        id: fnv1a64(receipt.canonical().as_bytes()),
        ..receipt
    };
    Ok(Continuation {
        next_graph,
        tables: run.tables,
        receipt,
    })
}

/// Convenience: builds a satisfiable law set (identity + commutativity +
/// idempotence) for the acceptance story.
#[must_use]
pub fn satisfiable_or_table_laws(operator: &SymbolId) -> Vec<SynthesisLaw> {
    vec![
        SynthesisLaw::Identity(operator.clone(), SymbolId("0".to_string())),
        SynthesisLaw::Commutative(operator.clone()),
        SynthesisLaw::Idempotent(operator.clone()),
    ]
}

/// Convenience: the seeded impossible law set (two distinct identities
/// for the same operator).
#[must_use]
pub fn impossible_identity_laws(operator: &SymbolId) -> Vec<SynthesisLaw> {
    vec![
        SynthesisLaw::Identity(operator.clone(), SymbolId("0".to_string())),
        SynthesisLaw::Identity(operator.clone(), SymbolId("1".to_string())),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let obligation = obligations_for(
            &SymbolId("op".to_string()),
            &[SynthesisLaw::Commutative(SymbolId("op".to_string()))],
        );
        for table in &run.tables {
            let report = check_world(WorldId(0), table, &obligation).expect("table must be total");
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
        assert_eq!(error, SynthesisError::EmptyLaws);
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
}
