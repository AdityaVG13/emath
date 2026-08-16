#![forbid(unsafe_code)]

//! Provider-neutral World IR and meaning-hole structures.

pub mod builtin;
pub mod translation;

use emath_term::{Signature, SymbolId};

/// Content identity placeholder for an admitted world.
///
/// Production emath should replace the demonstration FNV identity with its
/// canonical cryptographic identity service while preserving the semantic domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorldId(pub u64);

/// A carrier/domain declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarrierDef {
    /// Stable local name.
    pub name: String,
    /// Canonical type description in the seed representation.
    pub type_expression: String,
}

/// Surface fixity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixity {
    /// Nullary symbol.
    Constant,
    /// Prefix operator.
    Prefix,
    /// Infix operator.
    Infix,
    /// Postfix operator.
    Postfix,
    /// Function-style application.
    Function,
}

/// Stable symbol declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDef {
    /// Stable symbol ID.
    pub id: SymbolId,
    /// Display glyph or name.
    pub display: String,
    /// Surface fixity.
    pub fixity: Fixity,
    /// Optional precedence.
    pub precedence: Option<u16>,
    /// Canonical type scheme.
    pub type_scheme: String,
}

/// Origin of an operator meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeaningOrigin {
    /// Authored explicitly.
    Declared,
    /// Imported from a library.
    Imported,
    /// Derived mechanically.
    Derived,
    /// Inferred from constraints.
    Inferred,
    /// Synthesized by a search provider.
    Synthesized,
    /// Fitted from examples or observations.
    Fitted,
    /// Proposed by an agent.
    AgentProposed,
    /// Produced by evolutionary search.
    Evolutionary,
    /// Selected explicitly by a user.
    UserSelected,
}

/// Executable or open operator semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorSemantics {
    /// A structural term constructor.
    StructuralConstructor,
    /// A canonical expression understood by another emath-owned IR.
    DeclaredExpression(String),
    /// A finite lookup table encoded canonically.
    FiniteTable(Vec<String>),
    /// A provider binding identified without leaking provider-native types.
    ProviderBinding(String),
    /// A synthesized program and receipt ID.
    Synthesized { program: String, receipt: String },
    /// An unresolved meaning hole.
    Parametric(MeaningHoleId),
}

/// Operator declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorDef {
    /// Symbol being interpreted.
    pub symbol: SymbolId,
    /// Semantics.
    pub semantics: OperatorSemantics,
    /// Meaning origin.
    pub origin: MeaningOrigin,
}

/// Stable meaning-hole ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeaningHoleId(pub u64);

/// Hole category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeaningHoleKind {
    /// Surface fixity.
    Fixity,
    /// Precedence.
    Precedence,
    /// Arity.
    Arity,
    /// Carrier/domain.
    Carrier,
    /// Type.
    Type,
    /// Operator implementation.
    OperatorDefinition,
    /// Constant implementation.
    ConstantDefinition,
    /// Constructor.
    Constructor,
    /// Law.
    Law,
    /// Variable value.
    VariableValue,
    /// Goal.
    Goal,
    /// Provider.
    Provider,
    /// Evidence.
    Evidence,
}

/// Hole lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeaningHoleState {
    /// No accepted proposal.
    Open,
    /// At least one unchecked proposal.
    Proposed,
    /// Exactly one admitted solution selected by policy.
    Solved,
    /// Several admitted solutions remain.
    Ambiguous,
    /// Constraints are contradictory.
    Contradictory,
    /// Work is intentionally postponed.
    Deferred,
    /// Budget ended before resolution.
    BudgetExhausted,
}

/// Explicit unresolved semantic requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeaningHole {
    /// Stable ID.
    pub id: MeaningHoleId,
    /// Hole category.
    pub kind: MeaningHoleKind,
    /// Canonical constraints.
    pub constraints: Vec<String>,
    /// Current state.
    pub state: MeaningHoleState,
}

/// Provider-neutral mathematical world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldIr {
    /// World schema version.
    pub version: u32,
    /// Human-readable name.
    pub name: String,
    /// First-order signature.
    pub signature: Signature,
    /// Carrier definitions.
    pub carriers: Vec<CarrierDef>,
    /// Symbols.
    pub symbols: Vec<SymbolDef>,
    /// Operator meanings.
    pub operators: Vec<OperatorDef>,
    /// Canonical constructor contracts.
    pub constructors: Vec<String>,
    /// Canonical laws.
    pub laws: Vec<String>,
    /// Open holes.
    pub holes: Vec<MeaningHole>,
    /// Declared capability names.
    pub capabilities: Vec<String>,
}

impl WorldIr {
    /// Computes a deterministic seed identity over canonical content.
    #[must_use]
    pub fn identity(&self) -> WorldId {
        WorldId(fnv1a64(self.canonical().as_bytes()))
    }

    /// Renders a deterministic seed canonical form.
    ///
    /// The name is a display-only alias and is excluded from content
    /// identity (spec 07): a `WorldId` binds semantic content, not
    /// incidental labels.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut carriers = self.carriers.clone();
        carriers.sort_by(|a, b| a.name.cmp(&b.name));
        let mut symbols = self.symbols.clone();
        symbols.sort_by(|a, b| a.id.cmp(&b.id));
        let mut operators = self.operators.clone();
        operators.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        let mut laws = self.laws.clone();
        laws.sort();
        let mut capabilities = self.capabilities.clone();
        capabilities.sort();
        format!(
            "world:v{}:{carriers:?}:{symbols:?}:{operators:?}:{:?}:{laws:?}:{:?}:{capabilities:?}",
            self.version, self.constructors, self.holes
        )
    }
}

/// Deterministic seed FNV-1a64 content identity (replaced by the canonical
/// cryptographic identity service before stable publication).
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_mutation_moves_world_id() {
        let signature = Signature::default();
        let base = WorldIr {
            version: 1,
            name: "Example".into(),
            signature,
            carriers: vec![CarrierDef {
                name: "Element".into(),
                type_expression: "Bool".into(),
            }],
            symbols: vec![],
            operators: vec![],
            constructors: vec![],
            laws: vec!["total".into()],
            holes: vec![],
            capabilities: vec!["pure".into()],
        };
        let mut changed = base.clone();
        changed.laws.push("nontrivial".into());
        assert_ne!(base.identity(), changed.identity());
    }
}
