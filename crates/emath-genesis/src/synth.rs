//! SG-14 finite-world synthesis: budgeted search over binary operation
//! tables on a finite carrier.
//!
//! A [`SynthRequest`] names a carrier `{0, …, n−1}`, a list of laws, and
//! a list of example triples `op(a, b) = c`. The search enumerates the
//! `n^(n²)` tables in a fixed row-major mixed-radix order and returns
//! the first table that satisfies every constraint, or a typed refusal.
//!
//! Rules (honest, no silent truncation):
//!
//! - Carrier elements are canonical indices `0..n`. Size `0` is refused.
//!   Size above [`MAX_CARRIER_SIZE`] is refused (`carrier-too-large`).
//! - Enumeration is total: table index `i` decodes to `n²` cells, least
//!   significant digit first, row-major `(a, b)` pairs.
//! - [`SynthBudget::max_tables`] is a hard window, not a hint. Exhausting
//!   the window with tables still unexamined is [`SynthError::BudgetExceeded`],
//!   never a truncated “no table” answer. Exhausting the space inside the
//!   window is [`SynthError::Unsatisfiable`].
//! - [`SynthRequest::resume_cursor`] is the next unexamined index. A
//!   continued request with the same constraints and that cursor resumes
//!   the same enumeration. Budget and cursor are execution parameters
//!   and are excluded from [`synth_id`].
//!
//! Determinism class: pure integer enumeration, no floats. Receipts are
//! BTreeMap-ordered JSON, byte-identical across runs.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use emath_world_ir::fnv1a64;

/// Finite-world synthesis schema id for artifacts and receipts.
pub const SYNTH_SCHEMA: &str = "emath.finite-world";
/// Finite-world synthesis schema version. Bump on any change to the
/// canonical request encoding, the enumeration order, law semantics, or
/// the receipt layout; consumers refuse versions they do not know.
pub const SYNTH_VERSION: u32 = 1;
/// Hard carrier-size ceiling. Enumeration space is `n^(n²)`; a larger
/// carrier is a typed refusal, not an attempted search.
pub const MAX_CARRIER_SIZE: u8 = 8;

/// Refuses schema versions this module does not understand.
pub fn check_version(version: u32) -> Result<(), SynthError> {
    if version == SYNTH_VERSION {
        Ok(())
    } else {
        Err(SynthError::UnknownVersion { version })
    }
}

/// Typed refusals for finite-world synthesis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SynthError {
    /// Schema version this module does not understand.
    UnknownVersion {
        /// Offending version.
        version: u32,
    },
    /// The search window filled without a winner and tables remain.
    BudgetExceeded {
        /// Budget limit that was exhausted.
        limit: u64,
    },
    /// Every table in range was examined; none satisfied the constraints.
    Unsatisfiable {
        /// Tables actually examined in this window.
        tables_examined: u64,
    },
    /// Request failed a well-formedness check.
    InvalidRequest {
        /// Stable reason token.
        reason: &'static str,
    },
}

/// A declared law over the binary operation being synthesized.
///
/// Vocabulary mirrors `emath-holes` / `emath-law-check` (commutative,
/// associative, identity) and adds left/right identity, each either
/// named or existential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SynthLaw {
    /// `op(x, y) == op(y, x)` for all pairs.
    Commutative,
    /// `op(op(x, y), z) == op(x, op(y, z))` for all triples.
    Associative,
    /// Left identity: named `e` or some existing `e`, `op(e, x) == x`.
    LeftIdentity {
        /// Named element, or `None` for existential.
        element: Option<u8>,
    },
    /// Right identity: named `e` or some existing `e`, `op(x, e) == x`.
    RightIdentity {
        /// Named element, or `None` for existential.
        element: Option<u8>,
    },
    /// Two-sided identity: named `e` or some existing `e`.
    Identity {
        /// Named element, or `None` for existential.
        element: Option<u8>,
    },
}

impl SynthLaw {
    /// Canonical law token (used in request identity).
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Commutative => "commutative".to_string(),
            Self::Associative => "associative".to_string(),
            Self::LeftIdentity { element: None } => "left-identity".to_string(),
            Self::LeftIdentity { element: Some(e) } => format!("left-identity({e})"),
            Self::RightIdentity { element: None } => "right-identity".to_string(),
            Self::RightIdentity { element: Some(e) } => format!("right-identity({e})"),
            Self::Identity { element: None } => "identity".to_string(),
            Self::Identity { element: Some(e) } => format!("identity({e})"),
        }
    }
}

/// Example constraint `op(left, right) == result`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SynthExample {
    /// Left operand (carrier index).
    pub left: u8,
    /// Right operand (carrier index).
    pub right: u8,
    /// Required result (carrier index).
    pub result: u8,
}

impl SynthExample {
    fn canonical(self) -> String {
        format!("example({},{},{})", self.left, self.right, self.result)
    }
}

/// Search window: maximum tables one [`SynthRequest::synthesize`] call
/// may examine. Excluded from [`synth_id`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SynthBudget {
    /// Maximum tables examined in this window.
    pub max_tables: u64,
}

impl Default for SynthBudget {
    fn default() -> Self {
        Self { max_tables: 256 }
    }
}

/// One finite-world synthesis request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthRequest {
    /// Carrier size `n`; elements are `0..n`.
    pub carrier_size: u8,
    /// Laws the table must satisfy (request order is part of identity).
    pub laws: Vec<SynthLaw>,
    /// Example triples the table must satisfy.
    pub examples: Vec<SynthExample>,
    /// Search window. Excluded from [`synth_id`].
    pub budget: SynthBudget,
    /// Next unexamined table index. Excluded from [`synth_id`].
    pub resume_cursor: u64,
}

/// A binary operation table on `{0, …, n−1}`.
///
/// Cells are stored row-major: `cells[a * n + b] == op(a, b)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpTable {
    /// Carrier size.
    pub carrier_size: u8,
    /// `n²` cells, row-major.
    pub cells: Vec<u8>,
}

impl OpTable {
    /// Decode enumeration index `index` into a table of size `n`.
    #[must_use]
    pub fn from_index(n: u8, mut index: u64) -> Self {
        let width = usize::from(n);
        let mut cells = vec![0_u8; width.saturating_mul(width)];
        if n == 0 {
            return Self {
                carrier_size: n,
                cells,
            };
        }
        let radix = u64::from(n);
        for cell in &mut cells {
            *cell = u8::try_from(index % radix).unwrap_or(0);
            index /= radix;
        }
        Self {
            carrier_size: n,
            cells,
        }
    }

    /// `op(left, right)`.
    #[must_use]
    pub fn apply(&self, left: u8, right: u8) -> u8 {
        let n = usize::from(self.carrier_size);
        self.cells[usize::from(left) * n + usize::from(right)]
    }
}

/// Deterministic machine-readable synthesis receipt (a winner).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthReceipt {
    /// Schema version echoed into the JSON.
    pub version: u32,
    /// FNV-1a64 of the versioned canonical request (budget/cursor out).
    pub request_id: u64,
    /// Carrier size.
    pub carrier_size: u8,
    /// Tables examined in this window (including the winner).
    pub tables_examined: u64,
    /// Index of the next unexamined table (winner index + 1).
    pub resume_cursor: u64,
    /// Deterministic first winner.
    pub table: OpTable,
}

impl SynthReceipt {
    /// BTreeMap-ordered JSON. Key order is lexicographic. Byte-identical
    /// across runs for the same receipt.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut root = BTreeMap::new();
        root.insert("carrier_size", Json::Number(self.carrier_size.to_string()));
        root.insert("request_id", Json::Str(format!("{:016x}", self.request_id)));
        root.insert(
            "resume_cursor",
            Json::Number(self.resume_cursor.to_string()),
        );
        root.insert("schema", Json::Str(SYNTH_SCHEMA.to_string()));
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
        root.insert(
            "tables_examined",
            Json::Number(self.tables_examined.to_string()),
        );
        root.insert("version", Json::Number(self.version.to_string()));
        emit_object(&root)
    }
}

/// A law-checker refusal with a concrete counterexample triple.
///
/// For commutativity the third slot is `0` (unused). For identity the
/// triple is `(e, x, 0)` or `(x, e, 0)` for the first failing pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawViolation {
    /// Canonical law token that failed.
    pub law: String,
    /// Lexicographically first counterexample `(a, b, c)`.
    pub counterexample: [u8; 3],
}

/// Alpha-style synthesis identity: FNV-1a64 over the versioned canonical
/// request. Budget and resume cursor are excluded.
#[must_use]
pub fn synth_id(request: &SynthRequest) -> u64 {
    fnv1a64(format!("{SYNTH_SCHEMA}.v{SYNTH_VERSION}:{}", request.canonical()).as_bytes())
}

impl SynthRequest {
    /// Canonical request text (budget and cursor omitted).
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        let _ = write!(out, "synth({},[", self.carrier_size);
        for (index, law) in self.laws.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&law.canonical());
        }
        out.push_str("],[");
        for (index, example) in self.examples.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&example.canonical());
        }
        out.push_str("])");
        out
    }

    /// Search for the first satisfying table in this window.
    pub fn synthesize(&self) -> Result<SynthReceipt, SynthError> {
        check_version(SYNTH_VERSION)?;
        validate(self)?;
        let n = self.carrier_size;
        let total = table_space(n);
        let start = self.resume_cursor;
        if let Some(total) = total {
            if start > total {
                return Err(SynthError::InvalidRequest {
                    reason: "cursor-out-of-range",
                });
            }
            if start == total {
                return Err(SynthError::Unsatisfiable { tables_examined: 0 });
            }
        }

        let limit = self.budget.max_tables;
        let mut examined = 0_u64;
        let mut index = start;
        while examined < limit {
            if let Some(total) = total {
                if index >= total {
                    return Err(SynthError::Unsatisfiable {
                        tables_examined: examined,
                    });
                }
            }
            let table = OpTable::from_index(n, index);
            examined += 1;
            if table_satisfies(&table, &self.laws, &self.examples) {
                return Ok(SynthReceipt {
                    version: SYNTH_VERSION,
                    request_id: synth_id(self),
                    carrier_size: n,
                    tables_examined: examined,
                    resume_cursor: index.saturating_add(1),
                    table,
                });
            }
            index = index.saturating_add(1);
        }
        if let Some(total) = total {
            if index >= total {
                return Err(SynthError::Unsatisfiable {
                    tables_examined: examined,
                });
            }
        }
        Err(SynthError::BudgetExceeded { limit })
    }
}

/// Independent law check over an existing table. The first
/// lexicographic violation is returned; this is the seeded-negative-
/// control seam (plant a bad table, read the counterexample).
pub fn check_table(table: &OpTable, laws: &[SynthLaw]) -> Result<(), LawViolation> {
    for law in laws {
        if let Some(violation) = law_violation(table, law) {
            return Err(violation);
        }
    }
    Ok(())
}

fn validate(request: &SynthRequest) -> Result<(), SynthError> {
    if request.carrier_size == 0 {
        return Err(SynthError::InvalidRequest {
            reason: "empty-carrier",
        });
    }
    if request.carrier_size > MAX_CARRIER_SIZE {
        return Err(SynthError::InvalidRequest {
            reason: "carrier-too-large",
        });
    }
    let n = request.carrier_size;
    for example in &request.examples {
        if example.left >= n || example.right >= n || example.result >= n {
            return Err(SynthError::InvalidRequest {
                reason: "example-out-of-range",
            });
        }
    }
    for law in &request.laws {
        let named = match law {
            SynthLaw::LeftIdentity { element }
            | SynthLaw::RightIdentity { element }
            | SynthLaw::Identity { element } => *element,
            SynthLaw::Commutative | SynthLaw::Associative => None,
        };
        if let Some(element) = named {
            if element >= n {
                return Err(SynthError::InvalidRequest {
                    reason: "identity-out-of-range",
                });
            }
        }
    }
    Ok(())
}

fn table_space(n: u8) -> Option<u64> {
    let cells = u32::from(n).saturating_mul(u32::from(n));
    u64::from(n).checked_pow(cells)
}

fn table_satisfies(table: &OpTable, laws: &[SynthLaw], examples: &[SynthExample]) -> bool {
    if check_table(table, laws).is_err() {
        return false;
    }
    examples
        .iter()
        .all(|example| table.apply(example.left, example.right) == example.result)
}

fn law_violation(table: &OpTable, law: &SynthLaw) -> Option<LawViolation> {
    let n = table.carrier_size;
    match law {
        SynthLaw::Commutative => {
            for a in 0..n {
                for b in 0..n {
                    if table.apply(a, b) != table.apply(b, a) {
                        return Some(LawViolation {
                            law: law.canonical(),
                            counterexample: [a, b, 0],
                        });
                    }
                }
            }
            None
        }
        SynthLaw::Associative => {
            for a in 0..n {
                for b in 0..n {
                    for c in 0..n {
                        let left = table.apply(table.apply(a, b), c);
                        let right = table.apply(a, table.apply(b, c));
                        if left != right {
                            return Some(LawViolation {
                                law: law.canonical(),
                                counterexample: [a, b, c],
                            });
                        }
                    }
                }
            }
            None
        }
        SynthLaw::LeftIdentity { element } => identity_violation(table, *element, true, false, law),
        SynthLaw::RightIdentity { element } => {
            identity_violation(table, *element, false, true, law)
        }
        SynthLaw::Identity { element } => identity_violation(table, *element, true, true, law),
    }
}

fn identity_violation(
    table: &OpTable,
    named: Option<u8>,
    left: bool,
    right: bool,
    law: &SynthLaw,
) -> Option<LawViolation> {
    let n = table.carrier_size;
    if let Some(e) = named {
        return first_identity_fail(table, e, left, right, law);
    }
    for e in 0..n {
        if first_identity_fail(table, e, left, right, law).is_none() {
            return None;
        }
    }
    Some(LawViolation {
        law: law.canonical(),
        counterexample: [0, 0, 0],
    })
}

fn first_identity_fail(
    table: &OpTable,
    e: u8,
    left: bool,
    right: bool,
    law: &SynthLaw,
) -> Option<LawViolation> {
    let n = table.carrier_size;
    for x in 0..n {
        if left && table.apply(e, x) != x {
            return Some(LawViolation {
                law: law.canonical(),
                counterexample: [e, x, 0],
            });
        }
        if right && table.apply(x, e) != x {
            return Some(LawViolation {
                law: law.canonical(),
                counterexample: [x, e, 0],
            });
        }
    }
    None
}

enum Json {
    Str(String),
    Number(String),
    Array(Vec<Json>),
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
        check_table, check_version, synth_id, OpTable, SynthBudget, SynthError, SynthExample,
        SynthLaw, SynthRequest, MAX_CARRIER_SIZE, SYNTH_VERSION,
    };

    fn request(n: u8, laws: Vec<SynthLaw>) -> SynthRequest {
        SynthRequest {
            carrier_size: n,
            laws,
            examples: Vec::new(),
            budget: SynthBudget::default(),
            resume_cursor: 0,
        }
    }

    fn comm_id() -> Vec<SynthLaw> {
        vec![SynthLaw::Commutative, SynthLaw::Identity { element: None }]
    }

    #[test]
    fn happy_path_size_two_commutative_identity() {
        let receipt = request(2, comm_id()).synthesize().expect("winner");
        assert_eq!(receipt.table.cells, vec![0, 1, 1, 0]);
        assert_eq!(receipt.carrier_size, 2);
        assert_eq!(receipt.tables_examined, 7);
        assert_eq!(receipt.resume_cursor, 7);
        assert_eq!(receipt.version, SYNTH_VERSION);
    }

    #[test]
    fn impossible_law_refuses_unsatisfiable() {
        let request = SynthRequest {
            carrier_size: 2,
            laws: vec![SynthLaw::Commutative],
            examples: vec![
                SynthExample {
                    left: 0,
                    right: 1,
                    result: 0,
                },
                SynthExample {
                    left: 1,
                    right: 0,
                    result: 1,
                },
            ],
            budget: SynthBudget { max_tables: 16 },
            resume_cursor: 0,
        };
        assert_eq!(
            request.synthesize(),
            Err(SynthError::Unsatisfiable {
                tables_examined: 16
            })
        );
    }

    #[test]
    fn budget_exceeded_then_split_equals_unsplit() {
        let oversized = SynthRequest {
            budget: SynthBudget { max_tables: 10 },
            ..request(3, comm_id())
        };
        assert_eq!(
            oversized.synthesize(),
            Err(SynthError::BudgetExceeded { limit: 10 })
        );

        let laws = comm_id();
        let unsplit = request(2, laws.clone()).synthesize().expect("unsplit");

        let first_window = SynthRequest {
            budget: SynthBudget { max_tables: 3 },
            ..request(2, laws.clone())
        };
        assert_eq!(
            first_window.synthesize(),
            Err(SynthError::BudgetExceeded { limit: 3 })
        );
        let continued = SynthRequest {
            budget: SynthBudget { max_tables: 16 },
            resume_cursor: 3,
            ..request(2, laws)
        }
        .synthesize()
        .expect("resume");
        assert_eq!(continued.table, unsplit.table);
        assert_eq!(continued.request_id, unsplit.request_id);
        assert_eq!(continued.table.cells, vec![0, 1, 1, 0]);
    }

    #[test]
    fn malformed_and_adversarial_requests_refuse() {
        assert_eq!(
            request(0, vec![SynthLaw::Commutative]).synthesize(),
            Err(SynthError::InvalidRequest {
                reason: "empty-carrier"
            })
        );
        assert_eq!(
            request(MAX_CARRIER_SIZE + 1, vec![SynthLaw::Commutative]).synthesize(),
            Err(SynthError::InvalidRequest {
                reason: "carrier-too-large"
            })
        );
        assert_eq!(
            SynthRequest {
                examples: vec![SynthExample {
                    left: 0,
                    right: 2,
                    result: 0,
                }],
                ..request(2, vec![SynthLaw::Commutative])
            }
            .synthesize(),
            Err(SynthError::InvalidRequest {
                reason: "example-out-of-range"
            })
        );
        assert_eq!(
            request(2, vec![SynthLaw::Identity { element: Some(5) }]).synthesize(),
            Err(SynthError::InvalidRequest {
                reason: "identity-out-of-range"
            })
        );
        assert_eq!(check_version(SYNTH_VERSION), Ok(()));
        assert_eq!(
            check_version(SYNTH_VERSION + 1),
            Err(SynthError::UnknownVersion {
                version: SYNTH_VERSION + 1
            })
        );
    }

    #[test]
    fn receipts_are_byte_identical_across_runs() {
        let request = request(2, comm_id());
        let first = request.synthesize().expect("first").to_json();
        let second = request.synthesize().expect("second").to_json();
        assert_eq!(first, second);
        assert!(first.starts_with('{'));
        assert!(first.contains("\"schema\":\"emath.finite-world\""));
        assert_eq!(synth_id(&request), synth_id(&request));
        let shifted_budget = SynthRequest {
            budget: SynthBudget { max_tables: 64 },
            ..request.clone()
        };
        assert_eq!(synth_id(&request), synth_id(&shifted_budget));
        let shifted_cursor = SynthRequest {
            resume_cursor: 1,
            ..request.clone()
        };
        assert_eq!(synth_id(&request), synth_id(&shifted_cursor));
    }

    #[test]
    fn every_commutative_winner_actually_commutes() {
        let mut cursor = 0_u64;
        let mut found = 0_u32;
        loop {
            let outcome = SynthRequest {
                resume_cursor: cursor,
                budget: SynthBudget { max_tables: 16 },
                ..request(2, vec![SynthLaw::Commutative])
            }
            .synthesize();
            match outcome {
                Ok(receipt) => {
                    for a in 0..2_u8 {
                        for b in 0..2_u8 {
                            assert_eq!(
                                receipt.table.apply(a, b),
                                receipt.table.apply(b, a),
                                "winner at cursor {cursor} must commute"
                            );
                        }
                    }
                    found += 1;
                    cursor = receipt.resume_cursor;
                }
                Err(SynthError::Unsatisfiable { .. }) => break,
                other => panic!("unexpected outcome {other:?}"),
            }
        }
        assert_eq!(found, 8, "size-2 has eight commutative tables");
    }

    #[test]
    fn seeded_non_associative_table_is_rejected_with_triple() {
        // NAND on {0,1}: op = 1 except op(1,1)=0. Not associative.
        let planted = OpTable {
            carrier_size: 2,
            cells: vec![1, 1, 1, 0],
        };
        let violation = check_table(&planted, &[SynthLaw::Associative])
            .expect_err("NAND must violate associativity");
        assert_eq!(violation.law, "associative");
        assert_eq!(violation.counterexample, [0, 0, 1]);
        let left = planted.apply(planted.apply(0, 0), 1);
        let right = planted.apply(0, planted.apply(0, 1));
        assert_ne!(left, right);
        assert_eq!(check_table(&planted, &[SynthLaw::Commutative]), Ok(()));
    }
}
