//! Canonical seeded algebra tests.

use emath_genesis::{CSA_MEANING_CLAIM, Environment, OnePointWorld, SeededCsaWorld, evaluate, reference_alien_term};
use emath_term::{SymbolId, Term, VariableId};
use emath_test_harness::Probe;

fn csa_env(world: SeededCsaWorld) -> Environment<u64> {
    [(VariableId("a".into()), world.variable_value("a")), (VariableId("b".into()), world.variable_value("b"))].into()
}

#[test]
fn seeded_csa() {
    let mut p = Probe::new("the seeded algebra is total, deterministic, and disowns meaning");
    p.case("total-and-reproducible", |p| {
        let world = SeededCsaWorld::baseline();
        let (_, term) = reference_alien_term();
        let first = evaluate(&term, &world, &csa_env(world)).expect("CSA is total");
        let second = evaluate(&term, &world, &csa_env(world)).expect("CSA is total");
        p.eq("deterministic", first, second);
    });
    p.case("seed-negative-control", |p| {
        let baseline = SeededCsaWorld::baseline();
        let wrong = SeededCsaWorld { seed: baseline.seed ^ 1 };
        let (_, term) = reference_alien_term();
        let expected = evaluate(&term, &baseline, &csa_env(baseline)).expect("total");
        let actual = evaluate(&term, &wrong, &csa_env(wrong)).expect("total");
        p.ne("seed-trips", expected, actual);
    });
    p.case("order-matters", |p| {
        let world = SeededCsaWorld::baseline();
        let apply = |first: &str, second: &str| Term::Apply { operator: SymbolId("⋈".into()), arguments: vec![Term::Variable(VariableId(first.into())), Term::Variable(VariableId(second.into()))] };
        let left = evaluate(&apply("a", "b"), &world, &csa_env(world)).expect("total");
        let right = evaluate(&apply("b", "a"), &world, &csa_env(world)).expect("total");
        p.ne("args-distinguished", left, right);
    });
    p.case("one-point-total", |p| {
        let (_, term) = reference_alien_term();
        let environment: Environment<()> = [(VariableId("a".into()), ()), (VariableId("b".into()), ())].into();
        p.demand("total", evaluate(&term, &OnePointWorld, &environment).is_ok(), "one-point world is total");
    });
    p.case("no-meaning-claim", |p| {
        p.contains("disown", CSA_MEANING_CLAIM, "never author-intended meaning");
        p.demand("untested", !CSA_MEANING_CLAIM.contains("tested"), "must never say tested");
    });
    p.finish();
}
