use emath_term::{Signature, SymbolId};
use emath_world_ir::{
    CarrierDef, Fixity, MeaningHole, MeaningHoleId, MeaningHoleKind, MeaningHoleState,
    MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WORLD_IR_VERSION, WorldIr,
};

type MutationRow = (&'static str, Box<dyn Fn(&mut WorldIr)>);

fn reference_world() -> WorldIr {
    let mut signature = Signature::default();
    signature.insert(SymbolId("⋈".to_string()), 2).unwrap();
    signature.insert(SymbolId("ζ".to_string()), 0).unwrap();
    WorldIr {
        version: WORLD_IR_VERSION,
        name: "reference".to_string(),
        signature,
        carriers: vec![CarrierDef {
            name: "C".to_string(),
            type_expression: "carrier".to_string(),
        }],
        symbols: vec![SymbolDef {
            id: SymbolId("⋈".to_string()),
            display: "⋈".to_string(),
            fixity: Fixity::Infix,
            precedence: Some(50),
            type_scheme: "C × C → C".to_string(),
        }],
        operators: vec![OperatorDef {
            symbol: SymbolId("⋈".to_string()),
            semantics: OperatorSemantics::DeclaredExpression("x + y".to_string()),
            origin: MeaningOrigin::Declared,
        }],
        constructors: vec!["C.new() -> C".to_string()],
        laws: vec!["forall x y. x ⋈ y == y ⋈ x".to_string()],
        effects: vec![],
        holes: vec![MeaningHole {
            id: MeaningHoleId(1),
            kind: MeaningHoleKind::Law,
            constraints: vec!["associativity unproven".to_string()],
            state: MeaningHoleState::Open,
        }],
        capabilities: vec!["commutative".to_string()],
    }
}

/// World IR mutation matrix: every semantic component participates
/// in identity (mutating it yields a new `WorldId`), and
/// presentation-only fields (name, symbol display) do not. One row
/// per component so a missed field fails by name.
#[test]
fn semantic_mutations_change_identity_and_presentation_does_not() {
    let base = reference_world();
    let base_id = base.identity();

    let semantic: Vec<MutationRow> = vec![
        ("version", Box::new(|w| w.version += 1)),
        (
            "signature",
            Box::new(|w| {
                w.signature.insert(SymbolId("★".to_string()), 1).unwrap();
            }),
        ),
        (
            "carriers",
            Box::new(|w| w.carriers[0].type_expression = "other carrier".to_string()),
        ),
        (
            "symbols/fixity",
            Box::new(|w| w.symbols[0].fixity = Fixity::Function),
        ),
        (
            "symbols/precedence",
            Box::new(|w| w.symbols[0].precedence = Some(60)),
        ),
        (
            "symbols/type_scheme",
            Box::new(|w| w.symbols[0].type_scheme = "C → C".to_string()),
        ),
        (
            "operators/semantics",
            Box::new(|w| {
                w.operators[0].semantics =
                    OperatorSemantics::DeclaredExpression("x * y".to_string());
            }),
        ),
        (
            "constructors",
            Box::new(|w| w.constructors.push("C.other() -> C".to_string())),
        ),
        (
            "laws",
            Box::new(|w| w.laws.push("forall x. x ⋈ x == x".to_string())),
        ),
        ("effects", Box::new(|w| w.effects.push("io".to_string()))),
        (
            "holes/state",
            Box::new(|w| w.holes[0].state = MeaningHoleState::Solved),
        ),
        (
            "capabilities",
            Box::new(|w| w.capabilities.push("associative".to_string())),
        ),
    ];
    for (component, mutate) in semantic {
        let mut mutated = base.clone();
        mutate(&mut mutated);
        assert_ne!(
            mutated.identity(),
            base_id,
            "semantic mutation of `{component}` must change WorldId"
        );
    }

    let presentation: Vec<MutationRow> = vec![
        ("name", Box::new(|w| w.name = "alias".to_string())),
        (
            "symbols/display",
            Box::new(|w| w.symbols[0].display = "join".to_string()),
        ),
    ];
    for (component, mutate) in presentation {
        let mut mutated = base.clone();
        mutate(&mut mutated);
        assert_eq!(
            mutated.identity(),
            base_id,
            "presentation-only mutation of `{component}` must not change WorldId"
        );
    }
}

/// Canonicalization is input-order independent: permuting the vector
/// fields yields the same canonical form and identity.
#[test]
fn canonical_form_is_input_order_independent() {
    let mut base = reference_world();
    base.laws.push("forall x. x ⋈ ζ == x".to_string());
    base.capabilities.push("monoid".to_string());
    base.effects.push("alloc".to_string());
    base.effects.push("io".to_string());

    let mut permuted = base.clone();
    permuted.laws.reverse();
    permuted.capabilities.reverse();
    permuted.effects.reverse();
    assert_eq!(permuted.canonical(), base.canonical());
    assert_eq!(permuted.identity(), base.identity());
}
