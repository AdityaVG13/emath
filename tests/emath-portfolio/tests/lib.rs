//! Deterministic interpretation portfolio tests, moved from
//! `crates/emath-portfolio/src/lib.rs`.

use emath_portfolio::{
    Authority, InterpretationCandidate, InterpretationPortfolio, ScoreVector, translated_candidate,
};
use emath_term::{Signature, SymbolId};
use emath_world_ir::translation::{
    CarrierMap, FastPathGuard, PreservationObligation, PreservationRelation, StrictFastPortfolio,
    WorldMorphism,
};
use emath_world_ir::{
    CarrierDef, Fixity, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WORLD_IR_VERSION,
    WorldId, WorldIr,
};

fn candidate(
    id: u64,
    name: &str,
    authority: Authority,
    utility: f64,
    cost: f64,
) -> InterpretationCandidate {
    InterpretationCandidate {
        world_id: WorldId(id),
        name: name.to_string(),
        answer: String::new(),
        authority,
        score: ScoreVector {
            cost,
            complexity: 1.0,
            evidence: 0.0,
            utility,
        },
        provenance: String::new(),
    }
}

#[test]
fn portfolio_order_follows_the_stable_policy() {
    // Authority descending beats utility; utility descending beats
    // cost; input order never matters.
    let shuffled = vec![
        candidate(3, "cheap-structural", Authority::Structural, 5.0, 1.0),
        candidate(1, "tested", Authority::Tested, 1.0, 9.0),
        candidate(2, "useful-structural", Authority::Structural, 9.0, 5.0),
    ];
    let portfolio = InterpretationPortfolio::new(shuffled);
    let names: Vec<&str> = portfolio
        .candidates()
        .iter()
        .map(|candidate| candidate.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["tested", "useful-structural", "cheap-structural"]
    );
}

#[test]
fn equal_scores_tie_break_on_world_identity() {
    let portfolio = InterpretationPortfolio::new(vec![
        candidate(9, "b", Authority::Structural, 1.0, 1.0),
        candidate(4, "a", Authority::Structural, 1.0, 1.0),
    ]);
    let ids: Vec<u64> = portfolio
        .candidates()
        .iter()
        .map(|candidate| candidate.world_id.0)
        .collect();
    assert_eq!(ids, vec![4, 9], "ties resolve by world identity ascending");
}

fn small_world(name: &str, law: &str) -> WorldIr {
    let mut signature = Signature::default();
    let symbol = SymbolId("op".to_string());
    signature.insert(symbol.clone(), 1).unwrap();
    WorldIr {
        version: WORLD_IR_VERSION,
        name: name.to_string(),
        signature,
        carriers: vec![CarrierDef {
            name: "C".to_string(),
            type_expression: "carrier".to_string(),
        }],
        symbols: vec![SymbolDef {
            id: symbol.clone(),
            display: name.to_string(),
            fixity: Fixity::Prefix,
            precedence: None,
            type_scheme: "C → C".to_string(),
        }],
        operators: vec![OperatorDef {
            symbol,
            semantics: OperatorSemantics::StructuralConstructor,
            origin: MeaningOrigin::Declared,
        }],
        constructors: vec![],
        laws: vec![law.to_string()],
        effects: vec![],
        holes: vec![],
        capabilities: vec![],
    }
}

fn morphism_between(
    source: WorldId,
    target: WorldId,
    relation: PreservationRelation,
) -> WorldMorphism {
    WorldMorphism::new(
        source,
        target,
        vec![CarrierMap {
            source_carrier: "C".to_string(),
            target_carrier: "C".to_string(),
            mapping: "id".to_string(),
        }],
        vec![PreservationObligation {
            symbol: SymbolId("op".to_string()),
            relation,
            obligation: "map(op(x)) == op(map(x))".to_string(),
        }],
        vec![],
    )
}

#[test]
fn exit_gate_keeps_both_worlds_and_deopts_on_failed_guard() {
    let world_a = small_world("A", "law-a");
    let world_b = small_world("B", "law-b");
    let morphism = morphism_between(
        world_a.identity(),
        world_b.identity(),
        PreservationRelation::Exact,
    );
    let fast_portfolio = StrictFastPortfolio::new(
        world_a.clone(),
        world_b.clone(),
        morphism.clone(),
        FastPathGuard {
            domain: "input < 8".to_string(),
            required_evidence: vec![],
        },
    )
    .expect("strict/fast constructor invariants hold");
    let (selected, reason) =
        fast_portfolio.select_world(&[SymbolId("op".to_string())], false, true);
    assert_eq!(selected, world_a.identity());
    assert!(reason.is_some(), "failed domain guard must deopt");

    let base = InterpretationCandidate {
        world_id: world_a.identity(),
        name: "strict".to_string(),
        answer: "7".to_string(),
        authority: Authority::Tested,
        score: ScoreVector {
            cost: 1.0,
            complexity: 1.0,
            evidence: 1.0,
            utility: 1.0,
        },
        provenance: "base".to_string(),
    };
    let translated = translated_candidate(&morphism, &base, "7".to_string());
    assert_eq!(translated.world_id, world_b.identity());
    assert!(
        translated
            .provenance
            .contains(&format!("morphism:{:x}", morphism.identity()))
    );

    let portfolio = InterpretationPortfolio::new(vec![translated, base]);
    assert_eq!(portfolio.candidates().len(), 2);
    let ids: Vec<WorldId> = portfolio
        .candidates()
        .iter()
        .map(|candidate| candidate.world_id)
        .collect();
    let mut expected = vec![world_a.identity(), world_b.identity()];
    expected.sort();
    assert_eq!(ids, expected);
}

#[test]
fn translated_authority_is_capped_by_preservation_relation() {
    let base = candidate(1, "tested", Authority::Tested, 1.0, 1.0);
    let degraded = translated_candidate(
        &morphism_between(WorldId(1), WorldId(2), PreservationRelation::Approximation),
        &base,
        "approx".to_string(),
    );
    assert_eq!(degraded.authority, Authority::Structural);
    assert_eq!(degraded.world_id, WorldId(2));

    let preserved = translated_candidate(
        &morphism_between(WorldId(1), WorldId(2), PreservationRelation::Exact),
        &base,
        "exact".to_string(),
    );
    assert_eq!(preserved.authority, Authority::Tested);
    assert_eq!(preserved.world_id, WorldId(2));
}
