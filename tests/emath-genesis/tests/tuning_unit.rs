use emath_genesis::tuning::{
    CoverageSample, DeltaError, DeltaReceipt, SemanticChange, SemanticVariableKind, WorldDelta,
    calibrate_confidence,
};
use emath_term::{Signature, SymbolId};
use emath_world_ir::{
    CarrierDef, Fixity, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WORLD_IR_VERSION,
    WorldId, WorldIr,
};

const KINDS: [SemanticVariableKind; 8] = [
    SemanticVariableKind::Carrier,
    SemanticVariableKind::Symbol,
    SemanticVariableKind::Signature,
    SemanticVariableKind::Operator,
    SemanticVariableKind::Constant,
    SemanticVariableKind::Constructor,
    SemanticVariableKind::Law,
    SemanticVariableKind::Effect,
];

fn change(kind: SemanticVariableKind, description: &str) -> SemanticChange {
    SemanticChange {
        kind,
        symbol: None,
        description: description.to_string(),
        provenance: "synthesized".to_string(),
    }
}

fn lifecycle_world() -> WorldIr {
    let mut signature = Signature::default();
    signature.insert(SymbolId("⋈".to_string()), 2).unwrap();
    signature.insert(SymbolId("ζ".to_string()), 0).unwrap();
    WorldIr {
        version: WORLD_IR_VERSION,
        name: "lifecycle".to_string(),
        signature,
        carriers: vec![CarrierDef {
            name: "C".to_string(),
            type_expression: "carrier".to_string(),
        }],
        symbols: vec![
            SymbolDef {
                id: SymbolId("⋈".to_string()),
                display: "⋈".to_string(),
                fixity: Fixity::Infix,
                precedence: Some(50),
                type_scheme: "C × C → C".to_string(),
            },
            SymbolDef {
                id: SymbolId("ζ".to_string()),
                display: "ζ".to_string(),
                fixity: Fixity::Constant,
                precedence: None,
                type_scheme: "C".to_string(),
            },
        ],
        operators: vec![
            OperatorDef {
                symbol: SymbolId("⋈".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("x + y".to_string()),
                origin: MeaningOrigin::Declared,
            },
            OperatorDef {
                symbol: SymbolId("ζ".to_string()),
                semantics: OperatorSemantics::DeclaredExpression("0".to_string()),
                origin: MeaningOrigin::Declared,
            },
        ],
        constructors: vec!["C.new() -> C".to_string()],
        laws: vec!["forall x y. x ⋈ y == y ⋈ x".to_string()],
        effects: vec!["pure".to_string()],
        holes: vec![],
        capabilities: vec![],
    }
}

fn lifecycle_delta(base: &WorldIr) -> WorldDelta {
    WorldDelta::new(
        base.identity(),
        vec![
            SemanticChange::replace(
                SemanticVariableKind::Carrier,
                Some(SymbolId("C".to_string())),
                "carrier",
                "other carrier",
                "synthesized",
            ),
            SemanticChange::replace(
                SemanticVariableKind::Symbol,
                Some(SymbolId("⋈".to_string())),
                "infix:50:C × C → C",
                "infix:50:C → C",
                "synthesized",
            ),
            SemanticChange::replace(
                SemanticVariableKind::Signature,
                Some(SymbolId("⋈".to_string())),
                "2",
                "3",
                "synthesized",
            ),
            SemanticChange::replace(
                SemanticVariableKind::Operator,
                Some(SymbolId("⋈".to_string())),
                "expr:x + y",
                "expr:x * y",
                "synthesized",
            ),
            SemanticChange::replace(
                SemanticVariableKind::Constant,
                Some(SymbolId("ζ".to_string())),
                "expr:0",
                "expr:1",
                "synthesized",
            ),
            SemanticChange::replace(
                SemanticVariableKind::Constructor,
                Some(SymbolId("C.new() -> C".to_string())),
                "C.new() -> C",
                "C.other() -> C",
                "synthesized",
            ),
            SemanticChange::replace(
                SemanticVariableKind::Law,
                Some(SymbolId("forall x y. x ⋈ y == y ⋈ x".to_string())),
                "forall x y. x ⋈ y == y ⋈ x",
                "forall x. x ⋈ x == x",
                "synthesized",
            ),
            SemanticChange::replace(
                SemanticVariableKind::Effect,
                Some(SymbolId("pure".to_string())),
                "pure",
                "io",
                "synthesized",
            ),
        ],
    )
}

#[test]
fn semantic_variable_kind_canonical_names_round_trip() {
    for kind in KINDS {
        assert_eq!(
            SemanticVariableKind::from_canonical(kind.canonical()),
            Some(kind)
        );
    }
}

#[test]
fn receipt_identity_is_deterministic() {
    let changes = vec![
        change(SemanticVariableKind::Law, "assoc"),
        change(SemanticVariableKind::Carrier, "bool"),
    ];
    let first = DeltaReceipt::new(0x1111, &changes);
    let second = DeltaReceipt::new(0x1111, &changes);
    let reordered = DeltaReceipt::new(
        0x1111,
        &[
            change(SemanticVariableKind::Carrier, "bool"),
            change(SemanticVariableKind::Law, "assoc"),
        ],
    );
    assert_eq!(first.identity, second.identity);
    assert_eq!(first, second);
    assert_eq!(first.identity, reordered.identity);
}

#[test]
fn receipt_identity_is_sensitive_to_base_and_changes() {
    let law = change(SemanticVariableKind::Law, "assoc");
    let carrier = change(SemanticVariableKind::Carrier, "bool");
    let base = DeltaReceipt::new(1, std::slice::from_ref(&law));
    assert_ne!(
        base.identity,
        DeltaReceipt::new(2, std::slice::from_ref(&law)).identity
    );
    assert_ne!(base.identity, DeltaReceipt::new(1, &[carrier]).identity);
}

#[test]
fn locality_reports_exactly_the_touched_components() {
    let delta = WorldDelta::new(
        WorldId(7),
        vec![
            change(SemanticVariableKind::Law, "assoc"),
            change(SemanticVariableKind::Carrier, "bool"),
            change(SemanticVariableKind::Carrier, "fin"),
            change(SemanticVariableKind::Effect, "io"),
        ],
    );
    assert_eq!(delta.locality(), ["carriers", "effects", "laws"]);
    assert_eq!(delta.receipt().locality(), ["carriers", "effects", "laws"]);
    assert!(DeltaReceipt::new(0, &[]).locality().is_empty());
}

#[test]
fn memorizing_candidate_fails_held_out_challenge() {
    let result = calibrate_confidence(CoverageSample {
        construction_permille: 1000,
        held_out_permille: 0,
        table_cells: 4,
        construction_examples: 4,
    });
    assert!(!result.admitted);
    assert_eq!(result.reason, "held-out:memorization");
}

#[test]
fn general_candidate_survives_and_oversize_table_is_penalized() {
    let general = calibrate_confidence(CoverageSample {
        construction_permille: 1000,
        held_out_permille: 1000,
        table_cells: 4,
        construction_examples: 4,
    });
    assert!(general.admitted);
    assert_eq!(general.reason, "passed");
    assert_eq!(general.complexity_penalty_permille, 0);

    let oversize = calibrate_confidence(CoverageSample {
        construction_permille: 1000,
        held_out_permille: 1000,
        table_cells: 16,
        construction_examples: 2,
    });
    assert!(!oversize.admitted);
    assert_eq!(oversize.reason, "complexity-penalty");
    assert_eq!(oversize.complexity_penalty_permille, 875);
}

#[test]
fn apply_then_revert_restores_canonical_form_and_identity() {
    let base = lifecycle_world();
    let delta = lifecycle_delta(&base);
    let applied = delta.apply(&base).expect("apply");
    let reverted = delta.revert(&applied).expect("revert");
    assert_eq!(reverted.canonical(), base.canonical());
    assert_eq!(reverted.identity(), base.identity());
}

#[test]
fn apply_changes_world_identity() {
    let base = lifecycle_world();
    let applied = lifecycle_delta(&base).apply(&base).expect("apply");
    assert_ne!(applied.identity(), base.identity());
}

#[test]
fn apply_refuses_missing_target() {
    let base = lifecycle_world();
    let delta = WorldDelta::new(
        base.identity(),
        vec![SemanticChange::replace(
            SemanticVariableKind::Law,
            Some(SymbolId("missing-law".to_string())),
            "missing-law",
            "other-law",
            "synthesized",
        )],
    );
    match delta.apply(&base) {
        Err(DeltaError::MissingTarget { kind, target }) => {
            assert_eq!(kind, SemanticVariableKind::Law);
            assert_eq!(target, "missing-law");
        }
        other => panic!("expected MissingTarget, got {other:?}"),
    }
}
