//! Fitted finite-carrier operator tables (magnet relocation,
//! emath-mct-type-magnets-fnv-fitted-o7a6): the shared leaf type lives
//! beside the WorldIr vocabulary so `emath-holes`, `emath-law-check`,
//! and `emath-diagnostics` take ONE Tier-adjacent edge instead of the
//! whole calibration machinery for a single symbol. `emath-lab-core`'s
//! re-exports it for stable paths; the fitting procedures stay there.

use std::collections::BTreeMap;

use emath_term::SymbolId;

/// A fitted finite-carrier operator table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FittedTable {
    /// Operator the table defines.
    pub operator: SymbolId,
    /// Input arity.
    pub arity: usize,
    cells: BTreeMap<Vec<String>, String>,
}

impl FittedTable {
    /// Builds a table from explicit cells; every row must match `arity`.
    #[must_use]
    pub fn from_cells(
        operator: SymbolId,
        arity: usize,
        cells: BTreeMap<Vec<String>, String>,
    ) -> Self {
        assert!(
            cells.keys().all(|inputs| inputs.len() == arity),
            "all cells must match the declared arity"
        );
        Self {
            operator,
            arity,
            cells,
        }
    }

    /// Looks up an input row.
    #[must_use]
    pub fn get(&self, inputs: &[String]) -> Option<&str> {
        if inputs.len() != self.arity {
            return None;
        }
        self.cells.get(inputs).map(String::as_str)
    }

    /// Rows in deterministic (lexicographic) order.
    pub fn cells(&self) -> impl Iterator<Item = (&Vec<String>, &String)> + '_ {
        self.cells.iter()
    }

    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        let cells = self
            .cells
            .iter()
            .map(|(inputs, output)| format!("{}=>{}", inputs.join(","), output))
            .collect::<Vec<_>>()
            .join(";");
        format!("table:{}:arity={}:{}", self.operator.0, self.arity, cells)
    }
}
