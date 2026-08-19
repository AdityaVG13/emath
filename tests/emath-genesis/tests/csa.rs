//! Canonical seeded algebra tests (origin `crates/emath-genesis/src/csa.rs`).

use emath_genesis::{
    CSA_MEANING_CLAIM, Environment, OnePointWorld, SeededCsaWorld, evaluate, reference_alien_term,
};
use emath_term::{SymbolId, Term, VariableId};

fn csa_env(world: SeededCsaWorld) -> Environment<u64> {
    [
        (VariableId("a".into()), world.variable_value("a")),
        (VariableId("b".into()), world.variable_value("b")),
    ]
    .into()
}

#[test]
fn seeded_csa_is_total_and_reproducible() {
    // Totality: the reference alien term (unknown to any hand-written
    // table) evaluates without error. Determinism: two independent
    // evaluations agree bit-exactly.
    let world = SeededCsaWorld::baseline();
    let (_, term) = reference_alien_term();
    let first = evaluate(&term, &world, &csa_env(world)).expect("CSA is total");
    let second = evaluate(&term, &world, &csa_env(world)).expect("CSA is total");
    assert_eq!(first, second);
}

#[test]
fn distinct_seeds_are_a_negative_control() {
    // The same term under a different seed must produce a different
    // value: replay against the wrong seed is detected, not absorbed.
    let baseline = SeededCsaWorld::baseline();
    let wrong = SeededCsaWorld {
        seed: baseline.seed ^ 1,
    };
    let (_, term) = reference_alien_term();
    let expected = evaluate(&term, &baseline, &csa_env(baseline)).expect("total");
    let actual = evaluate(&term, &wrong, &csa_env(wrong)).expect("total");
    assert_ne!(expected, actual, "seeded negative control must trip");
}

#[test]
fn seeded_csa_distinguishes_argument_order() {
    // Adversarial: a mixing function that ignored argument order would
    // collapse distinct terms. Swapped arguments must differ.
    let world = SeededCsaWorld::baseline();
    let apply = |first: &str, second: &str| Term::Apply {
        operator: SymbolId("⋈".into()),
        arguments: vec![
            Term::Variable(VariableId(first.into())),
            Term::Variable(VariableId(second.into())),
        ],
    };
    let left = evaluate(&apply("a", "b"), &world, &csa_env(world)).expect("total");
    let right = evaluate(&apply("b", "a"), &world, &csa_env(world)).expect("total");
    assert_ne!(left, right);
}

#[test]
#[allow(clippy::zero_sized_map_values)] // the unit carrier is the point of the one-point world
fn one_point_world_is_total_on_anything() {
    let (_, term) = reference_alien_term();
    let environment: Environment<()> =
        [(VariableId("a".into()), ()), (VariableId("b".into()), ())].into();
    evaluate(&term, &OnePointWorld, &environment).expect("one-point world is total");
}

#[test]
fn csa_never_claims_intended_meaning() {
    // Labeling control: the claim string every CSA artifact carries
    // must disown intended meaning and must never say "tested".
    assert!(CSA_MEANING_CLAIM.contains("never author-intended meaning"));
    assert!(!CSA_MEANING_CLAIM.contains("tested"));
}
