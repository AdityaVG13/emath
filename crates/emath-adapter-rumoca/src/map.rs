//!: semantic mapping table.
//!
//! Every Modelica/Rumoca construct emath may encounter is classified as
//! exact, refinement, lossy, unsupported or presentation-only, with a
//! stable reason. Deterministic static table, provider-free.

/// Mapping classification for a construct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MappingClass {
    /// Semantics transfer without qualification.
    Exact,
    /// Emath expresses an equivalent-but-narrower meaning.
    Refinement,
    /// Semantics transfer loses information.
    Lossy,
    /// No emath meaning exists yet.
    Unsupported,
    /// Presentation/annotation only, no semantics.
    PresentationOnly,
}

/// One row of the semantic mapping table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstructMapping {
    /// Modelica/Rumoca construct name.
    pub construct: &'static str,
    /// Classification.
    pub class: MappingClass,
    /// Stable reason.
    pub reason: &'static str,
}

/// Deterministic, sorted semantic mapping table.
pub const TABLE: [ConstructMapping; 16] = [
    ConstructMapping {
        construct: "annotation",
        class: MappingClass::PresentationOnly,
        reason: "no runtime semantics",
    },
    ConstructMapping {
        construct: "connect",
        class: MappingClass::Exact,
        reason: "maps to connection edges",
    },
    ConstructMapping {
        construct: "connector",
        class: MappingClass::Exact,
        reason: "maps to connection port components",
    },
    ConstructMapping {
        construct: "der",
        class: MappingClass::Exact,
        reason: "maps to derivative equations",
    },
    ConstructMapping {
        construct: "equation",
        class: MappingClass::Exact,
        reason: "maps to structural equations",
    },
    ConstructMapping {
        construct: "if",
        class: MappingClass::Refinement,
        reason: "parametric conditional, evaluated by plan context",
    },
    ConstructMapping {
        construct: "initial equation",
        class: MappingClass::Exact,
        reason: "maps to initial conditions",
    },
    ConstructMapping {
        construct: "inner",
        class: MappingClass::Unsupported,
        reason: "inheritance/visibility context outside subset",
    },
    ConstructMapping {
        construct: "noEvent",
        class: MappingClass::Lossy,
        reason: "event suppression hint not representable",
    },
    ConstructMapping {
        construct: "outer",
        class: MappingClass::Unsupported,
        reason: "inheritance/visibility context outside subset",
    },
    ConstructMapping {
        construct: "parameter",
        class: MappingClass::Exact,
        reason: "maps to model parameters",
    },
    ConstructMapping {
        construct: "record",
        class: MappingClass::Refinement,
        reason: "canonical constructor admits only valid-state records",
    },
    ConstructMapping {
        construct: "reinit",
        class: MappingClass::Lossy,
        reason: "state re-initialization becomes a parametric event",
    },
    ConstructMapping {
        construct: "replaceable",
        class: MappingClass::PresentationOnly,
        reason: "variability annotation, resolved by adapter",
    },
    ConstructMapping {
        construct: "sample",
        class: MappingClass::Unsupported,
        reason: "time-sampled dynamics outside Phase 1 subset",
    },
    ConstructMapping {
        construct: "when",
        class: MappingClass::Refinement,
        reason: "maps to basic continuous events only",
    },
];

/// Looks up a construct mapping by name.
#[must_use]
pub fn classify(construct: &str) -> Option<&'static ConstructMapping> {
    let mut index = 0;
    while index < TABLE.len() {
        if TABLE[index].construct == construct {
            return Some(&TABLE[index]);
        }
        index += 1;
    }
    None
}

/// The mapping table in deterministic order.
#[must_use]
pub fn table() -> &'static [ConstructMapping] {
    &TABLE
}
