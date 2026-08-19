//! Free-world evaluation tests (origin `crates/emath-genesis/src/lib.rs`).

use emath_genesis::{Environment, FreeTermWorld, evaluate, reference_alien_term};
use emath_term::{SymbolId, Term, VariableId};

/// Identity environment: every free variable evaluates to itself.
fn identity_env(term: &Term) -> Environment<Term> {
    let mut environment = Environment::new();
    collect_variables(term, &mut environment);
    environment
}

fn collect_variables(term: &Term, environment: &mut Environment<Term>) {
    match term {
        Term::Variable(variable) => {
            environment.insert(variable.clone(), Term::Variable(variable.clone()));
        }
        Term::Constant(_) => {}
        Term::Apply { arguments, .. } => {
            for argument in arguments {
                collect_variables(argument, environment);
            }
        }
    }
}

#[test]
fn free_world_evaluation_is_a_universal_round_trip() {
    // TΣ(X) universality: evaluating any admitted term in the free
    // world under the identity valuation reproduces the term itself,
    // byte-exactly in canonical form.
    let (_, term) = reference_alien_term();
    let value = evaluate(&term, &FreeTermWorld, &identity_env(&term))
        .expect("free world evaluation is total on admitted terms");
    assert_eq!(value.canonical(), term.canonical());
}

#[test]
fn free_world_detects_argument_mutation() {
    // Mutation control: swapping application arguments must change
    // the free value; the free world can never collapse distinct
    // structure.
    let left = Term::Apply {
        operator: SymbolId("⋈".into()),
        arguments: vec![
            Term::Variable(VariableId("a".into())),
            Term::Variable(VariableId("b".into())),
        ],
    };
    let swapped = Term::Apply {
        operator: SymbolId("⋈".into()),
        arguments: vec![
            Term::Variable(VariableId("b".into())),
            Term::Variable(VariableId("a".into())),
        ],
    };
    let left_value = evaluate(&left, &FreeTermWorld, &identity_env(&left))
        .expect("free world evaluation is total");
    let swapped_value = evaluate(&swapped, &FreeTermWorld, &identity_env(&swapped))
        .expect("free world evaluation is total");
    assert_ne!(left_value.canonical(), swapped_value.canonical());
}
