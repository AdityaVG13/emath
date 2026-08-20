//! SG-16 world morphisms: homomorphisms, observational quotients,
//! isomorphism search, portfolio invariant mining, and deduplication.
//!
//! Worlds here are finite binary operation tables ([`OpTable`] from
//! [`crate::synth`]). A [`WorldMorphism`] is a total map of carrier
//! indices. The homomorphism law is checked on every pair — no
//! probabilistic shortcuts.
//!
//! Observational equivalence on a table is the standard left-and-right
//! multiplication test: `x ≡ y` iff for every `z`, `op(x,z) == op(y,z)`
//! and `op(z,x) == op(z,y)`. Under that definition the class operation
//! is always well-defined: if `x ≡ x'` and `y ≡ y'` then
//! `op(x,y) == op(x',y')` as elements (because `x ≡ x'` gives
//! `op(x,y) == op(x',y)` and `y ≡ y'` gives `op(x',y) == op(x',y')`).
//! [`quotient`] still verifies this and refuses with a typed error if it
//! ever fails.
//!
//! Isomorphism search enumerates bijections in lexicographic order and
//! returns the first homomorphism. Carrier sizes above
//! [`MAX_ISO_SEARCH_SIZE`] refuse with [`MorphismError::BudgetExceeded`];
//! a hard [`MAX_ISO_CANDIDATES`] cap applies even inside that bound.
//!
//! Determinism class: pure integer tables, no floats. Receipts are
//! BTreeMap-ordered JSON, byte-identical across runs. Execution
//! parameters (the candidate budget) are excluded from [`morphism_id`].

use std::collections::BTreeMap;
use std::fmt::Write as _;

use emath_world_ir::fnv1a64;

use crate::synth::{check_table, OpTable, SynthLaw, MAX_CARRIER_SIZE};

/// World-morphism schema id for artifacts and receipts.
pub const MORPHISM_SCHEMA: &str = "emath.world-morphism";
/// World-morphism schema version. Bump on any change to the canonical
/// encoding, the homomorphism law, the quotient representative rule,
/// the law vocabulary, or a receipt layout; consumers refuse versions
/// they do not know.
pub const MORPHISM_VERSION: u32 = 1;
/// Isomorphism search refuses carriers larger than this (`n!` grows
/// faster than we want to enumerate without an explicit budget).
pub const MAX_ISO_SEARCH_SIZE: u8 = 5;
/// Hard cap on bijections examined during isomorphism search.
pub const MAX_ISO_CANDIDATES: u64 = 120;

/// Refuses schema versions this module does not understand.
pub fn check_version(version: u32) -> Result<(), MorphismError> {
    if version == MORPHISM_VERSION {
        Ok(())
    } else {
        Err(MorphismError::UnknownVersion { version })
    }
}

/// Typed refusals for world morphisms, quotients, and portfolio tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MorphismError {
    /// Schema version this module does not understand.
    UnknownVersion {
        /// Offending version.
        version: u32,
    },
    /// A morphism or table failed a well-formedness check.
    InvalidMorphism {
        /// Stable reason token.
        reason: &'static str,
    },
    /// Declared morphism sizes do not match the supplied tables.
    SizeMismatch,
    /// Search window filled (`MAX_ISO_SEARCH_SIZE` or
    /// `MAX_ISO_CANDIDATES`).
    BudgetExceeded {
        /// Budget limit that was exhausted.
        limit: u64,
    },
    /// Homomorphism law failed; `pair` is the lexicographically first
    /// counterexample `(a, b)`.
    NotAHomomorphism {
        /// First violating source pair.
        pair: [u8; 2],
    },
}

/// Concrete homomorphism-law counterexample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorphismViolation {
    /// Lexicographically first pair `(a, b)` with
    /// `map[op_s(a,b)] != op_t(map[a], map[b])`.
    pub pair: [u8; 2],
}

/// A total map from a source carrier to a target carrier.
///
/// `map[i]` is the image of source index `i`. Well-formed when
/// `map.len() == source_size` and every image is `< target_size`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldMorphism {
    /// Source carrier size.
    pub source_size: u8,
    /// Target carrier size.
    pub target_size: u8,
    /// Total map, indexed by source element.
    pub map: Vec<u8>,
}

impl WorldMorphism {
    /// Construct a morphism after well-formedness checks.
    pub fn new(
        source_size: u8,
        target_size: u8,
        map: Vec<u8>,
    ) -> Result<Self, MorphismError> {
        let morphism = Self {
            source_size,
            target_size,
            map,
        };
        validate_morphism(&morphism)?;
        Ok(morphism)
    }

    /// Identity map on a carrier of size `n`.
    pub fn identity(n: u8) -> Result<Self, MorphismError> {
        if n == 0 {
            return Err(MorphismError::InvalidMorphism {
                reason: "empty-carrier",
            });
        }
        if n > MAX_CARRIER_SIZE {
            return Err(MorphismError::InvalidMorphism {
                reason: "carrier-too-large",
            });
        }
        Ok(Self {
            source_size: n,
            target_size: n,
            map: (0..n).collect(),
        })
    }

    /// Canonical morphism text (no execution parameters).
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        let _ = write!(out, "morph({},{},[", self.source_size, self.target_size);
        for (index, image) in self.map.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(out, "{image}");
        }
        out.push_str("])");
        out
    }
}

/// FNV-1a64 of the versioned canonical morphism. Candidate budgets are
/// execution parameters and are not part of the encoding.
#[must_use]
pub fn morphism_id(morphism: &WorldMorphism) -> u64 {
    fnv1a64(format!("{MORPHISM_SCHEMA}.v{MORPHISM_VERSION}:{}", morphism.canonical()).as_bytes())
}

/// Observational quotient of a finite table: classes ordered by least
/// element, table over class indices, projection morphism onto the
/// quotient carrier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotientReceipt {
    /// Schema version echoed into the JSON.
    pub version: u32,
    /// [`morphism_id`] of the projection onto the quotient.
    pub morphism_id: u64,
    /// Equivalence classes, each sorted, classes ordered by least element.
    pub classes: Vec<Vec<u8>>,
    /// Quotient operation table (class indices).
    pub table: OpTable,
    /// Projection: source index → class index.
    pub projection: WorldMorphism,
}

impl QuotientReceipt {
    /// BTreeMap-ordered JSON. Byte-identical across runs.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut root = BTreeMap::new();
        root.insert(
            "classes",
            Json::Array(
                self.classes
                    .iter()
                    .map(|class| {
                        Json::Array(class.iter().map(|el| Json::Number(el.to_string())).collect())
                    })
                    .collect(),
            ),
        );
        root.insert(
            "morphism_id",
            Json::Str(format!("{:016x}", self.morphism_id)),
        );
        root.insert(
            "projection",
            Json::Array(
                self.projection
                    .map
                    .iter()
                    .map(|image| Json::Number(image.to_string()))
                    .collect(),
            ),
        );
        root.insert("schema", Json::Str(MORPHISM_SCHEMA.to_string()));
        root.insert(
            "table",
            Json::Array(
                self.table
                    .cells
                    .iter()
                    .map(|cell| Json::Number(cell.to_string()))
                    .collect(),
            ),
        );
        root.insert("version", Json::Number(self.version.to_string()));
        emit_object(&root)
    }
}

/// One law's per-world verdicts inside an [`InvariantReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawPortfolioVerdict {
    /// Canonical law token.
    pub law: String,
    /// Per-world hold/fail, in input-table order.
    pub holds: Vec<bool>,
    /// True iff the law holds on every world.
    pub shared: bool,
}

/// Portfolio invariant-mining report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantReport {
    /// Schema version echoed into the JSON.
    pub version: u32,
    /// Number of tables examined.
    pub world_count: usize,
    /// Verdicts in deterministic law-vocabulary order.
    pub laws: Vec<LawPortfolioVerdict>,
    /// Canonical tokens of laws that hold on every world.
    pub shared: Vec<String>,
}

impl InvariantReport {
    /// BTreeMap-ordered JSON. Byte-identical across runs.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut root = BTreeMap::new();
        root.insert(
            "laws",
            Json::Array(
                self.laws
                    .iter()
                    .map(|verdict| {
                        let mut object = BTreeMap::new();
                        object.insert(
                            "holds",
                            Json::Array(
                                verdict
                                    .holds
                                    .iter()
                                    .map(|hold| Json::Bool(*hold))
                                    .collect(),
                            ),
                        );
                        object.insert("law", Json::Str(verdict.law.clone()));
                        object.insert("shared", Json::Bool(verdict.shared));
                        Json::Object(object)
                    })
                    .collect(),
            ),
        );
        root.insert("schema", Json::Str(MORPHISM_SCHEMA.to_string()));
        root.insert(
            "shared",
            Json::Array(self.shared.iter().map(|law| Json::Str(law.clone())).collect()),
        );
        root.insert("version", Json::Number(self.version.to_string()));
        root.insert("world_count", Json::Number(self.world_count.to_string()));
        emit_object(&root)
    }
}

/// A dropped isomorphic copy and the witness map onto the representative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedDuplicate {
    /// Duplicate table (not the group representative).
    pub table: OpTable,
    /// Isomorphism from this table onto the representative.
    pub witness: WorldMorphism,
}

/// One isomorphism class kept by [`dedupe`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupeGroup {
    /// Least table in the class (cells lexicographic).
    pub representative: OpTable,
    /// Other members, each with a witness onto the representative.
    pub dropped: Vec<DroppedDuplicate>,
}

/// Deterministic isomorphism-class grouping of a portfolio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupeReceipt {
    /// Schema version echoed into the JSON.
    pub version: u32,
    /// FNV-1a64 of the versioned canonical grouping.
    pub receipt_id: u64,
    /// Groups ordered by representative cells lexicographic.
    pub groups: Vec<DedupeGroup>,
}

impl DedupeReceipt {
    /// BTreeMap-ordered JSON. Byte-identical across runs.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut root = BTreeMap::new();
        root.insert(
            "groups",
            Json::Array(self.groups.iter().map(dedupe_group_json).collect()),
        );
        root.insert("receipt_id", Json::Str(format!("{:016x}", self.receipt_id)));
        root.insert("schema", Json::Str(MORPHISM_SCHEMA.to_string()));
        root.insert("version", Json::Number(self.version.to_string()));
        emit_object(&root)
    }
}

/// Checks `map[op_s(a,b)] == op_t(map[a], map[b])` for every pair.
/// The first lexicographic failure is [`MorphismError::NotAHomomorphism`].
pub fn verify(
    morphism: &WorldMorphism,
    source: &OpTable,
    target: &OpTable,
) -> Result<(), MorphismError> {
    validate_morphism(morphism)?;
    validate_table(source)?;
    validate_table(target)?;
    if morphism.source_size != source.carrier_size || morphism.target_size != target.carrier_size {
        return Err(MorphismError::SizeMismatch);
    }
    if let Some(violation) = first_homo_violation(morphism, source, target) {
        return Err(MorphismError::NotAHomomorphism {
            pair: violation.pair,
        });
    }
    Ok(())
}

/// First bijection in lexicographic order that is a homomorphism, or
/// `Ok(None)` when the tables are not isomorphic. Different carrier
/// sizes are not isomorphic. Sizes above [`MAX_ISO_SEARCH_SIZE`] refuse.
pub fn find_isomorphism(
    left: &OpTable,
    right: &OpTable,
) -> Result<Option<WorldMorphism>, MorphismError> {
    validate_table(left)?;
    validate_table(right)?;
    if left.carrier_size != right.carrier_size {
        return Ok(None);
    }
    let n = left.carrier_size;
    if n > MAX_ISO_SEARCH_SIZE {
        return Err(MorphismError::BudgetExceeded {
            limit: u64::from(MAX_ISO_SEARCH_SIZE),
        });
    }
    let mut candidate: Vec<u8> = (0..n).collect();
    let mut examined = 0_u64;
    loop {
        if examined >= MAX_ISO_CANDIDATES {
            return Err(MorphismError::BudgetExceeded {
                limit: MAX_ISO_CANDIDATES,
            });
        }
        examined += 1;
        let morphism = WorldMorphism {
            source_size: n,
            target_size: n,
            map: candidate.clone(),
        };
        if first_homo_violation(&morphism, left, right).is_none() {
            return Ok(Some(morphism));
        }
        if !next_permutation(&mut candidate) {
            return Ok(None);
        }
    }
}

/// Observational quotient. Class representatives are least elements;
/// class order is by least element.
pub fn quotient(table: &OpTable) -> Result<QuotientReceipt, MorphismError> {
    validate_table(table)?;
    let n = table.carrier_size;
    let reps: Vec<u8> = (0..n).map(|element| class_rep(table, element)).collect();
    let mut unique = reps.clone();
    unique.sort_unstable();
    unique.dedup();

    for x in 0..n {
        for x_prime in 0..n {
            if !observational_eq(table, x, x_prime) {
                continue;
            }
            for y in 0..n {
                for y_prime in 0..n {
                    if !observational_eq(table, y, y_prime) {
                        continue;
                    }
                    let left = class_rep(table, table.apply(x, y));
                    let right = class_rep(table, table.apply(x_prime, y_prime));
                    if left != right {
                        return Err(MorphismError::InvalidMorphism {
                            reason: "quotient-not-well-defined",
                        });
                    }
                }
            }
        }
    }

    let class_count = u8::try_from(unique.len()).unwrap_or(0);
    let mut cells = Vec::with_capacity(unique.len().saturating_mul(unique.len()));
    for &left_rep in &unique {
        for &right_rep in &unique {
            let product = table.apply(left_rep, right_rep);
            let class = class_index(&unique, class_rep(table, product));
            cells.push(class);
        }
    }
    let quotient_table = OpTable {
        carrier_size: class_count,
        cells,
    };
    let projection = WorldMorphism {
        source_size: n,
        target_size: class_count,
        map: reps.iter().map(|rep| class_index(&unique, *rep)).collect(),
    };
    let classes = unique
        .iter()
        .map(|rep| {
            (0..n)
                .filter(|element| class_rep(table, *element) == *rep)
                .collect::<Vec<_>>()
        })
        .collect();
    Ok(QuotientReceipt {
        version: MORPHISM_VERSION,
        morphism_id: morphism_id(&projection),
        classes,
        table: quotient_table,
        projection,
    })
}

/// Check the synth law vocabulary on every table. Shared invariants are
/// the laws that hold on all of them. Law order is fixed: commutative,
/// associative, existential left/right/two-sided identity.
pub fn mine_invariants(tables: &[OpTable]) -> Result<InvariantReport, MorphismError> {
    for table in tables {
        validate_table(table)?;
    }
    let vocabulary = law_vocabulary();
    let mut laws = Vec::with_capacity(vocabulary.len());
    let mut shared = Vec::new();
    for law in &vocabulary {
        let holds: Vec<bool> = tables
            .iter()
            .map(|table| check_table(table, std::slice::from_ref(law)).is_ok())
            .collect();
        let is_shared = holds.iter().all(|hold| *hold);
        let token = law.canonical();
        if is_shared {
            shared.push(token.clone());
        }
        laws.push(LawPortfolioVerdict {
            law: token,
            holds,
            shared: is_shared,
        });
    }
    Ok(InvariantReport {
        version: MORPHISM_VERSION,
        world_count: tables.len(),
        laws,
        shared,
    })
}

/// Group tables by isomorphism. The representative of each class is the
/// least table by cells lexicographic; dropped members carry a witness
/// morphism onto that representative.
pub fn dedupe(tables: &[OpTable]) -> Result<DedupeReceipt, MorphismError> {
    for table in tables {
        validate_table(table)?;
    }
    let mut assigned = vec![false; tables.len()];
    let mut groups = Vec::new();
    for index in 0..tables.len() {
        if assigned[index] {
            continue;
        }
        let mut members = vec![index];
        assigned[index] = true;
        for other in (index + 1)..tables.len() {
            if assigned[other] {
                continue;
            }
            if find_isomorphism(&tables[index], &tables[other])?.is_some() {
                assigned[other] = true;
                members.push(other);
            }
        }
        members.sort_by(|&left, &right| tables[left].cells.cmp(&tables[right].cells));
        let representative_index = members[0];
        let mut dropped = Vec::new();
        for &member in members.iter().skip(1) {
            let witness = find_isomorphism(&tables[member], &tables[representative_index])?
                .ok_or_else(|| MorphismError::InvalidMorphism {
                    reason: "iso-witness-missing",
                })?;
            dropped.push(DroppedDuplicate {
                table: tables[member].clone(),
                witness,
            });
        }
        dropped.sort_by(|left, right| left.table.cells.cmp(&right.table.cells));
        groups.push(DedupeGroup {
            representative: tables[representative_index].clone(),
            dropped,
        });
    }
    groups.sort_by(|left, right| left.representative.cells.cmp(&right.representative.cells));
    let receipt = DedupeReceipt {
        version: MORPHISM_VERSION,
        receipt_id: 0,
        groups,
    };
    let receipt_id = fnv1a64(
        format!(
            "{MORPHISM_SCHEMA}.v{MORPHISM_VERSION}:{}",
            dedupe_canonical(&receipt)
        )
        .as_bytes(),
    );
    Ok(DedupeReceipt {
        receipt_id,
        ..receipt
    })
}

fn law_vocabulary() -> [SynthLaw; 5] {
    [
        SynthLaw::Commutative,
        SynthLaw::Associative,
        SynthLaw::LeftIdentity { element: None },
        SynthLaw::RightIdentity { element: None },
        SynthLaw::Identity { element: None },
    ]
}

fn validate_morphism(morphism: &WorldMorphism) -> Result<(), MorphismError> {
    if morphism.source_size == 0 || morphism.target_size == 0 {
        return Err(MorphismError::InvalidMorphism {
            reason: "empty-carrier",
        });
    }
    if morphism.source_size > MAX_CARRIER_SIZE || morphism.target_size > MAX_CARRIER_SIZE {
        return Err(MorphismError::InvalidMorphism {
            reason: "carrier-too-large",
        });
    }
    if morphism.map.len() != usize::from(morphism.source_size) {
        return Err(MorphismError::InvalidMorphism {
            reason: "map-length",
        });
    }
    if morphism
        .map
        .iter()
        .any(|image| *image >= morphism.target_size)
    {
        return Err(MorphismError::InvalidMorphism {
            reason: "image-out-of-range",
        });
    }
    Ok(())
}

fn validate_table(table: &OpTable) -> Result<(), MorphismError> {
    if table.carrier_size == 0 {
        return Err(MorphismError::InvalidMorphism {
            reason: "empty-carrier",
        });
    }
    if table.carrier_size > MAX_CARRIER_SIZE {
        return Err(MorphismError::InvalidMorphism {
            reason: "carrier-too-large",
        });
    }
    let expected = usize::from(table.carrier_size).saturating_mul(usize::from(table.carrier_size));
    if table.cells.len() != expected {
        return Err(MorphismError::InvalidMorphism {
            reason: "table-length",
        });
    }
    if table.cells.iter().any(|cell| *cell >= table.carrier_size) {
        return Err(MorphismError::InvalidMorphism {
            reason: "cell-out-of-range",
        });
    }
    Ok(())
}

fn first_homo_violation(
    morphism: &WorldMorphism,
    source: &OpTable,
    target: &OpTable,
) -> Option<MorphismViolation> {
    let n = morphism.source_size;
    for a in 0..n {
        for b in 0..n {
            let image_of_op = morphism.map[usize::from(source.apply(a, b))];
            let op_of_images =
                target.apply(morphism.map[usize::from(a)], morphism.map[usize::from(b)]);
            if image_of_op != op_of_images {
                return Some(MorphismViolation { pair: [a, b] });
            }
        }
    }
    None
}

fn observational_eq(table: &OpTable, left: u8, right: u8) -> bool {
    let n = table.carrier_size;
    for other in 0..n {
        if table.apply(left, other) != table.apply(right, other) {
            return false;
        }
        if table.apply(other, left) != table.apply(other, right) {
            return false;
        }
    }
    true
}

fn class_rep(table: &OpTable, element: u8) -> u8 {
    for candidate in 0..table.carrier_size {
        if observational_eq(table, element, candidate) {
            return candidate;
        }
    }
    element
}

fn class_index(unique: &[u8], rep: u8) -> u8 {
    unique
        .iter()
        .position(|candidate| *candidate == rep)
        .and_then(|index| u8::try_from(index).ok())
        .unwrap_or(0)
}

fn next_permutation(values: &mut [u8]) -> bool {
    let Some(pivot) = (0..values.len().saturating_sub(1))
        .rev()
        .find(|&index| values[index] < values[index + 1])
    else {
        return false;
    };
    let swap_at = (pivot + 1..values.len())
        .rev()
        .find(|&index| values[index] > values[pivot])
        .unwrap_or(pivot);
    values.swap(pivot, swap_at);
    values[pivot + 1..].reverse();
    true
}

fn dedupe_canonical(receipt: &DedupeReceipt) -> String {
    let mut out = String::from("dedupe([");
    for (index, group) in receipt.groups.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "rep([{}])", join_cells(&group.representative.cells));
        for dropped in &group.dropped {
            let _ = write!(
                out,
                ",drop([{}],{})",
                join_cells(&dropped.table.cells),
                dropped.witness.canonical()
            );
        }
    }
    out.push_str("])");
    out
}

fn join_cells(cells: &[u8]) -> String {
    cells
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn table_json(table: &OpTable) -> Json {
    Json::Array(
        table
            .cells
            .iter()
            .map(|cell| Json::Number(cell.to_string()))
            .collect(),
    )
}

fn morphism_map_json(morphism: &WorldMorphism) -> Json {
    Json::Array(
        morphism
            .map
            .iter()
            .map(|image| Json::Number(image.to_string()))
            .collect(),
    )
}

fn dedupe_group_json(group: &DedupeGroup) -> Json {
    let mut object = BTreeMap::new();
    object.insert(
        "dropped",
        Json::Array(
            group
                .dropped
                .iter()
                .map(|dropped| {
                    let mut item = BTreeMap::new();
                    item.insert("table", table_json(&dropped.table));
                    item.insert("witness", morphism_map_json(&dropped.witness));
                    Json::Object(item)
                })
                .collect(),
        ),
    );
    object.insert("representative", table_json(&group.representative));
    Json::Object(object)
}

enum Json {
    Str(String),
    Number(String),
    Bool(bool),
    Array(Vec<Json>),
    Object(BTreeMap<&'static str, Json>),
}

fn emit_object(fields: &BTreeMap<&'static str, Json>) -> String {
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
        Json::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
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
        check_version, dedupe, find_isomorphism, mine_invariants, morphism_id, quotient, verify,
        MorphismError, OpTable, WorldMorphism, MAX_ISO_SEARCH_SIZE, MORPHISM_VERSION,
    };

    fn xor_table() -> OpTable {
        OpTable {
            carrier_size: 2,
            cells: vec![0, 1, 1, 0],
        }
    }

    fn xnor_table() -> OpTable {
        OpTable {
            carrier_size: 2,
            cells: vec![1, 0, 0, 1],
        }
    }

    fn constant_table() -> OpTable {
        OpTable {
            carrier_size: 2,
            cells: vec![0, 0, 0, 0],
        }
    }

    fn and_table() -> OpTable {
        OpTable {
            carrier_size: 2,
            cells: vec![0, 0, 0, 1],
        }
    }

    #[test]
    fn happy_path_xor_and_constant_quotients() {
        let xor = quotient(&xor_table()).expect("xor quotient");
        assert_eq!(xor.classes, vec![vec![0], vec![1]]);
        assert_eq!(xor.table.carrier_size, 2);
        assert_eq!(xor.table.cells, vec![0, 1, 1, 0]);
        assert_eq!(xor.projection.map, vec![0, 1]);

        let constant = quotient(&constant_table()).expect("constant quotient");
        assert_eq!(constant.classes, vec![vec![0, 1]]);
        assert_eq!(constant.table.carrier_size, 1);
        assert_eq!(constant.table.cells, vec![0]);
        assert_eq!(constant.projection.map, vec![0, 0]);
    }

    #[test]
    fn homomorphism_identity_ok_wrong_map_refused() {
        let xor = xor_table();
        let identity = WorldMorphism {
            source_size: 2,
            target_size: 2,
            map: vec![0, 1],
        };
        assert_eq!(verify(&identity, &xor, &xor), Ok(()));

        let wrong = WorldMorphism {
            source_size: 2,
            target_size: 2,
            map: vec![1, 0],
        };
        assert_eq!(
            verify(&wrong, &xor, &xor),
            Err(MorphismError::NotAHomomorphism { pair: [0, 0] })
        );
    }

    #[test]
    fn isomorphism_relabeling_detected_xor_vs_constant_none() {
        let witness = find_isomorphism(&xor_table(), &xnor_table())
            .expect("budget")
            .expect("iso");
        assert_eq!(witness.map, vec![1, 0]);
        assert_eq!(verify(&witness, &xor_table(), &xnor_table()), Ok(()));
        assert_eq!(
            find_isomorphism(&xor_table(), &constant_table()).expect("budget"),
            None
        );
    }

    #[test]
    fn invariant_mining_shared_commutative_not_identity() {
        // XOR has a two-sided identity (0). The constant-0 table is
        // commutative (AND-like collapse) but has no identity, so
        // existential identity is not shared. AND on {0,1} would share
        // identity existentially (element 1).
        let report = mine_invariants(&[xor_table(), constant_table()]).expect("mine");
        assert_eq!(report.world_count, 2);
        assert_eq!(
            report.shared,
            vec!["commutative".to_string(), "associative".to_string()]
        );
        let by_law = |token: &str| {
            report
                .laws
                .iter()
                .find(|verdict| verdict.law == token)
                .cloned()
                .expect(token)
        };
        assert_eq!(by_law("commutative").holds, vec![true, true]);
        assert_eq!(by_law("associative").holds, vec![true, true]);
        assert_eq!(by_law("left-identity").holds, vec![true, false]);
        assert_eq!(by_law("right-identity").holds, vec![true, false]);
        assert_eq!(by_law("identity").holds, vec![true, false]);
        assert!(!by_law("identity").shared);
    }

    #[test]
    fn dedupe_groups_isomorphic_pair() {
        let receipt = dedupe(&[xor_table(), xnor_table(), constant_table()]).expect("dedupe");
        assert_eq!(receipt.groups.len(), 2);
        assert_eq!(receipt.groups[0].representative.cells, vec![0, 0, 0, 0]);
        assert!(receipt.groups[0].dropped.is_empty());
        assert_eq!(receipt.groups[1].representative.cells, vec![0, 1, 1, 0]);
        assert_eq!(receipt.groups[1].dropped.len(), 1);
        assert_eq!(receipt.groups[1].dropped[0].table.cells, vec![1, 0, 0, 1]);
        assert_eq!(receipt.groups[1].dropped[0].witness.map, vec![1, 0]);
    }

    #[test]
    fn malformed_maps_and_versions_refuse() {
        let xor = xor_table();
        let short = WorldMorphism {
            source_size: 2,
            target_size: 2,
            map: vec![0],
        };
        assert_eq!(
            verify(&short, &xor, &xor),
            Err(MorphismError::InvalidMorphism {
                reason: "map-length"
            })
        );
        let oob = WorldMorphism {
            source_size: 2,
            target_size: 2,
            map: vec![0, 2],
        };
        assert_eq!(
            verify(&oob, &xor, &xor),
            Err(MorphismError::InvalidMorphism {
                reason: "image-out-of-range"
            })
        );
        let mismatched = WorldMorphism {
            source_size: 1,
            target_size: 1,
            map: vec![0],
        };
        assert_eq!(
            verify(&mismatched, &xor, &xor),
            Err(MorphismError::SizeMismatch)
        );
        assert_eq!(
            WorldMorphism::new(2, 2, vec![0]),
            Err(MorphismError::InvalidMorphism {
                reason: "map-length"
            })
        );
        assert_eq!(check_version(MORPHISM_VERSION), Ok(()));
        assert_eq!(
            check_version(MORPHISM_VERSION + 1),
            Err(MorphismError::UnknownVersion {
                version: MORPHISM_VERSION + 1
            })
        );
        let oversized = usize::from(MAX_ISO_SEARCH_SIZE + 1);
        let cells = vec![0; oversized.saturating_mul(oversized)];
        assert_eq!(
            find_isomorphism(
                &OpTable {
                    carrier_size: MAX_ISO_SEARCH_SIZE + 1,
                    cells: cells.clone(),
                },
                &OpTable {
                    carrier_size: MAX_ISO_SEARCH_SIZE + 1,
                    cells,
                }
            ),
            Err(MorphismError::BudgetExceeded {
                limit: u64::from(MAX_ISO_SEARCH_SIZE)
            })
        );
    }

    #[test]
    fn receipts_are_byte_identical_across_runs() {
        let first_q = quotient(&constant_table()).expect("q1").to_json();
        let second_q = quotient(&constant_table()).expect("q2").to_json();
        assert_eq!(first_q, second_q);
        assert!(first_q.contains("\"schema\":\"emath.world-morphism\""));

        let first_i = mine_invariants(&[xor_table(), and_table(), constant_table()])
            .expect("i1")
            .to_json();
        let second_i = mine_invariants(&[xor_table(), and_table(), constant_table()])
            .expect("i2")
            .to_json();
        assert_eq!(first_i, second_i);

        let first_d = dedupe(&[xnor_table(), xor_table(), constant_table()])
            .expect("d1")
            .to_json();
        let second_d = dedupe(&[xnor_table(), xor_table(), constant_table()])
            .expect("d2")
            .to_json();
        assert_eq!(first_d, second_d);

        let morphism = WorldMorphism {
            source_size: 2,
            target_size: 2,
            map: vec![0, 1],
        };
        assert_eq!(morphism_id(&morphism), morphism_id(&morphism));
    }
}
