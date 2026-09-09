//! Free-world evaluation tests (origin `crates/emath-genesis/src/lib.rs`).

use emath_genesis::{Environment, FreeTermWorld, evaluate, reference_alien_term};
use emath_term::{SymbolId, Term, VariableId};
use emath_test_harness::Probe;

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
fn probe() {
    let mut p = Probe::new(
        "free-world evaluation is a universal round-trip and never collapses swapped arguments",
    );
    p.case("round-trip", |p| {
        let (_, term) = reference_alien_term();
        match evaluate(&term, &FreeTermWorld, &identity_env(&term)) {
            Ok(value) => {
                p.eq("canonical", value.canonical(), term.canonical());
            }
            Err(error) => {
                p.fail(
                    "total",
                    format!("free world evaluation is total on admitted terms: {error}"),
                );
            }
        }
    });
    p.case("argument-order", |p| {
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
        match (
            evaluate(&left, &FreeTermWorld, &identity_env(&left)),
            evaluate(&swapped, &FreeTermWorld, &identity_env(&swapped)),
        ) {
            (Ok(left_value), Ok(swapped_value)) => {
                p.ne("swap", left_value.canonical(), swapped_value.canonical());
            }
            (left_result, swapped_result) => {
                p.fail(
                    "total",
                    format!("free world evaluation is total: {left_result:?} / {swapped_result:?}"),
                );
            }
        }
    });
    p.finish();
}
