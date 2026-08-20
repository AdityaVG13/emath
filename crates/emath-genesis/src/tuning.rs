//! SG-18 host joint tuning: budgeted search over meanings and host
//! evaluation strategies under a protected real-program objective.
//!
//! A [`TuningRequest`] names a carrier `{0, …, n−1}` and a
//! [`ProtectedObjective`] — host input sequences with required outputs.
//! The search enumerates the joint space (semantic table ×
//! [`ImplVariant`]) in semantic-major order and returns the lowest-cost
//! **qualified** candidate, or a typed refusal.
//!
//! Protection is absolute. A candidate that fails any protected example
//! is [`CandidateStatus::Disqualified`] and can never be selected,
//! regardless of cost. Cost is a secondary integer (op applications on
//! the protected inputs plus table complexity) used only among
//! qualified candidates. Ties break by lowest joint index.
//!
//! Rules (honest, no silent truncation):
//!
//! - Carrier elements are canonical indices `0..n`. Size `0` is refused.
//!   Size above [`MAX_CARRIER_SIZE`] is refused (`carrier-too-large`).
//! - An empty protected set is refused (`no-protected-objective`).
//!   Unprotected tuning is not this phase.
//! - Semantic tables are decoded by [`OpTable::from_index`] in the same
//!   mixed-radix order as [`crate::synth`]. Same index, same table.
//! - Joint index `j` decodes as table `j / 3`, variant `j % 3`
//!   (`FoldLeft`, `FoldRight`, `PairwiseTree`).
//! - [`TuningBudget::max_candidates`] is a hard window. Filling it with
//!   candidates still unexamined is [`TuningError::BudgetExceeded`],
//!   never a truncated winner. Exhausting the space with no qualified
//!   candidate is [`TuningError::NoQualifiedCandidate`].
//! - [`TuningRequest::joint_cursor`] is the next unexamined joint index.
//!   Because tuning selects a **global** cost minimum (not a first
//!   winner), a budget refusal carries the best qualified joint index
//!   found so far, and [`TuningRequest::incumbent`] seeds the next
//!   window with it. Splitting a search across windows and threading
//!   the incumbent is exactly equivalent to the unsplit search.
//!   Budget, cursor, and incumbent are execution parameters and are
//!   excluded from [`tuning_id`].
//!
//! Determinism class: pure integer enumeration, no floats. Receipts are
//! BTreeMap-ordered JSON, byte-identical across runs.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use emath_world_ir::fnv1a64;

use crate::synth::{OpTable, MAX_CARRIER_SIZE};

/// Joint-tuning schema id for artifacts and receipts.
pub const TUNING_SCHEMA: &str = "emath.joint-tuning";
/// Joint-tuning schema version. Bump on any change to the canonical
/// request encoding, the joint enumeration order, impl-variant
/// semantics, the cost rule, or the receipt layout; consumers refuse
/// versions they do not know.
pub const TUNING_VERSION: u32 = 1;
/// Number of host evaluation strategies in the implementation space.
pub const IMPL_VARIANT_COUNT: u64 = 3;

/// Refuses schema versions this module does not understand.
pub fn check_version(version: u32) -> Result<(), TuningError> {
    if version == TUNING_VERSION {
        Ok(())
    } else {
        Err(TuningError::UnknownVersion { version })
    }
}

/// Typed refusals for joint tuning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuningError {
    /// Schema version this module does not understand.
    UnknownVersion {
        /// Offending version.
        version: u32,
    },
    /// Request failed a well-formedness check.
    InvalidRequest {
        /// Stable reason token.
        reason: &'static str,
    },
    /// The search window filled without exhausting the joint space.
    /// `incumbent` is the best qualified joint index found so far
    /// (including any seed); thread it into the next window's
    /// [`TuningRequest::incumbent`] to keep the search globally exact.
    BudgetExceeded {
        /// Budget limit that was exhausted.
        limit: u64,
        /// Best qualified joint index so far, if any.
        incumbent: Option<u64>,
    },
    /// Every remaining candidate was examined; none satisfied the
    /// protected objective.
    NoQualifiedCandidate {
        /// Candidates actually examined in this window.
        examined: u64,
    },
}

/// How a host program evaluates a chain `op(op(...op(x1,x2)...), xk)`.
///
/// Each variant is a distinct evaluation strategy. They agree on
/// associative commutative tables and diverge otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplVariant {
    /// Left fold: `(((x1 ⊕ x2) ⊕ x3) ⊕ … ⊕ xk)`.
    FoldLeft,
    /// Right fold: `(x1 ⊕ (x2 ⊕ (… ⊕ xk)))`.
    FoldRight,
    /// Balanced pairwise reduction: adjacent pairs, leftover carried.
    PairwiseTree,
}

impl ImplVariant {
    /// Stable token used in DNA-adjacent identity and receipts.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::FoldLeft => "fold-left",
            Self::FoldRight => "fold-right",
            Self::PairwiseTree => "pairwise-tree",
        }
    }

    /// Decode the implementation coordinate of a joint index.
    #[must_use]
    pub fn from_joint_inner(inner: u64) -> Self {
        match inner % IMPL_VARIANT_COUNT {
            0 => Self::FoldLeft,
            1 => Self::FoldRight,
            _ => Self::PairwiseTree,
        }
    }

    /// Implementation-space index (`0..IMPL_VARIANT_COUNT`).
    #[must_use]
    pub fn index(self) -> u64 {
        match self {
            Self::FoldLeft => 0,
            Self::FoldRight => 1,
            Self::PairwiseTree => 2,
        }
    }

    /// Evaluate `inputs` under `table` with this strategy.
    ///
    /// Empty input is `None` (callers refuse empty examples before
    /// evaluation). A singleton is the element itself.
    #[must_use]
    pub fn evaluate(self, table: &OpTable, inputs: &[u8]) -> Option<u8> {
        if inputs.is_empty() {
            return None;
        }
        Some(match self {
            Self::FoldLeft => fold_left(table, inputs),
            Self::FoldRight => fold_right(table, inputs),
            Self::PairwiseTree => pairwise_tree(table, inputs),
        })
    }
}

/// One host example: `chain(inputs) == expected`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostExample {
    /// Input sequence (carrier indices). Must be non-empty.
    pub inputs: Vec<u8>,
    /// Required result (carrier index).
    pub expected: u8,
}

impl HostExample {
    fn canonical(&self) -> String {
        let mut out = String::from("ex(");
        for (index, input) in self.inputs.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(out, "{input}");
        }
        let _ = write!(out, "={})", self.expected);
        out
    }
}

/// Protected host objective: every example is an absolute constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedObjective {
    /// Host examples. Empty is refused (`no-protected-objective`).
    pub examples: Vec<HostExample>,
}

/// Search window: maximum joint candidates one [`tune`] call may
/// examine. Excluded from [`tuning_id`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuningBudget {
    /// Maximum candidates examined in this window.
    pub max_candidates: u64,
}

impl Default for TuningBudget {
    fn default() -> Self {
        Self {
            max_candidates: 256,
        }
    }
}

/// One joint-tuning request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuningRequest {
    /// Schema version; [`tune`] refuses unknown versions.
    pub version: u32,
    /// Carrier size `n`; elements are `0..n`.
    pub carrier_size: u8,
    /// Protected host examples. Protection is not a weighted penalty.
    pub objective: ProtectedObjective,
    /// Search window. Excluded from [`tuning_id`].
    pub budget: TuningBudget,
    /// Next unexamined joint index. Excluded from [`tuning_id`].
    pub joint_cursor: u64,
    /// Best qualified joint index from earlier windows, as reported by
    /// [`TuningError::BudgetExceeded`]. Must be `< joint_cursor` and
    /// must classify as qualified (re-verified, never trusted).
    /// Excluded from [`tuning_id`].
    pub incumbent: Option<u64>,
}

impl TuningRequest {
    /// Canonical request text (budget and cursor omitted).
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        let _ = write!(out, "tune({},[", self.carrier_size);
        for (index, example) in self.objective.examples.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&example.canonical());
        }
        out.push_str("])");
        out
    }
}

/// Qualification of one joint candidate against the protected objective.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateStatus {
    /// Passed every protected example. `cost` is the secondary integer.
    Qualified {
        /// Op applications on protected inputs plus table complexity.
        cost: u64,
    },
    /// Failed a protected example. Absolute; never selected.
    Disqualified {
        /// Index of the first failing example in request order.
        first_failed_example: usize,
    },
}

/// One ledger row: a disqualified candidate and the example that
/// rejected it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disqualification {
    /// [`candidate_id`] of the rejected joint candidate.
    pub candidate_id: u64,
    /// First failing example index.
    pub first_failed_example: usize,
}

/// Deterministic machine-readable tuning receipt (a qualified winner).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuningReceipt {
    /// Schema version echoed into the JSON.
    pub version: u32,
    /// FNV-1a64 of the versioned canonical request (budget/cursor out).
    pub tuning_id: u64,
    /// Semantic DNA of the winner (meaning only).
    pub dna: String,
    /// Winner implementation token.
    pub impl_token: String,
    /// Winner [`candidate_id`].
    pub winner_id: u64,
    /// Winner secondary cost.
    pub cost: u64,
    /// Candidates examined in this window (including the winner).
    pub examined: u64,
    /// Qualified candidates in this window.
    pub qualified: u64,
    /// Disqualified candidates in this window.
    pub disqualified: u64,
    /// Next unexamined joint index.
    pub resume_cursor: u64,
    /// Every disqualified candidate in examination order.
    pub ledger: Vec<Disqualification>,
}

impl TuningReceipt {
    /// BTreeMap-ordered JSON. Key order is lexicographic. Byte-identical
    /// across runs for the same receipt.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut root = BTreeMap::new();
        root.insert("cost", Json::Number(self.cost.to_string()));
        root.insert("disqualified", Json::Number(self.disqualified.to_string()));
        root.insert("dna", Json::Str(self.dna.clone()));
        root.insert("examined", Json::Number(self.examined.to_string()));
        root.insert("impl", Json::Str(self.impl_token.clone()));
        root.insert(
            "ledger",
            Json::Array(self.ledger.iter().map(ledger_json).collect()),
        );
        root.insert("qualified", Json::Number(self.qualified.to_string()));
        root.insert(
            "resume_cursor",
            Json::Number(self.resume_cursor.to_string()),
        );
        root.insert("schema", Json::Str(TUNING_SCHEMA.to_string()));
        root.insert("tuning_id", Json::Str(format!("{:016x}", self.tuning_id)));
        root.insert("version", Json::Number(self.version.to_string()));
        root.insert("winner_id", Json::Str(format!("{:016x}", self.winner_id)));
        emit_object(&root)
    }
}

/// Canonical compact encoding of meaning only: carrier size and cells.
/// Stable across implementation variants.
#[must_use]
pub fn semantic_dna(table: &OpTable) -> String {
    let mut out = String::new();
    let _ = write!(out, "{}:", table.carrier_size);
    for (index, cell) in table.cells.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "{cell}");
    }
    out
}

/// Joint candidate identity: FNV-1a64 over the versioned canonical
/// encoding of DNA plus the impl-variant token.
#[must_use]
pub fn candidate_id(table: &OpTable, variant: ImplVariant) -> u64 {
    fnv1a64(
        format!(
            "{TUNING_SCHEMA}.v{TUNING_VERSION}:{}|{}",
            semantic_dna(table),
            variant.token()
        )
        .as_bytes(),
    )
}

/// Request identity: FNV-1a64 over the versioned canonical request.
/// Budget and joint cursor are excluded.
#[must_use]
pub fn tuning_id(request: &TuningRequest) -> u64 {
    fnv1a64(format!("{TUNING_SCHEMA}.v{TUNING_VERSION}:{}", request.canonical()).as_bytes())
}

/// Decode a joint cursor into `(table_index, impl_variant)`.
#[must_use]
pub fn decode_joint(cursor: u64) -> (u64, ImplVariant) {
    (
        cursor / IMPL_VARIANT_COUNT,
        ImplVariant::from_joint_inner(cursor % IMPL_VARIANT_COUNT),
    )
}

/// Classify one (table, variant) pair against the protected objective.
///
/// Protection is absolute: the first failing example disqualifies.
/// Cost is computed only for qualified candidates.
#[must_use]
pub fn classify(
    table: &OpTable,
    variant: ImplVariant,
    objective: &ProtectedObjective,
) -> CandidateStatus {
    for (index, example) in objective.examples.iter().enumerate() {
        match variant.evaluate(table, &example.inputs) {
            Some(got) if got == example.expected => {}
            _ => {
                return CandidateStatus::Disqualified {
                    first_failed_example: index,
                };
            }
        }
    }
    CandidateStatus::Qualified {
        cost: candidate_cost(table, objective),
    }
}

/// Search the joint window for the lowest-cost qualified candidate.
pub fn tune(request: &TuningRequest) -> Result<TuningReceipt, TuningError> {
    check_version(request.version)?;
    validate(request)?;
    let n = request.carrier_size;
    let total = joint_space(n);
    let start = request.joint_cursor;
    if let Some(total) = total {
        if start > total {
            return Err(TuningError::InvalidRequest {
                reason: "cursor-out-of-range",
            });
        }
        if start == total {
            return Err(TuningError::NoQualifiedCandidate { examined: 0 });
        }
    }

    let limit = request.budget.max_candidates;
    let mut examined = 0_u64;
    let mut qualified = 0_u64;
    let mut disqualified = 0_u64;
    let mut ledger = Vec::new();
    // Seed the running best from an earlier window's incumbent. The
    // incumbent is re-verified, never trusted: it must precede this
    // window and must classify as qualified against the same objective.
    let mut best: Option<Winner> = match request.incumbent {
        None => None,
        Some(joint_index) => {
            if joint_index >= start {
                return Err(TuningError::InvalidRequest {
                    reason: "incumbent-out-of-window",
                });
            }
            let (table_index, variant) = decode_joint(joint_index);
            let table = OpTable::from_index(n, table_index);
            match classify(&table, variant, &request.objective) {
                CandidateStatus::Qualified { cost } => Some(Winner {
                    dna: semantic_dna(&table),
                    impl_token: variant.token().to_string(),
                    winner_id: candidate_id(&table, variant),
                    cost,
                    joint_index,
                }),
                CandidateStatus::Disqualified { .. } => {
                    return Err(TuningError::InvalidRequest {
                        reason: "incumbent-not-qualified",
                    });
                }
            }
        }
    };
    let mut index = start;
    while examined < limit {
        if let Some(total) = total {
            if index >= total {
                break;
            }
        }
        let (table_index, variant) = decode_joint(index);
        let table = OpTable::from_index(n, table_index);
        examined += 1;
        match classify(&table, variant, &request.objective) {
            CandidateStatus::Qualified { cost } => {
                qualified += 1;
                let take = match &best {
                    None => true,
                    Some(winner) => {
                        cost < winner.cost || (cost == winner.cost && index < winner.joint_index)
                    }
                };
                if take {
                    best = Some(Winner {
                        dna: semantic_dna(&table),
                        impl_token: variant.token().to_string(),
                        winner_id: candidate_id(&table, variant),
                        cost,
                        joint_index: index,
                    });
                }
            }
            CandidateStatus::Disqualified {
                first_failed_example,
            } => {
                disqualified += 1;
                ledger.push(Disqualification {
                    candidate_id: candidate_id(&table, variant),
                    first_failed_example,
                });
            }
        }
        index = index.saturating_add(1);
    }

    if let Some(total) = total {
        if index >= total {
            return finish(
                request,
                best,
                examined,
                qualified,
                disqualified,
                index,
                ledger,
            );
        }
    }
    if examined >= limit {
        return Err(TuningError::BudgetExceeded {
            limit,
            incumbent: best.as_ref().map(|winner| winner.joint_index),
        });
    }
    finish(
        request,
        best,
        examined,
        qualified,
        disqualified,
        index,
        ledger,
    )
}

struct Winner {
    dna: String,
    impl_token: String,
    winner_id: u64,
    cost: u64,
    joint_index: u64,
}

fn finish(
    request: &TuningRequest,
    best: Option<Winner>,
    examined: u64,
    qualified: u64,
    disqualified: u64,
    resume_cursor: u64,
    ledger: Vec<Disqualification>,
) -> Result<TuningReceipt, TuningError> {
    let Some(winner) = best else {
        return Err(TuningError::NoQualifiedCandidate { examined });
    };
    Ok(TuningReceipt {
        version: TUNING_VERSION,
        tuning_id: tuning_id(request),
        dna: winner.dna,
        impl_token: winner.impl_token,
        winner_id: winner.winner_id,
        cost: winner.cost,
        examined,
        qualified,
        disqualified,
        resume_cursor,
        ledger,
    })
}

fn validate(request: &TuningRequest) -> Result<(), TuningError> {
    if request.carrier_size == 0 {
        return Err(TuningError::InvalidRequest {
            reason: "empty-carrier",
        });
    }
    if request.carrier_size > MAX_CARRIER_SIZE {
        return Err(TuningError::InvalidRequest {
            reason: "carrier-too-large",
        });
    }
    if request.objective.examples.is_empty() {
        return Err(TuningError::InvalidRequest {
            reason: "no-protected-objective",
        });
    }
    let n = request.carrier_size;
    for example in &request.objective.examples {
        if example.inputs.is_empty() {
            return Err(TuningError::InvalidRequest {
                reason: "empty-input",
            });
        }
        if example.expected >= n || example.inputs.iter().any(|input| *input >= n) {
            return Err(TuningError::InvalidRequest {
                reason: "example-out-of-range",
            });
        }
    }
    Ok(())
}

fn joint_space(n: u8) -> Option<u64> {
    table_space(n).and_then(|tables| tables.checked_mul(IMPL_VARIANT_COUNT))
}

fn table_space(n: u8) -> Option<u64> {
    let cells = u32::from(n).saturating_mul(u32::from(n));
    u64::from(n).checked_pow(cells)
}

fn candidate_cost(table: &OpTable, objective: &ProtectedObjective) -> u64 {
    let applications = objective
        .examples
        .iter()
        .map(|example| example.inputs.len().saturating_sub(1) as u64)
        .sum::<u64>();
    applications.saturating_add(table_complexity(table))
}

fn table_complexity(table: &OpTable) -> u64 {
    let mut seen = [false; 256];
    let mut count = 0_u64;
    for &cell in &table.cells {
        let slot = usize::from(cell);
        if !seen[slot] {
            seen[slot] = true;
            count += 1;
        }
    }
    count
}

fn fold_left(table: &OpTable, inputs: &[u8]) -> u8 {
    let mut acc = inputs[0];
    for &input in &inputs[1..] {
        acc = table.apply(acc, input);
    }
    acc
}

fn fold_right(table: &OpTable, inputs: &[u8]) -> u8 {
    let mut acc = inputs[inputs.len() - 1];
    for &input in inputs.iter().rev().skip(1) {
        acc = table.apply(input, acc);
    }
    acc
}

fn pairwise_tree(table: &OpTable, inputs: &[u8]) -> u8 {
    let mut layer = inputs.to_vec();
    while layer.len() > 1 {
        let mut next = Vec::with_capacity(layer.len().div_ceil(2));
        let mut index = 0;
        while index + 1 < layer.len() {
            next.push(table.apply(layer[index], layer[index + 1]));
            index += 2;
        }
        if index < layer.len() {
            next.push(layer[index]);
        }
        layer = next;
    }
    layer[0]
}

fn ledger_json(entry: &Disqualification) -> Json {
    let mut object = BTreeMap::new();
    object.insert(
        "candidate_id",
        Json::Str(format!("{:016x}", entry.candidate_id)),
    );
    object.insert(
        "example",
        Json::Number(entry.first_failed_example.to_string()),
    );
    Json::Object(object)
}

enum Json {
    Str(String),
    Number(String),
    Array(Vec<Json>),
    Object(BTreeMap<&'static str, Json>),
}

fn emit_object(fields: &BTreeMap<&str, Json>) -> String {
    let mut out = String::from("{");
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\":", json_escape(key));
        emit_json(value, &mut out);
    }
    out.push('}');
    out
}

fn emit_json(value: &Json, out: &mut String) {
    match value {
        Json::Str(text) => {
            let _ = write!(out, "\"{}\"", json_escape(text));
        }
        Json::Number(text) => out.push_str(text),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                emit_json(item, out);
            }
            out.push(']');
        }
        Json::Object(fields) => {
            out.push_str(&emit_object(fields));
        }
    }
}

fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        candidate_id, check_version, classify, semantic_dna, tune, tuning_id, CandidateStatus,
        HostExample, ImplVariant, OpTable, ProtectedObjective, TuningBudget, TuningError,
        TuningRequest, TUNING_VERSION,
    };

    fn xor_table() -> OpTable {
        OpTable {
            carrier_size: 2,
            cells: vec![0, 1, 1, 0],
        }
    }

    fn xor_objective() -> ProtectedObjective {
        ProtectedObjective {
            examples: vec![
                HostExample {
                    inputs: vec![0, 0],
                    expected: 0,
                },
                HostExample {
                    inputs: vec![0, 1],
                    expected: 1,
                },
                HostExample {
                    inputs: vec![1, 0],
                    expected: 1,
                },
                HostExample {
                    inputs: vec![1, 1],
                    expected: 0,
                },
            ],
        }
    }

    fn xor_request() -> TuningRequest {
        TuningRequest {
            version: TUNING_VERSION,
            carrier_size: 2,
            objective: xor_objective(),
            budget: TuningBudget::default(),
            joint_cursor: 0,
            incumbent: None,
        }
    }

    #[test]
    fn happy_path_xor_fold_left_is_the_deterministic_winner() {
        assert_eq!(OpTable::from_index(2, 6).cells, xor_table().cells);
        let receipt = tune(&xor_request()).expect("winner");
        assert_eq!(receipt.dna, "2:0,1,1,0");
        assert_eq!(receipt.impl_token, "fold-left");
        assert_eq!(receipt.cost, 6);
        assert_eq!(receipt.version, TUNING_VERSION);
        assert_eq!(receipt.winner_id, candidate_id(&xor_table(), ImplVariant::FoldLeft));
        assert!(receipt.qualified >= 1);
        assert!(receipt.examined >= 19);
    }

    #[test]
    fn protection_beats_cost_seeded_negative_control() {
        let receipt = tune(&xor_request()).expect("winner");
        let cheap = OpTable::from_index(2, 0);
        let cheap_id = candidate_id(&cheap, ImplVariant::FoldLeft);
        let entry = receipt
            .ledger
            .iter()
            .find(|row| row.candidate_id == cheap_id)
            .expect("cheapest constant table must be in the ledger");
        assert_eq!(entry.first_failed_example, 1);
        let cheap_cost = 4 + 1;
        assert!(
            receipt.cost > cheap_cost,
            "winner cost {} must beat cheap disqualified cost {cheap_cost}",
            receipt.cost
        );
        assert_eq!(receipt.dna, "2:0,1,1,0");
        assert_eq!(receipt.impl_token, "fold-left");
    }

    #[test]
    fn impl_variants_are_real_on_a_non_associative_table() {
        let nand = OpTable {
            carrier_size: 2,
            cells: vec![1, 1, 1, 0],
        };
        let inputs = [0_u8, 0, 1];
        let left = ImplVariant::FoldLeft
            .evaluate(&nand, &inputs)
            .expect("non-empty");
        let right = ImplVariant::FoldRight
            .evaluate(&nand, &inputs)
            .expect("non-empty");
        assert_ne!(left, right);
        let objective = ProtectedObjective {
            examples: vec![HostExample {
                inputs: inputs.to_vec(),
                expected: left,
            }],
        };
        assert_eq!(
            classify(&nand, ImplVariant::FoldLeft, &objective),
            CandidateStatus::Qualified { cost: 4 }
        );
        assert_eq!(
            classify(&nand, ImplVariant::FoldRight, &objective),
            CandidateStatus::Disqualified {
                first_failed_example: 0
            }
        );
    }

    #[test]
    fn semantic_dna_is_meaning_only() {
        let table = xor_table();
        let dna = semantic_dna(&table);
        assert_eq!(dna, semantic_dna(&OpTable::from_index(2, 6)));
        let left = candidate_id(&table, ImplVariant::FoldLeft);
        let right = candidate_id(&table, ImplVariant::FoldRight);
        let tree = candidate_id(&table, ImplVariant::PairwiseTree);
        assert_eq!(semantic_dna(&table), "2:0,1,1,0");
        assert_ne!(left, right);
        assert_ne!(left, tree);
        assert_ne!(right, tree);
    }

    #[test]
    fn budget_then_resume_matches_unsplit_winner() {
        let unsplit = tune(&xor_request()).expect("unsplit");
        let first = TuningRequest {
            budget: TuningBudget { max_candidates: 8 },
            ..xor_request()
        };
        let incumbent = match tune(&first) {
            Err(TuningError::BudgetExceeded { limit: 8, incumbent }) => incumbent,
            other => panic!("window of 8 must refuse with incumbent, got {other:?}"),
        };
        // No qualified candidate exists in the first 8 joint indices of
        // the XOR objective, so the incumbent is empty here.
        assert_eq!(incumbent, None);
        let resumed = tune(&TuningRequest {
            budget: TuningBudget::default(),
            joint_cursor: 8,
            incumbent,
            ..xor_request()
        })
        .expect("resume");
        assert_eq!(resumed.dna, unsplit.dna);
        assert_eq!(resumed.impl_token, unsplit.impl_token);
        assert_eq!(resumed.cost, unsplit.cost);
        assert_eq!(resumed.winner_id, unsplit.winner_id);
        assert_eq!(resumed.tuning_id, unsplit.tuning_id);
    }

    #[test]
    fn incumbent_preserves_a_cheap_winner_found_before_the_split() {
        // Objective satisfied by the constant-0 table (joint index 0,
        // complexity 1, cheapest possible). Splitting right after table 0
        // must not lose it to a costlier later candidate.
        let request = TuningRequest {
            version: TUNING_VERSION,
            carrier_size: 2,
            objective: ProtectedObjective {
                examples: vec![HostExample {
                    inputs: vec![0, 0],
                    expected: 0,
                }],
            },
            budget: TuningBudget::default(),
            joint_cursor: 0,
            incumbent: None,
        };
        let unsplit = tune(&request).expect("unsplit");
        assert_eq!(unsplit.dna, "2:0,0,0,0");
        assert_eq!(unsplit.cost, 2);

        let window = TuningRequest {
            budget: TuningBudget { max_candidates: 3 },
            ..request.clone()
        };
        let incumbent = match tune(&window) {
            Err(TuningError::BudgetExceeded { limit: 3, incumbent }) => incumbent,
            other => panic!("window of 3 must refuse with incumbent, got {other:?}"),
        };
        assert_eq!(incumbent, Some(0), "constant-0 fold-left is the incumbent");

        let resumed = tune(&TuningRequest {
            joint_cursor: 3,
            incumbent,
            ..request.clone()
        })
        .expect("resume with incumbent");
        assert_eq!(resumed.dna, unsplit.dna);
        assert_eq!(resumed.impl_token, unsplit.impl_token);
        assert_eq!(resumed.cost, unsplit.cost);
        assert_eq!(resumed.winner_id, unsplit.winner_id);

        // Dropping the incumbent silently loses the pre-split winner:
        // the naive resume picks a strictly costlier later candidate.
        let naive = tune(&TuningRequest {
            joint_cursor: 3,
            incumbent: None,
            ..request.clone()
        })
        .expect("naive resume");
        assert!(
            naive.cost > unsplit.cost,
            "naive resume must miss the cheap pre-split winner ({} vs {})",
            naive.cost,
            unsplit.cost
        );

        // Adversarial incumbents are re-verified, never trusted.
        assert_eq!(
            tune(&TuningRequest {
                joint_cursor: 3,
                incumbent: Some(5),
                ..request.clone()
            }),
            Err(TuningError::InvalidRequest {
                reason: "incumbent-out-of-window"
            })
        );
        let disqualified_incumbent = TuningRequest {
            objective: ProtectedObjective {
                examples: vec![HostExample {
                    inputs: vec![0, 1],
                    expected: 1,
                }],
            },
            joint_cursor: 3,
            incumbent: Some(0),
            ..request
        };
        assert_eq!(
            tune(&disqualified_incumbent),
            Err(TuningError::InvalidRequest {
                reason: "incumbent-not-qualified"
            })
        );
    }

    #[test]
    fn malformed_requests_refuse() {
        let base = xor_request();
        assert_eq!(
            tune(&TuningRequest {
                carrier_size: 0,
                ..base.clone()
            }),
            Err(TuningError::InvalidRequest {
                reason: "empty-carrier"
            })
        );
        assert_eq!(
            tune(&TuningRequest {
                objective: ProtectedObjective {
                    examples: vec![HostExample {
                        inputs: vec![0, 2],
                        expected: 0,
                    }],
                },
                ..base.clone()
            }),
            Err(TuningError::InvalidRequest {
                reason: "example-out-of-range"
            })
        );
        assert_eq!(
            tune(&TuningRequest {
                objective: ProtectedObjective {
                    examples: Vec::new(),
                },
                ..base.clone()
            }),
            Err(TuningError::InvalidRequest {
                reason: "no-protected-objective"
            })
        );
        assert_eq!(check_version(TUNING_VERSION), Ok(()));
        assert_eq!(
            check_version(TUNING_VERSION + 1),
            Err(TuningError::UnknownVersion {
                version: TUNING_VERSION + 1
            })
        );
        assert_eq!(
            tune(&TuningRequest {
                version: TUNING_VERSION + 1,
                ..base
            }),
            Err(TuningError::UnknownVersion {
                version: TUNING_VERSION + 1
            })
        );
    }

    #[test]
    fn receipts_are_byte_identical_across_runs() {
        let request = xor_request();
        let first = tune(&request).expect("first").to_json();
        let second = tune(&request).expect("second").to_json();
        assert_eq!(first, second);
        assert!(first.starts_with('{'));
        assert!(first.contains("\"schema\":\"emath.joint-tuning\""));
        assert_eq!(tuning_id(&request), tuning_id(&request));
        let shifted_budget = TuningRequest {
            budget: TuningBudget { max_candidates: 64 },
            ..request.clone()
        };
        assert_eq!(tuning_id(&request), tuning_id(&shifted_budget));
        let shifted_cursor = TuningRequest {
            joint_cursor: 1,
            ..request.clone()
        };
        assert_eq!(tuning_id(&request), tuning_id(&shifted_cursor));
    }
}
