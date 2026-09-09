//! Strict/fast portfolio routes by guarded region and authority.

use emath_term::{Signature, SymbolId};
use emath_test_harness::Probe;
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
        carriers: vec![CarrierDef { name: "C".to_string(), type_expression: "carrier".to_string() }],
        symbols: vec![SymbolDef { id: symbol.clone(), display: name.to_string(), fixity: Fixity::Prefix, precedence: None, type_scheme: "C → C".to_string() }],
        operators: vec![OperatorDef { symbol, semantics: OperatorSemantics::StructuralConstructor, origin: MeaningOrigin::Declared }],
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
        vec![CarrierMap { source_carrier: "C".to_string(), target_carrier: "C".to_string(), mapping: "id".to_string() }],
        vec![PreservationObligation { symbol: SymbolId("op".to_string()), relation, obligation: "map(op(x)) == op(map(x))".to_string() }],
        vec![],
    );
    StrictFastPortfolio::new(strict, fast, morphism, FastPathGuard { domain: "input < 8".to_string(), required_evidence: vec![] }).expect("invariants")
}

#[test]
fn translation() {
    let mut p = Probe::new("portfolio routes fast inside the guard and deopts outside or weak");
    p.case("region", |p| {
        let portfolio = portfolio(PreservationRelation::Exact);
        let used = [SymbolId("op".to_string())];
        let (fast, reason) = portfolio.select_world(&used, true, true);
        p.eq("in-region", fast, portfolio.fast().identity());
        p.demand("no-receipt", reason.is_none(), "fast needs no receipt");
        let (strict, reason) = portfolio.select_world(&used, false, true);
        p.eq("out-region", strict, portfolio.strict().identity());
        p.eq("deopt", reason.expect("receipt").canonical(), "domain:input < 8".to_string());
    });
    p.case("authority", |p| {
        let weak = portfolio(PreservationRelation::Simulation);
        let used = [SymbolId("op".to_string())];
        let (fast, reason) = weak.select_world_with_authority(&used, true, true, false);
        p.eq("best-effort", fast, weak.fast().identity());
        p.demand("no-receipt", reason.is_none(), "best-effort accepts degradation");
        let (strict, reason) = weak.select_world_with_authority(&used, true, true, true);
        p.eq("authoritative", strict, weak.strict().identity());
        p.eq("deopt", reason.expect("receipt").canonical(), "authority:op:simulation".to_string());
        let exact = portfolio(PreservationRelation::Exact);
        let (fast, reason) = exact.select_world_with_authority(&used, true, true, true);
        p.eq("exact-keeps", fast, exact.fast().identity());
        p.demand("no-receipt", reason.is_none(), "exact keeps authority");
    });
    p.finish();
}
