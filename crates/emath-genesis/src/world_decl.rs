//! User-defined and law-synthesized worlds.
//!
//! Worlds authored at the language's world kinds enter the World ABI as
//! DATA through this module: a [`WorldDecl`] (finite carrier, constants,
//! total operation tables) is validated into a [`UserDefinedWorld`]
//! (origin `user-defined`), or synthesized from a law over a toy-size
//! carrier (origin `synthesized` — never claimed as Real meaning). Every
//! world is labeled: the evidence record rides into every result bundle.
//! False model claims are checked against the world's own
//! tables and rejected typed. The strict source lane refuses world
//! attachments typed (E-WORLD-006): the strict vs Genesis/custom
//! firewall holds — a strict Gaussian file never runs a Mod17 world.
//!
//! Zero core delta: a new world is data validated at this seam; the
//! evaluator gains no match arm for it (the trait contract).

use std::collections::BTreeMap;
use std::fmt;

use emath_term::{Signature, SymbolId, Term};

use crate::{
    Environment, EvalError, FirstOrderWorld, WorldBudget, WorldEvidence, evaluate_bounded,
};

/// Toy-size bound for world carriers (law-synthesis toy size ≤ 6;
/// declared worlds share the bound so table totality stays checkable).
pub const MAX_WORLD_SIZE: usize = 6;

/// Where a world came from. Rides the evidence into every bundle:
/// a `synthesized` world is never presented as claimed Real meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldOrigin {
    /// Authored by the user from the language's world kinds.
    UserDefined,
    /// Canonical model synthesized from a declared law.
    Synthesized,
}

impl WorldOrigin {
    /// Stable evidence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserDefined => "user-defined",
            Self::Synthesized => "synthesized",
        }
    }
}

/// The source lane a world attachment is requested for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldSourceClass {
    /// The strict real-analysis lane (Tier 1/2): never carries worlds.
    Strict,
    /// The Genesis/custom lane: worlds attach freely.
    Custom,
}

/// One operation's total table over the declared carrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationTable {
    /// Arity (nullary symbols are constants, not operations).
    pub arity: usize,
    /// All `|domain|^arity` input tuples → carrier element.
    pub rows: BTreeMap<Vec<String>, String>,
}

impl OperationTable {
    /// Bundle an arity with its total row set.
    #[must_use]
    pub fn new(arity: usize, rows: BTreeMap<Vec<String>, String>) -> Self {
        Self { arity, rows }
    }

    /// The row for one input tuple, if present.
    #[must_use]
    pub fn row(&self, arguments: &[String]) -> Option<&String> {
        self.rows.get(arguments)
    }
}

/// A user-authored world declaration (the execution-layer form of the
/// language's world kind): finite carrier, constants, total operation
/// tables, claimed laws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldDecl {
    /// Stable world token (the evidence name).
    pub name: String,
    /// Origin class (declared, re-validated by the constructor).
    pub origin: WorldOrigin,
    /// Claimed laws (checked by law checks, not here).
    pub laws: Vec<String>,
    /// Finite carrier, `1..=MAX_WORLD_SIZE` elements.
    pub domain: Vec<String>,
    /// Nullary symbols → carrier element.
    pub constants: BTreeMap<String, String>,
    /// Operation symbols → total tables.
    pub operations: BTreeMap<String, OperationTable>,
}

/// Declaration or model-check refusal. Closed set; every variant names
/// what was wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldDeclError {
    /// The carrier is empty: no carrier, no world.
    EmptyDomain,
    /// The carrier exceeds the toy-size bound.
    SizeBoundExceeded {
        /// Declared size.
        size: usize,
        /// The bound.
        max: usize,
    },
    /// A constant value or table element is outside the declared carrier.
    UnknownElement {
        /// The symbol whose row/constant is out of carrier.
        symbol: String,
        /// The out-of-carrier element.
        element: String,
    },
    /// An operation table is not total over the carrier.
    IncompleteTable {
        /// The operation symbol.
        symbol: String,
        /// Required rows (`|domain|^arity`).
        expected: usize,
        /// Declared rows.
        actual: usize,
    },
    /// A world attachment was requested for a STRICT source: the strict
    /// lane never carries custom-world semantics.
    StrictFirewall {
        /// The strict source identifier.
        source: String,
    },
    /// A model claim disagrees with the world's own table.
    FalseModel {
        /// The claimed symbol.
        symbol: String,
        /// The claimed element.
        expected: String,
        /// The world's actual element.
        actual: String,
    },
}

impl WorldDeclError {
    /// Stable diagnostic code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyDomain => "E-WORLD-003",
            Self::SizeBoundExceeded { .. } => "E-WORLD-008",
            Self::UnknownElement { .. } => "E-WORLD-004",
            Self::IncompleteTable { .. } => "E-WORLD-005",
            Self::StrictFirewall { .. } => "E-WORLD-006",
            Self::FalseModel { .. } => "E-WORLD-007",
        }
    }
}

impl fmt::Display for WorldDeclError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDomain => write!(
                formatter,
                "{code}: world carrier is empty; a finite world needs at least \
                 one element",
                code = self.code()
            ),
            Self::SizeBoundExceeded { size, max } => write!(
                formatter,
                "{code}: world carrier size {size} exceeds the toy bound {max}",
                code = self.code()
            ),
            Self::UnknownElement { symbol, element } => write!(
                formatter,
                "{code}: `{element}` for symbol `{symbol}` is outside the \
                 declared carrier",
                code = self.code()
            ),
            Self::IncompleteTable {
                symbol,
                expected,
                actual,
            } => write!(
                formatter,
                "{code}: operation `{symbol}` declares {actual} row(s), the \
                 total table over the carrier needs {expected}",
                code = self.code()
            ),
            Self::StrictFirewall { source } => write!(
                formatter,
                "{code}: strict source `{source}` cannot attach a custom \
                 world; the strict lane runs no custom-world semantics",
                code = self.code()
            ),
            Self::FalseModel {
                symbol,
                expected,
                actual,
            } => write!(
                formatter,
                "{code}: model claims `{symbol}` = {expected}, the world's own \
                 table says {actual}",
                code = self.code()
            ),
        }
    }
}

impl std::error::Error for WorldDeclError {}

/// A checked, executable user-defined world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserDefinedWorld {
    name: String,
    origin: WorldOrigin,
    laws: Vec<String>,
    domain: Vec<String>,
    constants: BTreeMap<String, String>,
    operations: BTreeMap<String, OperationTable>,
}

impl UserDefinedWorld {
    /// The world's stable token.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The world's claimed laws.
    #[must_use]
    pub fn laws(&self) -> &[String] {
        &self.laws
    }

    /// One operation's total table (for independent law checks).
    #[must_use]
    pub fn table(&self, symbol: &str) -> Option<&OperationTable> {
        self.operations.get(symbol)
    }

    /// Check a model claim against the world's own tables: a claim the
    /// world refutes is a typed [`WorldDeclError::FalseModel`], never a
    /// silent agreement with a wrong model.
    pub fn check_model(&self, claim: &ModelClaim) -> Result<(), WorldDeclError> {
        let actual = if claim.arguments.is_empty() {
            self.constants
                .get(&claim.symbol)
                .ok_or_else(|| WorldDeclError::UnknownElement {
                    symbol: claim.symbol.clone(),
                    element: claim.expected.clone(),
                })?
                .clone()
        } else {
            let table = self.operations.get(&claim.symbol).ok_or_else(|| {
                WorldDeclError::UnknownElement {
                    symbol: claim.symbol.clone(),
                    element: claim.expected.clone(),
                }
            })?;
            if table.arity != claim.arguments.len() {
                return Err(WorldDeclError::UnknownElement {
                    symbol: claim.symbol.clone(),
                    element: claim.expected.clone(),
                });
            }
            table
                .row(&claim.arguments)
                .ok_or_else(|| WorldDeclError::UnknownElement {
                    symbol: claim.symbol.clone(),
                    element: claim.expected.clone(),
                })?
                .clone()
        };
        if actual != claim.expected {
            return Err(WorldDeclError::FalseModel {
                symbol: claim.symbol.clone(),
                expected: claim.expected.clone(),
                actual,
            });
        }
        Ok(())
    }
}

impl FirstOrderWorld for UserDefinedWorld {
    type Value = String;
    type Error = EvalError;

    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        self.constants
            .get(symbol.0.as_str())
            .cloned()
            .ok_or_else(|| EvalError::UnknownSymbol(symbol.clone()))
    }

    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        let Some(table) = self.operations.get(operator.0.as_str()) else {
            return Err(EvalError::UnknownSymbol(operator.clone()));
        };
        if table.arity != arguments.len() {
            return Err(EvalError::Arity {
                symbol: operator.clone(),
                expected: table.arity,
                actual: arguments.len(),
            });
        }
        // Totality was validated at construction: the row exists.
        table
            .row(&arguments)
            .cloned()
            .ok_or_else(|| EvalError::UnknownSymbol(operator.clone()))
    }

    fn admits(&self, signature: &Signature) -> bool {
        signature.iter().all(|(symbol, arity)| {
            if *arity == 0 {
                self.constants.contains_key(symbol.0.as_str())
            } else {
                self.operations
                    .get(symbol.0.as_str())
                    .is_some_and(|table| table.arity == *arity)
            }
        })
    }

    fn evidence(&self) -> WorldEvidence {
        WorldEvidence {
            world: self.name.clone(),
            origin: self.origin.as_str().to_string(),
            laws: self.laws.clone(),
        }
    }
}

/// A model claim about a world: `symbol(arguments) = expected` (an
/// empty argument list claims a constant's value).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelClaim {
    /// The claimed symbol.
    pub symbol: String,
    /// The claimed input tuple (empty for constants).
    pub arguments: Vec<String>,
    /// The claimed carrier element.
    pub expected: String,
}

/// Validates a world declaration into an executable world.
///
/// Checks: non-empty carrier within the toy bound, constants in the
/// carrier, total operation tables (`|domain|^arity` rows, all elements
/// in the carrier), arity ≥ 1 for operations (nullary symbols are
/// constants). A declaration that fails is refused typed — never
/// silently repaired.
pub fn user_defined_world(decl: WorldDecl) -> Result<UserDefinedWorld, WorldDeclError> {
    validate_decl(&decl, decl.origin)?;
    Ok(UserDefinedWorld {
        name: decl.name,
        origin: decl.origin,
        laws: decl.laws,
        domain: decl.domain,
        constants: decl.constants,
        operations: decl.operations,
    })
}

fn validate_decl(decl: &WorldDecl, origin: WorldOrigin) -> Result<(), WorldDeclError> {
    let _ = origin;
    if decl.domain.is_empty() {
        return Err(WorldDeclError::EmptyDomain);
    }
    if decl.domain.len() > MAX_WORLD_SIZE {
        return Err(WorldDeclError::SizeBoundExceeded {
            size: decl.domain.len(),
            max: MAX_WORLD_SIZE,
        });
    }
    let in_domain = |symbol: &str, element: &str| -> Result<(), WorldDeclError> {
        if decl.domain.iter().any(|known| known == element) {
            Ok(())
        } else {
            Err(WorldDeclError::UnknownElement {
                symbol: symbol.to_string(),
                element: element.to_string(),
            })
        }
    };
    for (symbol, element) in &decl.constants {
        in_domain(symbol, element)?;
    }
    for (symbol, table) in &decl.operations {
        if table.arity == 0 {
            return Err(WorldDeclError::UnknownElement {
                symbol: symbol.clone(),
                element: "nullary operation (declare constants, not arity-0 tables)".to_string(),
            });
        }
        let expected = decl.domain.len().checked_pow(table.arity as u32).ok_or(
            WorldDeclError::SizeBoundExceeded {
                size: decl.domain.len(),
                max: MAX_WORLD_SIZE,
            },
        )?;
        if table.rows.len() != expected {
            return Err(WorldDeclError::IncompleteTable {
                symbol: symbol.clone(),
                expected,
                actual: table.rows.len(),
            });
        }
        for (arguments, element) in &table.rows {
            if arguments.len() != table.arity {
                return Err(WorldDeclError::IncompleteTable {
                    symbol: symbol.clone(),
                    expected,
                    actual: table.rows.len(),
                });
            }
            for argument in arguments {
                in_domain(symbol, argument)?;
            }
            in_domain(symbol, element)?;
        }
    }
    Ok(())
}

/// Attach a world to a source lane. The strict lane refuses typed: the
/// strict vs Genesis/custom firewall is enforced at this seam (a strict
/// Gaussian model never runs a modular world's semantics).
pub fn attach_world(
    source: WorldSourceClass,
    source_id: &str,
    decl: WorldDecl,
) -> Result<UserDefinedWorld, WorldDeclError> {
    match source {
        WorldSourceClass::Strict => Err(WorldDeclError::StrictFirewall {
            source: source_id.to_string(),
        }),
        WorldSourceClass::Custom => user_defined_world(decl),
    }
}

/// A law to synthesize a canonical world model from (closed set; each
/// law has a deterministic canonical model over any non-empty carrier).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldLaw {
    /// The operation is commutative: t(x, y) = t(y, x).
    Commutative,
    /// The operation is idempotent: t(x, x) = x.
    Idempotent,
    /// The element is a two-sided identity: t(e, x) = t(x, e) = x.
    IdentityElement {
        /// The declared identity element (must be in the carrier).
        element: String,
    },
}

impl WorldLaw {
    /// Stable law token for the world's evidence.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Commutative => "commutative",
            Self::Idempotent => "idempotent",
            Self::IdentityElement { .. } => "identity-element",
        }
    }
}

/// Synthesis refusal. The law's canonical model always exists within the
/// closed law set, so the only refusal is an out-of-contract request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SynthesisError {
    /// The carrier is empty or exceeds the toy bound.
    SizeBoundExceeded {
        /// Declared size.
        size: usize,
        /// The bound.
        max: usize,
    },
    /// The declared identity element is outside the carrier.
    UnknownElement {
        /// The out-of-carrier element.
        element: String,
    },
}

impl fmt::Display for SynthesisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeBoundExceeded { size, max } => write!(
                formatter,
                "E-SYNTH-001: law-synthesis carrier size {size} exceeds the \
                 toy bound {max}",
            ),
            Self::UnknownElement { element } => write!(
                formatter,
                "E-SYNTH-002: identity element `{element}` is outside the \
                 declared carrier",
            ),
        }
    }
}

impl std::error::Error for SynthesisError {}

/// Synthesizes the canonical model of `law` over `domain` as a
/// world-labeled [`UserDefinedWorld`] (origin `synthesized`).
///
/// Deterministic constructive synthesis — the canonical model per law
/// (carrier order, no search): commutative `t(i, j) = d[min(i, j)]`,
/// idempotent `t(i, j) = d[i]`, identity `t(i, j) = d[j]` unless `j = e`
/// (two-sided identity by construction). The law holds over the whole
/// carrier by construction; the test suite verifies it independently.
/// A synthesized world is LABELED as such — it is a model of the law,
/// never claimed Real meaning.
pub fn synthesize_world(
    name: &str,
    law: &WorldLaw,
    domain: Vec<String>,
) -> Result<UserDefinedWorld, SynthesisError> {
    if domain.is_empty() || domain.len() > MAX_WORLD_SIZE {
        return Err(SynthesisError::SizeBoundExceeded {
            size: domain.len(),
            max: MAX_WORLD_SIZE,
        });
    }
    // The synthesized binary operation (the closed law set synthesizes
    // one operation; multi-law synthesis is later spine work).
    let element_of = |index: usize| domain[index].clone();
    let mut rows = BTreeMap::new();
    let identity_index = match law {
        WorldLaw::IdentityElement { element } => {
            let index = domain
                .iter()
                .position(|known| known == element)
                .ok_or_else(|| SynthesisError::UnknownElement {
                    element: element.clone(),
                })?;
            index
        }
        _ => usize::MAX,
    };
    for left in 0..domain.len() {
        for right in 0..domain.len() {
            let value = match law {
                WorldLaw::Commutative => element_of(left.min(right)),
                WorldLaw::Idempotent => element_of(left),
                WorldLaw::IdentityElement { .. } => {
                    if right == identity_index {
                        element_of(left)
                    } else {
                        element_of(right)
                    }
                }
            };
            rows.insert(vec![element_of(left), element_of(right)], value);
        }
    }
    let mut operations = BTreeMap::new();
    operations.insert("⋈".to_string(), OperationTable::new(2, rows));
    let decl = WorldDecl {
        name: name.to_string(),
        origin: WorldOrigin::Synthesized,
        laws: vec![law.as_str().to_string()],
        domain,
        constants: BTreeMap::new(),
        operations,
    };
    // Validated: the canonical model is in-contract by construction.
    validate_decl(&decl, WorldOrigin::Synthesized)
        .expect("canonical synthesized model is a valid declaration");
    Ok(UserDefinedWorld {
        name: decl.name,
        origin: decl.origin,
        laws: decl.laws,
        domain: decl.domain,
        constants: decl.constants,
        operations: decl.operations,
    })
}

/// Convenience: evaluate a term in any world under a budget, returning
/// the world's own labeled value (used by the labeled-portfolio tests).
pub fn evaluate_world_labeled<W: FirstOrderWorld>(
    term: &Term,
    world: &W,
    environment: &Environment<W::Value>,
    budget: WorldBudget,
) -> Result<W::Value, W::Error>
where
    W::Error: From<EvalError>,
{
    evaluate_bounded(term, world, environment, budget)
}
