//! Built-in candidate-world providers: at least five world
//! classes with deterministic identities, provider-neutral and
//! emath-owned.

use crate::{
    CarrierDef, Fixity, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WorldId, WorldIr,
};
use emath_term::{Signature, SymbolId};

/// Built-in world classes (five or more classes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorldClass {
    /// Free term algebra over a signature; no laws.
    FreeTerm,
    /// Finite set carrier with finite-table operators.
    FiniteTable,
    /// Commutative monoid with declared laws.
    CommutativeMonoid,
    /// Boolean lattice with declared laws.
    BooleanLattice,
    /// Integer ring with declared laws.
    IntegerRing,
    /// Cyclic group Z/3 with declared laws.
    CyclicGroup,
    /// Square matrices with non-commutative multiplication.
    Matrix,
    /// Finite directed graphs under union.
    Graph,
}

impl WorldClass {
    /// All built-in classes in stable order.
    pub const ALL: [Self; 8] = [
        Self::FreeTerm,
        Self::FiniteTable,
        Self::CommutativeMonoid,
        Self::BooleanLattice,
        Self::IntegerRing,
        Self::CyclicGroup,
        Self::Matrix,
        Self::Graph,
    ];

    /// Deterministic canonical name.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::FreeTerm => "free-term",
            Self::FiniteTable => "finite-table",
            Self::CommutativeMonoid => "commutative-monoid",
            Self::BooleanLattice => "boolean-lattice",
            Self::IntegerRing => "integer-ring",
            Self::CyclicGroup => "cyclic-group",
            Self::Matrix => "matrix",
            Self::Graph => "graph",
        }
    }
}

/// A built-in candidate world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinWorld {
    /// World class.
    pub class: WorldClass,
    /// The world itself.
    pub world: WorldIr,
}

impl BuiltinWorld {
    /// Deterministic world identity (content-bound, not source order).
    #[must_use]
    pub fn identity(&self) -> WorldId {
        self.world.identity()
    }
}

/// The built-in candidate-world provider: builds all built-in world
/// classes. Deterministic by construction; identities are content-bound
/// and input-order independent.
#[must_use]
pub fn builtin_worlds() -> Vec<BuiltinWorld> {
    vec![
        free_term_world(),
        finite_table_world(),
        commutative_monoid_world(),
        boolean_lattice_world(),
        integer_ring_world(),
        cyclic_group_world(),
        matrix_world(),
        graph_world(),
    ]
}

fn symbol(id: &str, fixity: Fixity, precedence: Option<u16>, type_scheme: &str) -> SymbolDef {
    SymbolDef {
        id: SymbolId(id.to_string()),
        display: id.to_string(),
        fixity,
        precedence,
        type_scheme: type_scheme.to_string(),
    }
}

fn variable_signature() -> Signature {
    // Seed signature: a single monomorphic sort with the world's surface
    // symbols; worlds declare semantics through operators and laws.
    let mut signature = Signature::default();
    for id in [
        "ζ", "⋈", "⊙", "∧", "∨", "¬", "⊤", "⊥", "+", "×", "-", "⊕", "⊖", "0", "1", "2", "⊞", "⊠",
        "I", "O", "∪", "ø",
    ] {
        signature
            .insert(SymbolId(id.to_string()), arity(id))
            .unwrap();
    }
    signature
}

fn arity(id: &str) -> usize {
    match id {
        "ζ" | "⊤" | "⊥" | "0" | "1" | "2" | "I" | "O" | "ø" => 0,
        "¬" | "-" | "⊖" => 1,
        _ => 2,
    }
}

fn free_term_world() -> BuiltinWorld {
    let world = WorldIr {
        version: 1,
        name: "free-term".to_string(),
        signature: variable_signature(),
        carriers: vec![CarrierDef {
            name: "Term".to_string(),
            type_expression: "inductive Term".to_string(),
        }],
        symbols: vec![
            symbol("ζ", Fixity::Constant, None, "Term"),
            symbol("⋈", Fixity::Infix, Some(50), "Term × Term → Term"),
        ],
        operators: vec![
            OperatorDef {
                symbol: SymbolId("ζ".to_string()),
                semantics: OperatorSemantics::StructuralConstructor,
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("⋈".to_string()),
                semantics: OperatorSemantics::StructuralConstructor,
                origin: MeaningOrigin::Declared,
            },
        ],
        constructors: vec!["free term mode: no reduction laws".to_string()],
        laws: vec![],
        effects: vec![],
        holes: vec![],
        capabilities: vec!["term".to_string()],
    };
    BuiltinWorld {
        class: WorldClass::FreeTerm,
        world,
    }
}

fn finite_table_world() -> BuiltinWorld {
    let world = WorldIr {
        version: 1,
        name: "finite-table".to_string(),
        signature: variable_signature(),
        carriers: vec![CarrierDef {
            name: "Fin3".to_string(),
            type_expression: "finite carrier {0,1,2}".to_string(),
        }],
        symbols: vec![
            symbol("0", Fixity::Constant, None, "Fin3"),
            symbol("1", Fixity::Constant, None, "Fin3"),
            symbol("2", Fixity::Constant, None, "Fin3"),
            symbol("⊙", Fixity::Infix, Some(50), "Fin3 × Fin3 → Fin3"),
        ],
        operators: vec![
            OperatorDef {
                symbol: SymbolId("0".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("0".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("1".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("1".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("2".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("2".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("⊙".to_string()),
                semantics: OperatorSemantics::FiniteTable(vec![
                    "0,0→0".to_string(),
                    "0,1→1".to_string(),
                    "0,2→2".to_string(),
                    "1,0→1".to_string(),
                    "1,1→2".to_string(),
                    "1,2→0".to_string(),
                    "2,0→2".to_string(),
                    "2,1→0".to_string(),
                    "2,2→1".to_string(),
                ]),
                origin: MeaningOrigin::Declared,
            },
        ],
        constructors: vec!["Fin3.new(x: 0|1|2) -> Fin3".to_string()],
        laws: vec!["forall x y z. x ⊙ (y ⊙ z) == (x ⊙ y) ⊙ z".to_string()],
        effects: vec![],
        holes: vec![],
        capabilities: vec!["finite".to_string(), "table".to_string()],
    };
    BuiltinWorld {
        class: WorldClass::FiniteTable,
        world,
    }
}

fn commutative_monoid_world() -> BuiltinWorld {
    let world = WorldIr {
        version: 1,
        name: "commutative-monoid".to_string(),
        signature: variable_signature(),
        carriers: vec![CarrierDef {
            name: "Nat".to_string(),
            type_expression: "natural numbers".to_string(),
        }],
        symbols: vec![
            symbol("ζ", Fixity::Constant, None, "Nat"),
            symbol("⋈", Fixity::Infix, Some(50), "Nat × Nat → Nat"),
        ],
        operators: vec![
            OperatorDef {
                symbol: SymbolId("ζ".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("0".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("⋈".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("x + y".to_string()),
                origin: MeaningOrigin::Declared,
            },
        ],
        constructors: vec!["Nat.zero() -> Nat".to_string()],
        laws: vec![
            "forall x. x ⋈ ζ == x".to_string(),
            "forall x. ζ ⋈ x == x".to_string(),
            "forall x y. x ⋈ y == y ⋈ x".to_string(),
            "forall x y z. (x ⋈ y) ⋈ z == x ⋈ (y ⋈ z)".to_string(),
        ],
        effects: vec![],
        holes: vec![],
        capabilities: vec!["commutative".to_string(), "associative".to_string()],
    };
    BuiltinWorld {
        class: WorldClass::CommutativeMonoid,
        world,
    }
}

fn boolean_lattice_world() -> BuiltinWorld {
    let world = WorldIr {
        version: 1,
        name: "boolean-lattice".to_string(),
        signature: variable_signature(),
        carriers: vec![CarrierDef {
            name: "Bool".to_string(),
            type_expression: "boolean lattice".to_string(),
        }],
        symbols: vec![
            symbol("⊤", Fixity::Constant, None, "Bool"),
            symbol("⊥", Fixity::Constant, None, "Bool"),
            symbol("∧", Fixity::Infix, Some(60), "Bool × Bool → Bool"),
            symbol("∨", Fixity::Infix, Some(40), "Bool × Bool → Bool"),
            symbol("¬", Fixity::Prefix, Some(70), "Bool → Bool"),
        ],
        operators: vec![
            OperatorDef {
                symbol: SymbolId("⊤".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("true".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("⊥".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("false".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("∧".to_string()),
                semantics: OperatorSemantics::FiniteTable(vec![
                    "false,false→false".to_string(),
                    "false,true→false".to_string(),
                    "true,false→false".to_string(),
                    "true,true→true".to_string(),
                ]),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("∨".to_string()),
                semantics: OperatorSemantics::FiniteTable(vec![
                    "false,false→false".to_string(),
                    "false,true→true".to_string(),
                    "true,false→true".to_string(),
                    "true,true→true".to_string(),
                ]),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("¬".to_string()),
                semantics: OperatorSemantics::FiniteTable(vec![
                    "false→true".to_string(),
                    "true→false".to_string(),
                ]),
                origin: MeaningOrigin::Declared,
            },
        ],
        constructors: vec![
            "Bool.true() -> Bool".to_string(),
            "Bool.false() -> Bool".to_string(),
        ],
        laws: vec![
            "forall x. x ∧ x == x".to_string(),
            "forall x. x ∨ x == x".to_string(),
            "forall x y. x ∧ y == y ∧ x".to_string(),
            "forall x y. x ∨ y == y ∨ x".to_string(),
            "forall x. ¬¬x == x".to_string(),
            "forall x. x ∧ ⊥ == ⊥".to_string(),
            "forall x. x ∨ ⊤ == ⊤".to_string(),
        ],
        effects: vec![],
        holes: vec![],
        capabilities: vec!["lattice".to_string(), "idempotent".to_string()],
    };
    BuiltinWorld {
        class: WorldClass::BooleanLattice,
        world,
    }
}

fn integer_ring_world() -> BuiltinWorld {
    let world = WorldIr {
        version: 1,
        name: "integer-ring".to_string(),
        signature: variable_signature(),
        carriers: vec![CarrierDef {
            name: "Int".to_string(),
            type_expression: "integers".to_string(),
        }],
        symbols: vec![
            symbol("0", Fixity::Constant, None, "Int"),
            symbol("1", Fixity::Constant, None, "Int"),
            symbol("+", Fixity::Infix, Some(50), "Int × Int → Int"),
            symbol("×", Fixity::Infix, Some(60), "Int × Int → Int"),
            symbol("-", Fixity::Prefix, Some(70), "Int → Int"),
        ],
        operators: vec![
            OperatorDef {
                symbol: SymbolId("0".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("0".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("1".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("1".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("+".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("x + y".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("×".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("x * y".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("-".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("-x".to_string()),
                origin: MeaningOrigin::Declared,
            },
        ],
        constructors: vec![
            "Int.of_i64(x: i64) -> Int".to_string(),
            "Int.add(a: Int, b: Int) -> Result<Int, Overflow>".to_string(),
        ],
        laws: vec![
            "forall x. x + 0 == x".to_string(),
            "forall x. x × 1 == x".to_string(),
            "forall x y. x + y == y + x".to_string(),
            "forall x y z. (x + y) + z == x + (y + z)".to_string(),
            "forall x. x + (-x) == 0".to_string(),
            "forall x y z. x × (y + z) == (x × y) + (x × z)".to_string(),
        ],
        effects: vec![],
        holes: vec![],
        capabilities: vec!["ring".to_string(), "distributive".to_string()],
    };
    BuiltinWorld {
        class: WorldClass::IntegerRing,
        world,
    }
}

fn cyclic_group_world() -> BuiltinWorld {
    let world = WorldIr {
        version: 1,
        name: "cyclic-group-z3".to_string(),
        signature: variable_signature(),
        carriers: vec![CarrierDef {
            name: "Z3".to_string(),
            type_expression: "integers modulo 3".to_string(),
        }],
        symbols: vec![
            symbol("0", Fixity::Constant, None, "Z3"),
            symbol("1", Fixity::Constant, None, "Z3"),
            symbol("2", Fixity::Constant, None, "Z3"),
            symbol("⊕", Fixity::Infix, Some(50), "Z3 × Z3 → Z3"),
            symbol("⊖", Fixity::Prefix, Some(70), "Z3 → Z3"),
        ],
        operators: vec![
            OperatorDef {
                symbol: SymbolId("0".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("0".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("1".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("1".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("2".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("2".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("⊕".to_string()),
                semantics: OperatorSemantics::FiniteTable(vec![
                    "0,0→0".to_string(),
                    "0,1→1".to_string(),
                    "0,2→2".to_string(),
                    "1,0→1".to_string(),
                    "1,1→2".to_string(),
                    "1,2→0".to_string(),
                    "2,0→2".to_string(),
                    "2,1→0".to_string(),
                    "2,2→1".to_string(),
                ]),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("⊖".to_string()),
                semantics: OperatorSemantics::FiniteTable(vec![
                    "0→0".to_string(),
                    "1→2".to_string(),
                    "2→1".to_string(),
                ]),
                origin: MeaningOrigin::Declared,
            },
        ],
        constructors: vec!["Z3.from_residue(x: 0|1|2) -> Z3".to_string()],
        laws: vec![
            "forall x. x ⊕ 0 == x".to_string(),
            "forall x. 0 ⊕ x == x".to_string(),
            "forall x y. x ⊕ y == y ⊕ x".to_string(),
            "forall x. x ⊕ (⊖ x) == 0".to_string(),
            "forall x y z. (x ⊕ y) ⊕ z == x ⊕ (y ⊕ z)".to_string(),
        ],
        effects: vec![],
        holes: vec![],
        capabilities: vec!["group".to_string(), "cyclic".to_string()],
    };
    BuiltinWorld {
        class: WorldClass::CyclicGroup,
        world,
    }
}

fn matrix_world() -> BuiltinWorld {
    let world = WorldIr {
        version: 1,
        name: "matrix-2x2".to_string(),
        signature: variable_signature(),
        carriers: vec![CarrierDef {
            name: "Mat2".to_string(),
            type_expression: "2×2 integer matrices".to_string(),
        }],
        symbols: vec![
            symbol("O", Fixity::Constant, None, "Mat2"),
            symbol("I", Fixity::Constant, None, "Mat2"),
            symbol("⊞", Fixity::Infix, Some(50), "Mat2 × Mat2 → Mat2"),
            symbol("⊠", Fixity::Infix, Some(60), "Mat2 × Mat2 → Mat2"),
        ],
        operators: vec![
            OperatorDef {
                symbol: SymbolId("O".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("[[0,0],[0,0]]".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("I".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("[[1,0],[0,1]]".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("⊞".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("entrywise addition".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("⊠".to_string()),
                semantics: OperatorSemantics::DeclaredExpression(
                    "matrix multiplication".to_string(),
                ),
                origin: MeaningOrigin::Declared,
            },
        ],
        constructors: vec!["Mat2.new(a: Int, b: Int, c: Int, d: Int) -> Mat2".to_string()],
        // Deliberately no ⊠-commutativity law: matrix multiplication is
        // not commutative, and claiming it would be a wrong law the
        // finite checker must be able to refute on a counterexample.
        laws: vec![
            "forall a. a ⊞ O == a".to_string(),
            "forall a b. a ⊞ b == b ⊞ a".to_string(),
            "forall a. a ⊠ I == a".to_string(),
            "forall a. I ⊠ a == a".to_string(),
            "forall a b c. (a ⊠ b) ⊠ c == a ⊠ (b ⊠ c)".to_string(),
            "forall a b c. a ⊠ (b ⊞ c) == (a ⊠ b) ⊞ (a ⊠ c)".to_string(),
        ],
        effects: vec![],
        holes: vec![],
        capabilities: vec!["matrix".to_string(), "noncommutative".to_string()],
    };
    BuiltinWorld {
        class: WorldClass::Matrix,
        world,
    }
}

fn graph_world() -> BuiltinWorld {
    let world = WorldIr {
        version: 1,
        name: "graph-union".to_string(),
        signature: variable_signature(),
        carriers: vec![CarrierDef {
            name: "Graph".to_string(),
            type_expression: "finite directed graphs".to_string(),
        }],
        symbols: vec![
            symbol("ø", Fixity::Constant, None, "Graph"),
            symbol("∪", Fixity::Infix, Some(40), "Graph × Graph → Graph"),
        ],
        operators: vec![
            OperatorDef {
                symbol: SymbolId("ø".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("empty graph".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("∪".to_string()),
                semantics: OperatorSemantics::DeclaredExpression(
                    "vertex and edge set union".to_string(),
                ),
                origin: MeaningOrigin::Declared,
            },
        ],
        constructors: vec![
            "Graph.empty() -> Graph".to_string(),
            "Graph.edge(from: Vertex, to: Vertex) -> Graph".to_string(),
        ],
        laws: vec![
            "forall g. g ∪ ø == g".to_string(),
            "forall g. g ∪ g == g".to_string(),
            "forall g h. g ∪ h == h ∪ g".to_string(),
            "forall g h k. (g ∪ h) ∪ k == g ∪ (h ∪ k)".to_string(),
        ],
        effects: vec![],
        holes: vec![],
        capabilities: vec!["graph".to_string(), "idempotent".to_string()],
    };
    BuiltinWorld {
        class: WorldClass::Graph,
        world,
    }
}
