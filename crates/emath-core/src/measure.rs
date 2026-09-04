//! core::measure — data and measurement types (Phase 12).
//!
//! Observational data is honest data:
//!
//! - A measurement is the triple **quantity + uncertainty + provenance**
//!   (extends the Phase 2 `Measured<T>` family, spec 21ul design at the
//!   data layer). The uncertainty is data, honestly propagated in
//!   quadrature for independent operands — never hidden in a float.
//! - A dataset carries column units (resolved through the core unit
//!   table) and provenance fields.
//! - The CSV adapter imports with provenance stamped on the dataset.
//! - Authority lattice: observational data enters at
//!   [`DataAuthority::Structural`] and degrades per the lattice
//!   ([`DataAuthority::degrade`], [`DataAuthority::min`]). Nothing in
//!   this package promotes authority: combining sources takes the MIN,
//!   so one assumed input contaminates the derived result downward.
//!
//! Error model: typed `String` errors carrying an `E-MEASURE-*` code,
//! the offending row/column, or the unknown unit spelling — never a
//! silent short row, a guessed cell, or an unvalidated unit.
//!
//! Determinism: identities are FNV-1a64 over the canonical encoding
//! (values as bit patterns, provenance and authority INCLUDED — the
//! 21ul law that provenance keys participate in identity).

/// Typed refusal: a data row has a different cell count than the header.
pub const E_MEASURE_RAGGED: &str = "E-MEASURE-1";
/// Typed refusal: a cell is not a finite f64 (refusal names row/column).
pub const E_MEASURE_CELL: &str = "E-MEASURE-2";
/// Typed refusal: no header (empty import).
pub const E_MEASURE_EMPTY: &str = "E-MEASURE-3";
/// Typed refusal: a column unit is not in the core unit table.
pub const E_MEASURE_UNIT: &str = "E-MEASURE-4";

use crate::fnv1a64_bytes;
use crate::units::seed_table;

/// Closed provenance taxonomy (21ul design, mirrored at the data layer).
#[derive(Clone, Debug, PartialEq)]
pub enum DataProvenance {
    /// SI definition or mathematical identity.
    Exact { basis: String },
    /// Publication or URI citation, with optional adjustment note.
    Citation {
        source: String,
        adjustment: Option<String>,
    },
    /// Instrument acquisition run: file identity + processing note.
    InstrumentRun { file: String, processing: String },
    /// Output of a fit — recursively honest about being derived.
    Fitted { fit_id: u64 },
    /// The honest "I made this up".
    Assumed,
    /// Default for bare literals; prints loudly in canonical encoding.
    Unstated,
}

impl DataProvenance {
    /// Canonical text for identity: the discriminant and every field are
    /// identity-bearing (provenance is a field, not metadata).
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Exact { basis } => format!("exact:{basis}"),
            Self::Citation { source, adjustment } => match adjustment {
                Some(note) => format!("citation:{source}:{note}"),
                None => format!("citation:{source}"),
            },
            Self::InstrumentRun { file, processing } => {
                format!("instrument:{file}:{processing}")
            }
            Self::Fitted { fit_id } => format!("fitted:{fit_id:016x}"),
            Self::Assumed => "assumed".to_string(),
            Self::Unstated => "unstated".to_string(),
        }
    }
}

/// Data authority lattice. Ordered `Unstated < Structural < Certified`:
/// observational data enters at `Structural`, degradation is explicit,
/// and combination takes the MIN — authority never silently promotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DataAuthority {
    /// No basis asserted.
    Unstated,
    /// Observational / structurally checked data (imports land here).
    Structural,
    /// Certified reference data (constructed explicitly by the caller;
    /// nothing in this package mints it from observational input).
    Certified,
}

impl DataAuthority {
    /// Stable token for identity encoding.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unstated => "unstated",
            Self::Structural => "structural",
            Self::Certified => "certified",
        }
    }

    /// Lattice meet: the weaker authority wins (contamination is
    /// one-way down).
    #[must_use]
    pub fn min(left: Self, right: Self) -> Self {
        if left <= right { left } else { right }
    }
}

/// Uncertainty distribution tag (recorded, not yet propagated).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistributionKind {
    Normal,
    Uniform,
    Lognormal,
}

/// The measurement triple: quantity + uncertainty + provenance, with
/// units resolved through the core unit table.
#[derive(Clone, Debug, PartialEq)]
pub struct Measurement {
    /// Central value in `unit` units.
    pub value: f64,
    /// k=1 standard uncertainty in `unit` units.
    pub std_uncertainty: f64,
    /// Distribution tag.
    pub distribution: DistributionKind,
    /// Unit spelling, resolvable via the core unit table.
    pub unit: String,
    /// Provenance (a field, not metadata).
    pub provenance: DataProvenance,
    /// Authority (degrades, never promotes).
    pub authority: DataAuthority,
}

impl Measurement {
    /// Sum of independent measurements: values convert to the left
    /// operand's unit through the core unit table, uncertainties add in
    /// quadrature, authority takes the MIN, provenance follows the
    /// weaker operand. Dimension mismatch or affine operands refuse
    /// typed (affine points are not addable quantities).
    pub fn add_independent(&self, other: &Self) -> Result<Self, String> {
        let table = seed_table();
        let left = table
            .resolve(&self.unit)
            .map_err(|error| format!("{E_MEASURE_UNIT}: {}", error.message))?;
        let right = table
            .resolve(&other.unit)
            .map_err(|error| format!("{E_MEASURE_UNIT}: {}", error.message))?;
        if left.dims != right.dims {
            return Err(format!(
                "{E_MEASURE_UNIT}: dimension mismatch adding `{}` to `{}`",
                self.unit, other.unit
            ));
        }
        if left.offset != 0.0 || right.offset != 0.0 {
            return Err(format!(
                "{E_MEASURE_UNIT}: affine unit arithmetic is refused (`{}` / `{}` has an offset)",
                self.unit, other.unit
            ));
        }
        // Both operands to SI, sum, then express in the left spelling.
        let si_value = self.value * left.scale + other.value * right.scale;
        let value = si_value / left.scale;
        let uncertainty = ((self.std_uncertainty * left.scale).powi(2)
            + (other.std_uncertainty * right.scale).powi(2))
        .sqrt()
            / left.scale;
        Ok(Self {
            value,
            std_uncertainty: uncertainty,
            distribution: self.distribution,
            unit: self.unit.clone(),
            provenance: if self.authority <= other.authority {
                self.provenance.clone()
            } else {
                other.provenance.clone()
            },
            authority: DataAuthority::min(self.authority, other.authority),
        })
    }

    /// Inverse-variance weighted mean of same-dimension measurements:
    /// `w = 1/σ²`, the combined uncertainty is `1/sqrt(Σw)`. All values
    /// convert to the first measurement's unit frame; authority takes
    /// the MIN over all operands and provenance follows the weakest.
    /// Affine operands refuse typed.
    pub fn weighted_mean(measurements: &[Self]) -> Result<Self, String> {
        let Some(first) = measurements.first() else {
            return Err(format!(
                "{E_MEASURE_EMPTY}: weighted mean of no measurements"
            ));
        };
        let table = seed_table();
        let first_spec = table
            .resolve(&first.unit)
            .map_err(|error| format!("{E_MEASURE_UNIT}: {}", error.message))?;
        if first_spec.offset != 0.0 {
            return Err(format!(
                "{E_MEASURE_UNIT}: affine unit arithmetic is refused (`{}` has an offset)",
                first.unit
            ));
        }
        let mut weight_sum = 0.0_f64;
        let mut value_sum = 0.0_f64;
        let mut authority = first.authority;
        let mut provenance = first.provenance.clone();
        for measurement in measurements {
            let spec = table
                .resolve(&measurement.unit)
                .map_err(|error| format!("{E_MEASURE_UNIT}: {}", error.message))?;
            if spec.dims != first_spec.dims {
                return Err(format!(
                    "{E_MEASURE_UNIT}: dimension mismatch weighting `{}` against `{}`",
                    measurement.unit, first.unit
                ));
            }
            if spec.offset != 0.0 {
                return Err(format!(
                    "{E_MEASURE_UNIT}: affine unit arithmetic is refused (`{}` has an offset)",
                    measurement.unit
                ));
            }
            // Express this measurement in the first unit's frame.
            let ratio = spec.scale / first_spec.scale;
            let sigma = measurement.std_uncertainty * ratio;
            let weight = 1.0 / (sigma * sigma);
            weight_sum += weight;
            value_sum += weight * (measurement.value * ratio);
            if measurement.authority < authority {
                authority = measurement.authority;
                provenance = measurement.provenance.clone();
            }
        }
        Ok(Self {
            value: value_sum / weight_sum,
            std_uncertainty: 1.0 / weight_sum.sqrt(),
            distribution: first.distribution,
            unit: first.unit.clone(),
            provenance,
            authority,
        })
    }
}

/// One dataset column: name, optional unit spelling, values.
#[derive(Clone, Debug, PartialEq)]
pub struct DataColumn {
    pub name: String,
    pub unit: Option<String>,
    pub values: Vec<f64>,
}

/// A dataset: named columns with units, dataset-level provenance and
/// authority. Provenance and authority are fields (not metadata) and
/// participate in identity.
#[derive(Clone, Debug, PartialEq)]
pub struct DataSet {
    pub name: String,
    pub columns: Vec<DataColumn>,
    pub provenance: DataProvenance,
    pub authority: DataAuthority,
}

impl DataSet {
    /// Construct with validation: every column must have the same row
    /// count (a ragged dataset is a typed refusal at the seam, not a
    /// silent short column).
    pub fn new(
        name: String,
        columns: Vec<DataColumn>,
        provenance: DataProvenance,
        authority: DataAuthority,
    ) -> Result<Self, String> {
        if columns.is_empty() {
            return Err(format!(
                "{E_MEASURE_EMPTY}: dataset `{name}` has no columns"
            ));
        }
        let rows = columns[0].values.len();
        for column in &columns {
            if column.values.len() != rows {
                return Err(format!(
                    "{E_MEASURE_RAGGED}: dataset `{name}` column `{}` has {} rows, expected {rows}",
                    column.name,
                    column.values.len()
                ));
            }
        }
        let table = seed_table();
        for column in &columns {
            if let Some(unit) = &column.unit
                && let Err(error) = table.resolve(unit)
            {
                return Err(format!("{E_MEASURE_UNIT}: {}", error.message));
            }
        }
        Ok(Self {
            name,
            columns,
            provenance,
            authority,
        })
    }

    /// Explicit degradation. There is no promote: authority only moves
    /// down the lattice, and the move is identity-bearing.
    #[must_use]
    pub fn degrade(mut self, to: DataAuthority) -> Self {
        if to < self.authority {
            self.authority = to;
        }
        self
    }

    /// FNV-1a64 identity over the canonical encoding: values as bit
    /// patterns, provenance and authority INCLUDED.
    #[must_use]
    pub fn identity(&self) -> u64 {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.name.as_bytes());
        for column in &self.columns {
            bytes.extend_from_slice(column.name.as_bytes());
            bytes.push(0);
            if let Some(unit) = &column.unit {
                bytes.extend_from_slice(unit.as_bytes());
            }
            bytes.push(0);
            for value in &column.values {
                bytes.extend_from_slice(&value.to_bits().to_le_bytes());
            }
        }
        bytes.extend_from_slice(self.provenance.canonical().as_bytes());
        bytes.extend_from_slice(self.authority.as_str().as_bytes());
        fnv1a64_bytes(&bytes)
    }
}

/// Parse `name (unit)` header cells: `(t (s), displacement (m))`.
/// Split one CSV header cell into its name and optional unit annotation
/// (`"time (s)"` -> `("time", Some("s"))`; `"label"` -> `("label", None)`).
pub fn parse_header_cell(cell: &str) -> (String, Option<String>) {
    let cell = cell.trim();
    if let Some(open) = cell.rfind('(')
        && cell.ends_with(')')
    {
        let name = cell[..open].trim().to_string();
        let unit = cell[open + 1..cell.len() - 1].trim().to_string();
        if !name.is_empty() && !unit.is_empty() {
            return (name, Some(unit));
        }
    }
    (cell.to_string(), None)
}

/// CSV adapter: import a dataset with provenance stamped. The header
/// row carries `name` or `name (unit)` spellings; every data cell must
/// parse as a finite f64; rows must match the header width. The result
/// authority is ALWAYS [`DataAuthority::Structural`] — observational
/// data is structural, whatever provenance the caller asserts.
pub fn parse_csv_dataset(
    text: &str,
    name: &str,
    provenance: DataProvenance,
) -> Result<DataSet, String> {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let Some(header_line) = lines.next() else {
        return Err(format!(
            "{E_MEASURE_EMPTY}: dataset `{name}` has no header row"
        ));
    };
    let header: Vec<&str> = header_line.split(',').collect();
    let mut columns: Vec<DataColumn> = header
        .iter()
        .map(|cell| {
            let (column_name, unit) = parse_header_cell(cell);
            DataColumn {
                name: column_name,
                unit,
                values: Vec::new(),
            }
        })
        .collect();
    let table = seed_table();
    for column in &columns {
        if let Some(unit) = &column.unit
            && let Err(error) = table.resolve(unit)
        {
            return Err(format!("{E_MEASURE_UNIT}: {}", error.message));
        }
    }
    for (row_index, line) in lines.enumerate() {
        let cells: Vec<&str> = line.split(',').collect();
        if cells.len() != columns.len() {
            return Err(format!(
                "{E_MEASURE_RAGGED}: dataset `{name}` row {} has {} cells, expected {}",
                row_index + 2,
                cells.len(),
                columns.len()
            ));
        }
        for (column_index, cell) in cells.iter().enumerate() {
            let parsed: Result<f64, _> = cell.trim().parse();
            let value = parsed.map_err(|_| {
                format!(
                    "{E_MEASURE_CELL}: dataset `{name}` row {} column {} cell `{}` is not a finite f64",
                    row_index + 2,
                    column_index,
                    cell.trim()
                )
            })?;
            if !value.is_finite() {
                return Err(format!(
                    "{E_MEASURE_CELL}: dataset `{name}` row {} column {} cell `{}` is not finite",
                    row_index + 2,
                    column_index,
                    cell.trim()
                ));
            }
            columns[column_index].values.push(value);
        }
    }
    Ok(DataSet {
        name: name.to_string(),
        columns,
        // Observational import: structural, degrades per the lattice.
        // The adapter has no path to Certified.
        provenance,
        authority: DataAuthority::Structural,
    })
}
