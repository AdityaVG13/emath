use emath_genesis::{Disposition, EvalError, FirstOrderWorld, WorldBudget, evaluate_labeled};
use emath_term::{SymbolId, Term};

struct UnknownWorld;
impl FirstOrderWorld for UnknownWorld {
    type Value = i64;
    type Error = EvalError;
    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        Err(EvalError::UnknownSymbol(symbol.clone()))
    }
    fn apply(
        &self,
        symbol: &SymbolId,
        _arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        Err(EvalError::UnknownSymbol(symbol.clone()))
    }
    fn evidence(&self) -> emath_genesis::WorldEvidence {
        emath_genesis::WorldEvidence::seed("free-symbolic", &[])
    }
}

#[test]
fn unknown_symbol_becomes_labeled_open_structure_not_numeric_answer() {
    let term = Term::Constant(SymbolId("⊛".to_string()));
    let result = evaluate_labeled(
        &term,
        &UnknownWorld,
        &[].into_iter().collect(),
        WorldBudget { max_steps: 8 },
        |value| value.to_string(),
    );
    assert!(
        matches!(result.disposition, Disposition::Open { ref missing } if missing == &["symbol:⊛"])
    );
    assert_eq!(result.world, "free-symbolic");
    assert!(!matches!(result.disposition, Disposition::Answer { .. }));
}

#[test]
fn capsule_retains_glyph_hole_world_and_authority_ceiling() {
    let source = std::fs::read_to_string("../../language/spec/worlds/free-symbolic.emath").unwrap();
    for required in [
        "std.syntax.unknown_glyph",
        "std.world.free_symbolic",
        "structural-only",
        "strict-firewall",
        "drop-hole",
        "numeric-overclaim",
    ] {
        assert!(source.contains(required));
    }
    assert!(!source.contains("capsule-active"));
}
