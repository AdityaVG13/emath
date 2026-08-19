use emath_term::{Signature, SymbolId};
use emath_world_ir::translation::{
    CarrierMap, FastPathGuard, PreservationObligation, PreservationRelation, StrictFastPortfolio,
    WorldMorphism,
};
use emath_world_ir::{
    CarrierDef, Fixity, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WORLD_IR_VERSION,
    WorldIr,
};

fn world(name: &str, law: &str) -> WorldIr {
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

fn portfolio(relation: PreservationRelation) -> StrictFastPortfolio {
    let strict = world("strict", "law-strict");
    let fast = world("fast", "law-fast");
    let morphism = WorldMorphism::new(
        strict.identity(),
        fast.identity(),
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
    );
    StrictFastPortfolio::new(
        strict,
        fast,
        morphism,
        FastPathGuard {
            domain: "input < 8".to_string(),
            required_evidence: vec![],
        },
    )
    .expect("constructor invariants hold")
}

/// Region partitioning: the same request routes to the fast variant
/// inside the guarded input region and deoptimizes to strict outside
/// it, with a canonical deopt receipt.
#[test]
fn input_region_partitions_fast_and_strict_routing() {
    let portfolio = portfolio(PreservationRelation::Exact);
    let used = [SymbolId("op".to_string())];

    let (in_region, reason) = portfolio.select_world(&used, true, true);
    assert_eq!(in_region, portfolio.fast().identity());
    assert!(reason.is_none());

    let (out_of_region, reason) = portfolio.select_world(&used, false, true);
    assert_eq!(out_of_region, portfolio.strict().identity());
    assert_eq!(
        reason.expect("deopt receipt").canonical(),
        "domain:input < 8"
    );
}

/// Authority-aware dispatch: a simulation-only fast world serves
/// best-effort requests but deoptimizes when the caller requires an
/// answer with full authority.
#[test]
fn authority_requirement_deopts_weak_morphisms() {
    let weak = portfolio(PreservationRelation::Simulation);
    let used = [SymbolId("op".to_string())];

    let (best_effort, reason) = weak.select_world_with_authority(&used, true, true, false);
    assert_eq!(best_effort, weak.fast().identity());
    assert!(reason.is_none(), "best-effort requests accept degradation");

    let (authoritative, reason) = weak.select_world_with_authority(&used, true, true, true);
    assert_eq!(authoritative, weak.strict().identity());
    assert_eq!(
        reason.expect("deopt receipt").canonical(),
        "authority:op:simulation"
    );

    let exact = portfolio(PreservationRelation::Exact);
    let (fast, reason) = exact.select_world_with_authority(&used, true, true, true);
    assert_eq!(fast, exact.fast().identity());
    assert!(reason.is_none(), "exact morphisms keep full authority");
}
