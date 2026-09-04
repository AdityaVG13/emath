//! World ABI and default custom world portfolio.
//!
//! The World ABI is trait-style — carrier, constants,
//! variables, apply, effects, budgets, evidence — and a NEW world
//! implements the ABI only: the evaluator gains no match arm for it. The
//! default custom portfolio orders candidate worlds (free symbolic,
//! canonical finite: Boolean when applicable, modular when applicable)
//! and disposes typed when nothing applies — never a silent fallthrough,
//! and the strict lane never selects an invented world (worlds are passed
//! explicitly; nothing here mutates the strict VM seam).

use emath_genesis::{
    BooleanAlienWorld, Environment, EvalError, FirstOrderWorld, FreeTermWorld, ModularAlienWorld,
    WorldBudget, WorldEvidence, WorldName, evaluate, evaluate_bounded, reference_alien_term,
    select_world,
};
use emath_term::{Signature, SymbolId, Term, VariableId};

#[test]
fn new_world_implements_abi_only() {
    // A brand-new world, defined HERE in the test, implements the trait
    // and evaluates through the UNCHANGED generic evaluator. If this
    // required a genesis evaluator match arm, the law would fail.
    let mut signature = Signature::default();
    signature
        .insert(SymbolId("f".into()), 1)
        .expect("conflict-free");
    signature
        .insert(SymbolId("e".into()), 0)
        .expect("conflict-free");

    struct DoubleItWorld;
    impl FirstOrderWorld for DoubleItWorld {
        type Value = i64;
        type Error = EvalError;

        fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
            match symbol.0.as_str() {
                "e" => Ok(1),
                _ => Err(EvalError::UnknownSymbol(symbol.clone())),
            }
        }

        fn apply(
            &self,
            operator: &SymbolId,
            arguments: Vec<Self::Value>,
        ) -> Result<Self::Value, Self::Error> {
            match (operator.0.as_str(), arguments.as_slice()) {
                ("f", [value]) => Ok(value * 2),
                ("f", values) => Err(EvalError::Arity {
                    symbol: operator.clone(),
                    expected: 1,
                    actual: values.len(),
                }),
                _ => Err(EvalError::UnknownSymbol(operator.clone())),
            }
        }

        fn evidence(&self) -> WorldEvidence {
            WorldEvidence {
                world: "double-it".to_string(),
                origin: "user-defined".to_string(),
                laws: Vec::new(),
            }
        }
    }

    let term = Term::Apply {
        operator: SymbolId("f".into()),
        arguments: vec![Term::Apply {
            operator: SymbolId("f".into()),
            arguments: vec![Term::Constant(SymbolId("e".into()))],
        }],
    };
    let environment = Environment::new();
    let value = evaluate(&term, &DoubleItWorld, &environment).expect("ABI-only world evaluates");
    assert_eq!(value, 4);

    // The new world does not claim the alien portfolio: `admits` defaults
    // to false, so the default portfolio never selects it.
    assert!(!DoubleItWorld.admits(&signature));
    let disposition = select_world(&signature, &[]);
    assert_eq!(disposition.selected, Some(WorldName::FreeSymbolic));
}

#[test]
fn default_portfolio_orders_and_disposes_typed() {
    let (signature, _term) = reference_alien_term();

    // Doctrine order (free symbolic first) selects the free symbolic
    // world by default; the trail records every candidate verdict.
    let default = select_world(&signature, &[]);
    assert_eq!(default.selected, Some(WorldName::FreeSymbolic));
    assert!(
        default
            .trail
            .iter()
            .any(|entry| entry.contains("free-symbolic") && entry.contains("applicable"))
    );

    // Demand a concrete canonical-finite carrier: excluding the free
    // symbolic world selects the Boolean world when applicable.
    let concrete = select_world(&signature, &[WorldName::FreeSymbolic]);
    assert_eq!(concrete.selected, Some(WorldName::BooleanAlien));

    // Excluding both free symbolic and Boolean selects the modular world
    // (modular when applicable).
    let modular = select_world(
        &signature,
        &[WorldName::FreeSymbolic, WorldName::BooleanAlien],
    );
    assert_eq!(modular.selected, Some(WorldName::ModularAlien));

    // Excluding everything: a TYPED disposition with the full trail —
    // never a silent fallthrough, never an invented world.
    let exhausted = select_world(
        &signature,
        &[
            WorldName::FreeSymbolic,
            WorldName::BooleanAlien,
            WorldName::ModularAlien,
        ],
    );
    assert_eq!(exhausted.selected, None);
    assert_eq!(exhausted.trail.len(), 3);
    assert!(
        exhausted
            .trail
            .iter()
            .all(|entry| entry.contains("excluded"))
    );

    // A signature no concrete seed world binds still disposes through the
    // free symbolic baseline, and the trail names the concrete refusals.
    let mut alien = Signature::default();
    alien
        .insert(SymbolId("q".into()), 1)
        .expect("conflict-free");
    let other = select_world(&alien, &[]);
    assert_eq!(other.selected, Some(WorldName::FreeSymbolic));
    assert!(
        other
            .trail
            .iter()
            .any(|entry| entry.contains("boolean-alien") && entry.contains("not applicable"))
    );
}

#[test]
fn budgeted_evaluation_refuses_typed_never_partial() {
    let (signature, term) = reference_alien_term();
    assert!(signature.validate(&term).is_ok());

    // The reference term needs 6 evaluation steps: an exact budget
    // admits, one below refuses typed, and no partial value escapes.
    let environment: Environment<i64> = [
        (VariableId("a".into()), 2_i64),
        (VariableId("b".into()), 4_i64),
    ]
    .into_iter()
    .collect();

    let exact = WorldBudget { max_steps: 6 };
    let value = evaluate_bounded(&term, &ModularAlienWorld, &environment, exact)
        .expect("exact budget evaluates");
    assert_eq!(value, 6); // mod17: join(2,4)=6, neg=36->2, times zeta=3 -> 6

    let starved = WorldBudget { max_steps: 5 };
    match evaluate_bounded(&term, &ModularAlienWorld, &environment, starved) {
        Err(EvalError::BudgetExhausted { steps }) => assert_eq!(steps, 5),
        other => panic!("expected typed budget refusal, got {other:?}"),
    }

    // The unbounded `evaluate` keeps its behavior (delegates to MAX).
    assert_eq!(
        evaluate(&term, &ModularAlienWorld, &environment).expect("unbounded"),
        6
    );
}

#[test]
fn abi_evidence_and_world_bundle() {
    // ABI evidence: every seed world carries stable evidence (name,
    // origin, laws) and declares its effects (seeds are pure: none).
    let free = FreeTermWorld.evidence();
    assert_eq!(free.world, "free-symbolic");
    assert_eq!(free.origin, "seed");
    assert!(BooleanAlienWorld.effects().is_empty());
    assert_eq!(ModularAlienWorld.evidence().world, "modular-17");

    // WorldResultBundle fixture: the custom-world run
    // as a world record; the no-naked-answers rule consumes this shape.
    #[derive(Debug)]
    struct WorldResultBundle {
        world: String,
        verdict: &'static str,
        value: Option<i64>,
        refusals: Vec<String>,
    }
    let (_signature, term) = reference_alien_term();
    let environment: Environment<i64> = [
        (VariableId("a".into()), 2_i64),
        (VariableId("b".into()), 4_i64),
    ]
    .into_iter()
    .collect();
    let value = evaluate(&term, &ModularAlienWorld, &environment).ok();
    let bundle = WorldResultBundle {
        world: ModularAlienWorld.evidence().world,
        verdict: if value.is_some() {
            "evaluated"
        } else {
            "refused"
        },
        value,
        refusals: Vec::new(),
    };
    assert_eq!(bundle.world, "modular-17");
    assert_eq!(bundle.verdict, "evaluated");
    assert_eq!(bundle.value, Some(6));
    assert!(bundle.refusals.is_empty());

    // Negative seed: the seeded invented-world scenario declares a
    // typed refusal.
    const NEGATIVE_SEED: &str = include_str!("../../../tests/invalid/world_abi.emath");
    let expect_line = NEGATIVE_SEED
        .lines()
        .find(|l| l.trim_start().starts_with("# expect:"))
        .expect("seed declares its diagnostic");
    assert!(
        expect_line.contains("E-WORLD") || expect_line.contains("E-VM"),
        "seed expects a typed world refusal, found: {expect_line}"
    );
}
