//! Unknown symbols stay open structure, and the free-symbolic capsule keeps its ceiling.

use emath_genesis::{Disposition, EvalError, FirstOrderWorld, WorldBudget, evaluate_labeled};
use emath_term::{SymbolId, Term};
use emath_test_harness::Probe;

struct UnknownWorld;
impl FirstOrderWorld for UnknownWorld {
    type Value = i64;
    type Error = EvalError;
    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        Err(EvalError::UnknownSymbol(symbol.clone()))
    }
    fn apply(&self, symbol: &SymbolId, _arguments: Vec<Self::Value>) -> Result<Self::Value, Self::Error> {
        Err(EvalError::UnknownSymbol(symbol.clone()))
    }
    fn evidence(&self) -> emath_genesis::WorldEvidence {
        emath_genesis::WorldEvidence::seed("free-symbolic", &[])
    }
}

#[test]
fn symbolic_fallback() {
    let mut p = Probe::new("unknown symbols stay open, never numeric answers");
    p.case("open-structure", |p| {
        let term = Term::Constant(SymbolId("⊛".to_string()));
        let result = evaluate_labeled(&term, &UnknownWorld, &[].into_iter().collect(), WorldBudget { max_steps: 8 }, |value| value.to_string());
        p.demand("hole", matches!(result.disposition, Disposition::Open { ref missing } if missing == &["symbol:⊛"]), "unknown symbol is a labeled hole");
        p.eq("world", result.world, "free-symbolic".to_string());
        p.demand("no-answer", !matches!(result.disposition, Disposition::Answer { .. }), "never a numeric answer");
    });
    p.case("capsule-ceiling", |p| {
        let source = std::fs::read_to_string("../../language/spec/worlds/free-symbolic.emath").unwrap();
        for required in ["std.syntax.unknown_glyph", "std.world.free_symbolic", "structural-only", "strict-firewall", "drop-hole", "numeric-overclaim"] {
            p.contains(required, &source, required);
        }
        p.demand("no-active", !source.contains("capsule-active"), "capsule stays inactive");
    });
    p.finish();
}
