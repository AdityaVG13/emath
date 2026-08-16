//! Declared operator surface ( core declarations).
//!
//! An operator declaration attaches a semantics name (and optional
//! fixity) to an existing symbol at the SIR level. Fixity normalization:
//! `infixl`, `infixr` and `infix` keywords all map to `Infix`
//! (associativity is resolved by parentheses in the line-oriented
//! syntax); only `prefix` and `postfix` change arity placement.

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

#[cfg(test)]
mod tests {
    use super::*;

    fn plus() -> DeclaredOperator {
        DeclaredOperator {
            symbol: "+".into(),
            binds: QualifiedName("core::math::add".into()),
            fixity: Fixity::Infix,
            precedence: None,
            provenance: "operators".into(),
        }
    }

    #[test]
    fn op_canonical_is_stable() {
        assert_eq!(canonical_operator(&plus()), "op:+:core::math::add:infix:-");
        assert_eq!(canonical_operator(&plus()), canonical_operator(&plus()));
    }

    #[test]
    fn precedence_and_symbol_change_identity() {
        let mut high = plus();
        high.precedence = Some(11);
        let mut star = plus();
        star.symbol = "*".into();
        assert_ne!(canonical_operator(&plus()), canonical_operator(&high));
        assert_ne!(canonical_operator(&plus()), canonical_operator(&star));
    }
}
