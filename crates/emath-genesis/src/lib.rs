#![forbid(unsafe_code)]

//! Minimal Semantic Genesis evaluator and built-in example worlds.

use std::collections::BTreeMap;
use std::fmt;

use emath_term::{Signature, SymbolId, Term, VariableId};
use emath_world_ir::{
    CarrierDef, Fixity, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WorldIr,
};

/// Environment for free variables.
pub type Environment<V> = BTreeMap<VariableId, V>;

/// Generic first-order world implementation.
pub trait FirstOrderWorld {
    /// Runtime value.
    type Value: Clone;
    /// Evaluation error.
    type Error;

    /// Resolves a nullary symbol.
    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error>;

    /// Applies an operator to evaluated arguments.
    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}

/// Evaluation error shared by the seed worlds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// Missing free-variable valuation.
    MissingVariable(VariableId),
    /// Unknown symbol.
    UnknownSymbol(SymbolId),
    /// Incorrect runtime arity.
    Arity {
        /// Symbol.
        symbol: SymbolId,
        /// Expected arity.
        expected: usize,
        /// Actual arity.
        actual: usize,
    },
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EvalError {}

/// Evaluates a term in any world implementing its symbols.
pub fn evaluate<W: FirstOrderWorld>(
    term: &Term,
    world: &W,
    environment: &Environment<W::Value>,
) -> Result<W::Value, W::Error>
where
    W::Error: From<EvalError>,
{
    match term {
        Term::Variable(variable) => environment
            .get(variable)
            .cloned()
            .ok_or_else(|| EvalError::MissingVariable(variable.clone()).into()),
        Term::Constant(symbol) => world.constant(symbol),
        Term::Apply {
            operator,
            arguments,
        } => {
            let values = arguments
                .iter()
                .map(|argument| evaluate(argument, world, environment))
                .collect::<Result<Vec<_>, _>>()?;
            world.apply(operator, values)
        }
    }
}

/// Returns a provider-neutral free symbolic World IR.
#[must_use]
pub fn free_symbolic_world(name: &str, signature: Signature) -> WorldIr {
    let symbols = signature
        .iter()
        .map(|(symbol, arity)| SymbolDef {
            id: symbol.clone(),
            display: symbol.0.clone(),
            fixity: if *arity == 0 {
                Fixity::Constant
            } else {
                Fixity::Function
            },
            precedence: None,
            type_scheme: format!("Term^{arity} -> Term"),
        })
        .collect::<Vec<_>>();
    let operators = signature
        .iter()
        .map(|(symbol, _)| OperatorDef {
            symbol: symbol.clone(),
            semantics: OperatorSemantics::StructuralConstructor,
            origin: MeaningOrigin::Derived,
        })
        .collect::<Vec<_>>();
    WorldIr {
        version: 1,
        name: name.into(),
        signature,
        carriers: vec![CarrierDef {
            name: "Term".into(),
            type_expression: "FreeTerm".into(),
        }],
        symbols,
        operators,
        constructors: vec!["Term::Variable/Constant/Apply".into()],
        laws: vec!["structural-totality".into()],
        holes: vec![],
        capabilities: vec!["pure".into()],
    }
}

/// A free symbolic world whose values remain terms.
#[derive(Debug, Default, Clone, Copy)]
pub struct FreeTermWorld;

impl FirstOrderWorld for FreeTermWorld {
    type Value = Term;
    type Error = EvalError;

    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        Ok(Term::Constant(symbol.clone()))
    }

    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Term::Apply {
            operator: operator.clone(),
            arguments,
        })
    }
}

/// Boolean interpretation for the reference alien signature.
#[derive(Debug, Default, Clone, Copy)]
pub struct BooleanAlienWorld;

impl FirstOrderWorld for BooleanAlienWorld {
    type Value = bool;
    type Error = EvalError;

    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        match symbol.0.as_str() {
            "ζ" => Ok(true),
            _ => Err(EvalError::UnknownSymbol(symbol.clone())),
        }
    }

    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        match (operator.0.as_str(), arguments.as_slice()) {
            ("⋈", [left, right]) => Ok(*left ^ *right),
            ("⧖", [value]) => Ok(!*value),
            ("⊛", [left, right]) => Ok(*left && *right),
            ("⋈" | "⊛", values) => Err(EvalError::Arity {
                symbol: operator.clone(),
                expected: 2,
                actual: values.len(),
            }),
            ("⧖", values) => Err(EvalError::Arity {
                symbol: operator.clone(),
                expected: 1,
                actual: values.len(),
            }),
            _ => Err(EvalError::UnknownSymbol(operator.clone())),
        }
    }
}

/// Modular-17 interpretation for the reference alien signature.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularAlienWorld;

impl FirstOrderWorld for ModularAlienWorld {
    type Value = i64;
    type Error = EvalError;

    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        match symbol.0.as_str() {
            "ζ" => Ok(3),
            _ => Err(EvalError::UnknownSymbol(symbol.clone())),
        }
    }

    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        let modulo = |value: i64| value.rem_euclid(17);
        match (operator.0.as_str(), arguments.as_slice()) {
            ("⋈", [left, right]) => Ok(modulo(*left + *right)),
            ("⧖", [value]) => Ok(modulo(*value * *value)),
            ("⊛", [left, right]) => Ok(modulo(*left * *right)),
            ("⋈" | "⊛", values) => Err(EvalError::Arity {
                symbol: operator.clone(),
                expected: 2,
                actual: values.len(),
            }),
            ("⧖", values) => Err(EvalError::Arity {
                symbol: operator.clone(),
                expected: 1,
                actual: values.len(),
            }),
            _ => Err(EvalError::UnknownSymbol(operator.clone())),
        }
    }
}

/// Constructs the reference term and signature.
#[must_use]
pub fn reference_alien_term() -> (Signature, Term) {
    let mut signature = Signature::default();
    signature.insert(SymbolId("⧖".into()), 1).unwrap();
    signature.insert(SymbolId("⋈".into()), 2).unwrap();
    signature.insert(SymbolId("⊛".into()), 2).unwrap();
    signature.insert(SymbolId("ζ".into()), 0).unwrap();

    let joined = Term::Apply {
        operator: SymbolId("⋈".into()),
        arguments: vec![
            Term::Variable(VariableId("a".into())),
            Term::Variable(VariableId("b".into())),
        ],
    };
    let transformed = Term::Apply {
        operator: SymbolId("⧖".into()),
        arguments: vec![joined],
    };
    let term = Term::Apply {
        operator: SymbolId("⊛".into()),
        arguments: vec![transformed, Term::Constant(SymbolId("ζ".into()))],
    };
    (signature, term)
}
