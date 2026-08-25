//! Declared operator surface (core declarations): attaches a semantics
//! name (and optional fixity) to an existing symbol at the SIR level.
//! `infixl`/`infixr`/`infix` all normalize to `Infix` (associativity is
//! resolved by parentheses in the line-oriented syntax); only `prefix`
//! and `postfix` change arity placement.

use emath_core::QualifiedName;

/// Fixity class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fixity {
    Infix,
    Prefix,
    Postfix,
}

impl Fixity {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Infix => "infix",
            Self::Prefix => "prefix",
            Self::Postfix => "postfix",
        }
    }
}

/// One declared operator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredOperator {
    /// Unique operator symbol name (for example `⊕` or `**`).
    pub symbol: String,
    /// Symbol it binds to (for example `core::math::pow`).
    pub binds: QualifiedName,
    /// Fixity class.
    pub fixity: Fixity,
    /// Precedence, `None` = import/precedence default.
    pub precedence: Option<u8>,
    /// Section that declared it (`operators`, `op`, `op` in `notation`).
    pub provenance: String,
}

/// Deterministic canonical token for schema/identity continuity
/// (`schema mutation moves identity`).
#[must_use]
pub fn canonical_operator(operator: &DeclaredOperator) -> String {
    format!(
        "op:{}:{}:{}:{}",
        operator.symbol,
        operator.binds.0,
        operator.fixity.as_str(),
        operator
            .precedence
            .map_or_else(|| "-".to_string(), |p| p.to_string())
    )
}
