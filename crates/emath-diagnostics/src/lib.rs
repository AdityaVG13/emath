//! Pedagogic diagnostics: explanations backed by checker witnesses.
//!
//! Schema: `emath.diagnostic.explanation v1`.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use emath_artifact::JsonWriter;
use emath_calibration::FittedTable;
use emath_law_check::{
    CheckerError, CheckerReceipt, FiniteLawChecker, LawVerdict, MinimizedCounterexample,
    WorldCheckReport, WorldObligation,
};
use emath_term::SymbolId;
use emath_world_ir::WorldId;

/// Stable code for a finite-checker commutative-law refutation.
pub const E_LAW_001: &str = "E-LAW-001";

/// Schema id for explanation JSON.
pub const EXPLANATION_SCHEMA: &str = "emath.diagnostic.explanation v1";

/// Schema id for tutor-check JSON.
pub const TUTOR_CHECK_SCHEMA: &str = "tutor-check/v1";

/// Kind of pedagogic explanation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExplainKind {
    WorldRejected,
    LawFalsified,
    GoalUnplanned {
        qstate_facets: Vec<String>,
    },
    BudgetCut {
        consumed: u64,
        limit: u64,
    },
    AuthorityCapped {
        requested: String,
        granted: String,
        reason: String,
    },
}

impl ExplainKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WorldRejected => "world_rejected",
            Self::LawFalsified => "law_falsified",
            Self::GoalUnplanned { .. } => "goal_unplanned",
            Self::BudgetCut { .. } => "budget_cut",
            Self::AuthorityCapped { .. } => "authority_capped",
        }
    }
}

/// How a witness is rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderFormat {
    CayleyMatrix,
    Expression,
    ExecutionTrace,
}

impl RenderFormat {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CayleyMatrix => "cayley_matrix",
            Self::Expression => "expression",
            Self::ExecutionTrace => "execution_trace",
        }
    }
}

/// Excerpt of a finite operator table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableExcerpt {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub highlighted: Vec<(usize, usize)>,
}

/// Rendered checker witness. Cells are copied from the table, never invented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedWitness {
    pub carrier_type: String,
    pub counterexample_tuple: Vec<String>,
    pub excerpt_table: Option<TableExcerpt>,
    pub render_format: RenderFormat,
}

/// Documentation pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocLink {
    pub title: String,
    pub href: String,
}

/// `emath.diagnostic.explanation v1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explanation {
    pub code: String,
    pub kind: ExplainKind,
    pub witness: Option<RenderedWitness>,
    pub structured_narrative: String,
    pub documentation_links: Vec<DocLink>,
    pub receipt_id: Option<u64>,
}

/// Why `tutor-check/v1` rejected an explanation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TutorCheckError {
    MissingWitness,
    MissingReceipt,
    EmptyNarrative,
    ClaimedGreenWithoutWitness,
}

impl TutorCheckError {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MissingWitness => "missing-witness",
            Self::MissingReceipt => "missing-receipt",
            Self::EmptyNarrative => "empty-narrative",
            Self::ClaimedGreenWithoutWitness => "claimed-green-without-witness",
        }
    }
}

/// Faithfulness gate: a law/world explanation must carry the checker receipt
/// and, on falsification, the table-backed witness. Synthesized "green" claims
/// without a witness are refused.
pub fn tutor_check_v1(explanation: &Explanation) -> Result<(), TutorCheckError> {
    if explanation.structured_narrative.trim().is_empty() {
        return Err(TutorCheckError::EmptyNarrative);
    }
    if explanation
        .structured_narrative
        .contains("epic claimed green")
        && explanation.witness.is_none()
    {
        return Err(TutorCheckError::ClaimedGreenWithoutWitness);
    }
    match &explanation.kind {
        ExplainKind::LawFalsified | ExplainKind::WorldRejected => {
            if explanation.witness.is_none() {
                return Err(TutorCheckError::MissingWitness);
            }
            if explanation.receipt_id.is_none() {
                return Err(TutorCheckError::MissingReceipt);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Build a Cayley excerpt from a binary table, highlighting the counterexample.
#[must_use]
pub fn cayley_excerpt(
    table: &FittedTable,
    counterexample: &MinimizedCounterexample,
) -> Option<RenderedWitness> {
    if table.arity != 2 {
        return Some(RenderedWitness {
            carrier_type: format!("finite-carrier arity-{}", table.arity),
            counterexample_tuple: counterexample.inputs.clone(),
            excerpt_table: None,
            render_format: RenderFormat::Expression,
        });
    }
    let mut carrier = Vec::new();
    for (inputs, output) in table.cells() {
        for value in inputs.iter().chain(std::iter::once(output)) {
            if !carrier.iter().any(|have: &String| have == value) {
                carrier.push(value.clone());
            }
        }
    }
    carrier.sort();
    let mut headers = vec![table.operator.0.clone()];
    headers.extend(carrier.iter().cloned());
    let mut rows = Vec::new();
    let mut highlighted = Vec::new();
    for (row_index, left) in carrier.iter().enumerate() {
        let mut row = vec![left.clone()];
        for (col_index, right) in carrier.iter().enumerate() {
            let cell = table
                .get(&[left.clone(), right.clone()])
                .unwrap_or("?")
                .to_string();
            if counterexample.inputs.len() >= 2
                && counterexample.inputs[0] == *left
                && counterexample.inputs[1] == *right
            {
                highlighted.push((row_index, col_index));
            }
            row.push(cell);
        }
        rows.push(row);
    }
    Some(RenderedWitness {
        carrier_type: "finite-carrier".to_string(),
        counterexample_tuple: counterexample.inputs.clone(),
        excerpt_table: Some(TableExcerpt {
            headers,
            rows,
            highlighted,
        }),
        render_format: RenderFormat::CayleyMatrix,
    })
}

/// Explain every failed verdict in a checker report.
#[must_use]
pub fn explain_law_report(table: &FittedTable, report: &WorldCheckReport) -> Vec<Explanation> {
    report
        .verdicts
        .iter()
        .filter_map(|verdict| explain_verdict(table, verdict, &report.receipt))
        .collect()
}

/// Canonical finite-checker entry: every failed verdict carries a `RenderedWitness`.
pub fn check_and_explain(
    candidate: WorldId,
    table: &FittedTable,
    obligations: &[WorldObligation],
) -> Result<(WorldCheckReport, Vec<Explanation>), CheckerError> {
    let report = FiniteLawChecker.check(candidate, table, obligations)?;
    let explanations = explain_law_report(table, &report);
    Ok((report, explanations))
}

/// True when every failed verdict has a tutor-check-faithful witness.
#[must_use]
pub fn every_failure_has_witness(report: &WorldCheckReport, explanations: &[Explanation]) -> bool {
    let failed = report
        .verdicts
        .iter()
        .filter(|verdict| !verdict.passed)
        .count();
    failed == explanations.len()
        && explanations
            .iter()
            .all(|explanation| explanation.witness.is_some() && tutor_check_v1(explanation).is_ok())
}

fn explain_verdict(
    table: &FittedTable,
    verdict: &LawVerdict,
    receipt: &CheckerReceipt,
) -> Option<Explanation> {
    let counterexample = verdict.counterexample.as_ref()?;
    let witness = cayley_excerpt(table, counterexample)?;
    Some(Explanation {
        code: E_LAW_001.to_string(),
        kind: ExplainKind::LawFalsified,
        structured_narrative: format!(
            "understood: the checker evaluated the claimed law on the finite carrier\nunknown: the law does not hold at {:?}\nwhy: {}\nsmallest repair: change the table or drop the law claim\nauthority: explanation does not raise Tested to Certified",
            counterexample.inputs, counterexample.detail
        ),
        documentation_links: vec![DocLink {
            title: "Diagnostics and tooling contract".to_string(),
            href: "language/reference/diagnostics-and-tooling-contract.md".to_string(),
        }],
        witness: Some(witness),
        receipt_id: Some(receipt.id),
    })
}

/// Canonical non-commutative table used by `emath explain E-LAW-001`.
#[must_use]
pub fn e_law_001_demo_table() -> FittedTable {
    let mut cells = BTreeMap::new();
    for (left, right, value) in [
        ("0", "0", "0"),
        ("0", "1", "0"),
        ("1", "0", "1"),
        ("1", "1", "0"),
    ] {
        cells.insert(vec![left.to_string(), right.to_string()], value.to_string());
    }
    FittedTable::from_cells(SymbolId("op".to_string()), 2, cells)
}

/// Run the finite checker on the demo table and explain E-LAW-001.
#[must_use]
pub fn e_law_001_demo() -> (WorldCheckReport, Vec<Explanation>) {
    use emath_law_check::Law;
    let table = e_law_001_demo_table();
    let obligation = WorldObligation {
        id: 1,
        law: Law::Commutative(SymbolId("op".to_string())),
    };
    check_and_explain(WorldId(1), &table, &[obligation]).expect("demo table is total")
}

/// ASCII Cayley matrix from a witness.
#[must_use]
pub fn render_cayley_ascii(witness: &RenderedWitness) -> String {
    let Some(table) = &witness.excerpt_table else {
        return witness.counterexample_tuple.join(", ");
    };
    let widths: Vec<usize> = table
        .headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            table
                .rows
                .iter()
                .map(|row| row.get(index).map(String::len).unwrap_or(0))
                .max()
                .unwrap_or(0)
                .max(header.len())
        })
        .collect();
    let fmt_row = |cells: &[String]| -> String {
        cells
            .iter()
            .enumerate()
            .map(|(index, cell)| format!("{cell:width$}", width = widths[index]))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut out = fmt_row(&table.headers);
    out.push('\n');
    for row in &table.rows {
        out.push_str(&fmt_row(row));
        out.push('\n');
    }
    out.push_str("counterexample: ");
    out.push_str(&witness.counterexample_tuple.join(","));
    out.push('\n');
    out
}

/// JSON object for one explanation (schema v1).
#[must_use]
pub fn explanation_json(explanation: &Explanation) -> String {
    let mut object = JsonWriter::object();
    object.string("schema", EXPLANATION_SCHEMA);
    object.string("code", &explanation.code);
    object.string("kind", explanation.kind.as_str());
    object.string("structured_narrative", &explanation.structured_narrative);
    if let Some(id) = explanation.receipt_id {
        object.int("receipt_id", id);
    }
    if let Some(witness) = &explanation.witness {
        let mut w = JsonWriter::object();
        w.string("carrier_type", &witness.carrier_type);
        w.strings("counterexample_tuple", &witness.counterexample_tuple);
        w.string("render_format", witness.render_format.as_str());
        if let Some(table) = &witness.excerpt_table {
            w.strings("headers", &table.headers);
            let rows: Vec<String> = table.rows.iter().map(|row| row.join(" ")).collect();
            w.strings("rows", &rows);
        }
        object.object_field("witness", &w.finish().trim_end());
    }
    let links: Vec<String> = explanation
        .documentation_links
        .iter()
        .map(|link| format!("{} {}", link.title, link.href))
        .collect();
    object.strings("documentation_links", &links);
    object.finish()
}
