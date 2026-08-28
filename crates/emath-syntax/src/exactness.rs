//! Exactness ledger: declared / inferred / constructed / open meaning.
//!
//! Built from inspectable desugar notes. Open holes stay open; freeze and
//! `--raise` must not silently claim exactness.

use crate::scratch::{expand_scratch, ScratchExpansion, ScratchNote};

/// One exactness dimension the ledger tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExactnessDimension {
    Syntactic,
    Type,
    Unit,
    Domain,
    Numeric,
    Method,
    Evidence,
    Execution,
    Deployment,
}

impl ExactnessDimension {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Syntactic => "syntactic",
            Self::Type => "type",
            Self::Unit => "unit",
            Self::Domain => "domain",
            Self::Numeric => "numeric",
            Self::Method => "method",
            Self::Evidence => "evidence",
            Self::Execution => "execution",
            Self::Deployment => "deployment",
        }
    }

    /// CLI `--raise` / source `# emath exactness raise` token. Only Unit is raiseable.
    #[must_use]
    pub fn from_raise_token(token: &str) -> Option<Self> {
        match token {
            "units" | "unit" => Some(Self::Unit),
            _ => None,
        }
    }
}

/// How a dimension was obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExactnessStatus {
    Declared,
    Inferred,
    Constructed,
    Open,
}

impl ExactnessStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Inferred => "inferred",
            Self::Constructed => "constructed",
            Self::Open => "open",
        }
    }
}

/// One ledger row. `inference_id` is `inference:N` (1-based) for `emath why`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactnessEntry {
    pub inference_id: String,
    pub dimension: ExactnessDimension,
    pub status: ExactnessStatus,
    pub name: String,
    pub rationale: String,
}

/// Deterministic meaning budget for a source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactnessLedger {
    pub entries: Vec<ExactnessEntry>,
}

impl ExactnessLedger {
    #[must_use]
    pub fn count(&self, status: ExactnessStatus) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.status == status)
            .count()
    }

    #[must_use]
    pub fn open_holes(&self) -> Vec<&ExactnessEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.status == ExactnessStatus::Open)
            .collect()
    }

    #[must_use]
    pub fn inferred(&self) -> Vec<&ExactnessEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.status == ExactnessStatus::Inferred)
            .collect()
    }
}

/// Build the ledger from official scratch expansion plus optional raised dimensions.
#[must_use]
pub fn exactness_ledger(source: &str) -> ExactnessLedger {
    let expansion = expand_scratch(source);
    ledger_from_expansion(source, &expansion, &raised_dimensions(source))
}

#[must_use]
pub fn exactness_ledger_raised(source: &str, raise: &[ExactnessDimension]) -> ExactnessLedger {
    let expansion = expand_scratch(source);
    let mut raised = raised_dimensions(source);
    for &item in raise {
        if !raised.contains(&item) {
            raised.push(item);
        }
    }
    ledger_from_expansion(source, &expansion, &raised)
}

fn raised_dimensions(source: &str) -> Vec<ExactnessDimension> {
    let mut raised = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# emath exactness raise ") {
            for part in rest.split_whitespace() {
                if let Some(dimension) = ExactnessDimension::from_raise_token(part) {
                    if !raised.contains(&dimension) {
                        raised.push(dimension);
                    }
                }
            }
        }
    }
    raised
}

fn ledger_from_expansion(
    source: &str,
    expansion: &ScratchExpansion,
    raised: &[ExactnessDimension],
) -> ExactnessLedger {
    let mut entries = Vec::new();
    push(
        &mut entries,
        ExactnessDimension::Syntactic,
        if expansion.rewritten() {
            ExactnessStatus::Inferred
        } else {
            ExactnessStatus::Declared
        },
        "surface",
        if expansion.rewritten() {
            "scratch/L2 desugars to a contracted declaration"
        } else {
            "source already uses a contracted declaration"
        },
    );
    let mut saw_type = false;
    let mut saw_domain = false;
    let mut saw_hole = false;
    for note in &expansion.notes {
        if note.inferred.starts_with("inputs.") {
            saw_type = true;
            push(
                &mut entries,
                ExactnessDimension::Type,
                note.stability,
                &note.inferred,
                &note.rationale,
            );
        }
        if note.inferred.contains("solve candidates") || note.inferred.contains("domain") {
            saw_domain = true;
            push(
                &mut entries,
                ExactnessDimension::Domain,
                note.stability,
                &note.inferred,
                &note.rationale,
            );
        }
        if note.inferred.contains("hole") || note.rationale.contains("open hole") {
            saw_hole = true;
            push(
                &mut entries,
                ExactnessDimension::Evidence,
                ExactnessStatus::Open,
                &note.inferred,
                &note.rationale,
            );
        }
    }
    if !expansion.holes.is_empty() {
        saw_hole = true;
        push(
            &mut entries,
            ExactnessDimension::Evidence,
            ExactnessStatus::Open,
            "hole",
            "typed hole remains open meaning; freeze must not claim it exact",
        );
    }
    if !saw_type {
        push(
            &mut entries,
            ExactnessDimension::Type,
            ExactnessStatus::Inferred,
            "Float64",
            "admission defaults untyped names to Float64 (N-TYPE-001); not claimed exact",
        );
    }
    let unit_status = if raised.contains(&ExactnessDimension::Unit) {
        ExactnessStatus::Declared
    } else if source.contains(" km") || source.contains(" m") || expansion.expanded.contains(" km")
    {
        ExactnessStatus::Inferred
    } else {
        ExactnessStatus::Open
    };
    push(
        &mut entries,
        ExactnessDimension::Unit,
        unit_status,
        "units",
        if unit_status == ExactnessStatus::Declared {
            "raised to declared without rewriting other inferences"
        } else if unit_status == ExactnessStatus::Inferred {
            "quantity literals present; still not a claimed exactness proof"
        } else {
            "units remain open until declared or raised"
        },
    );
    if !saw_domain {
        push(
            &mut entries,
            ExactnessDimension::Domain,
            ExactnessStatus::Open,
            "domain",
            "no `over` clause; candidates stay labeled, none silently chosen",
        );
    }
    push(
        &mut entries,
        ExactnessDimension::Numeric,
        ExactnessStatus::Inferred,
        "strict-f64",
        "numeric policy inferred from the Phase 1 host; freeze must not raise evidence",
    );
    push(
        &mut entries,
        ExactnessDimension::Method,
        ExactnessStatus::Open,
        "method",
        "methods are not required on ordinary files",
    );
    if !saw_hole {
        push(
            &mut entries,
            ExactnessDimension::Evidence,
            ExactnessStatus::Open,
            "evidence",
            "no evidence: section; open until an L3 evidence budget is written",
        );
    }
    push(
        &mut entries,
        ExactnessDimension::Execution,
        ExactnessStatus::Inferred,
        "evaluate",
        "definitions evaluate on the existing interpreter; not a proof",
    );
    push(
        &mut entries,
        ExactnessDimension::Deployment,
        ExactnessStatus::Open,
        "deployment",
        "no compile lock until freeze writes one",
    );
    number_inferences(&mut entries);
    ExactnessLedger { entries }
}

fn push(
    entries: &mut Vec<ExactnessEntry>,
    dimension: ExactnessDimension,
    status: ExactnessStatus,
    name: &str,
    rationale: &str,
) {
    entries.push(ExactnessEntry {
        inference_id: String::new(),
        dimension,
        status,
        name: name.to_string(),
        rationale: rationale.to_string(),
    });
}

fn number_inferences(entries: &mut [ExactnessEntry]) {
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.inference_id = format!("inference:{}", index + 1);
    }
}

/// Notes that `emath why` / `assumptions` can address, including ledger rows.
#[must_use]
pub fn explanation_notes(source: &str) -> Vec<ScratchNote> {
    let expansion = expand_scratch(source);
    let ledger = exactness_ledger(source);
    let mut notes = expansion.notes;
    for entry in ledger.entries {
        notes.push(ScratchNote {
            inferred: format!(
                "{} {} ({})",
                entry.inference_id,
                entry.dimension.as_str(),
                entry.status.as_str()
            ),
            rationale: entry.rationale,
            replacement: entry.name,
            stability: entry.status,
        });
    }
    notes
}

/// True when the source tries to claim exactness while holes remain open.
#[must_use]
pub fn claims_exactness_with_open_holes(source: &str) -> bool {
    let claims = source.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.contains("claim exact")
            || trimmed.contains("@claim_exact")
            || trimmed == "exact"
            || trimmed.starts_with("exact ")
    });
    let has_hole = source.contains(" = ?")
        || source.contains("=?")
        || source.lines().any(|line| line.trim() == "?");
    claims && has_hole
}
