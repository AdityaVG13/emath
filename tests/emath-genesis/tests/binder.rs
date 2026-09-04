//! binder tests migrated from the in-crate `#[cfg(test)]`
//! module: every symbol they exercise is public crate surface.

use emath_genesis::binder::{
    BINDER_VERSION, BinderBudget, BinderDomain, BinderError, BinderFamily, BinderKind, BinderTerm,
    ScopedBinder, binder_id, check_version,
};
use emath_term::{SymbolId, Term, VariableId};

fn var(name: &str) -> Term {
    Term::Variable(VariableId(name.to_string()))
}

fn sum_over(bound: &str, lower: i64, upper: i64, body: BinderTerm) -> ScopedBinder {
    ScopedBinder {
        kind: BinderKind::Sum,
        family: BinderFamily::Structural,
        domain: BinderDomain::FiniteRange { lower, upper },
        bound: VariableId(bound.to_string()),
        body,
    }
}

/// Naive (capture-unsafe) substitution used only as this module's
/// negative reference: it descends under binders without renaming.
fn naive_substitute(term: &BinderTerm, variable: &VariableId, replacement: &Term) -> BinderTerm {
    match term {
        BinderTerm::Leaf(leaf) => BinderTerm::Leaf(emath_genesis::binder::substitute_term(
            leaf,
            variable,
            replacement,
        )),
        BinderTerm::Bind(binder) => BinderTerm::Bind(Box::new(ScopedBinder {
            kind: binder.kind.clone(),
            family: binder.family,
            domain: binder.domain.clone(),
            bound: binder.bound.clone(),
            body: naive_substitute(&binder.body, variable, replacement),
        })),
    }
}

#[test]
fn capture_avoiding_substitution_keeps_the_replacement_free() {
    // (sum x in 1..=2. y)[y := x] — x must stay free, not get captured.
    let binder = BinderTerm::Bind(Box::new(sum_over("x", 1, 2, BinderTerm::Leaf(var("y")))));
    let safe = binder.substitute(&VariableId("y".to_string()), &var("x"));
    let naive = naive_substitute(&binder, &VariableId("y".to_string()), &var("x"));

    // Capture-safe: x remains a free variable of the whole term.
    assert!(safe.free_variables().contains(&VariableId("x".to_string())));
    // Naive capture: x vanished into the binder.
    assert!(naive.free_variables().is_empty());
    assert_ne!(safe.canonical(), naive.canonical());

    // Expanding the safe term keeps var(x) intact in every instance.
    let BinderTerm::Bind(safe_binder) = safe else {
        panic!("substitution must preserve the binder node");
    };
    let expanded = safe_binder
        .expand(&SymbolId("+".to_string()), BinderBudget::default())
        .expect("structural expansion succeeds");
    assert_eq!(
        expanded.canonical(),
        "apply(+,var(x),var(x))",
        "the substituted variable must not be captured by the binder"
    );
}

#[test]
fn alpha_equivalent_binders_share_one_identity() {
    let with_x = sum_over("x", 1, 3, BinderTerm::Leaf(var("x")));
    let with_z = sum_over("z", 1, 3, BinderTerm::Leaf(var("z")));
    assert_eq!(with_x.canonical(), with_z.canonical());
    assert_eq!(binder_id(&with_x), binder_id(&with_z));

    // A semantic difference (domain) changes identity.
    let wider = sum_over("x", 1, 4, BinderTerm::Leaf(var("x")));
    assert_ne!(binder_id(&with_x), binder_id(&wider));
}

#[test]
fn nested_binders_expand_within_one_shared_budget() {
    // sum x in 1..=2. (sum y in 1..=2. y) — inner binder is expanded
    // per outer instantiation under the same budget.
    let inner = sum_over("y", 1, 2, BinderTerm::Leaf(var("y")));
    let outer = sum_over("x", 1, 2, BinderTerm::Bind(Box::new(inner)));
    let plus = SymbolId("+".to_string());
    let expanded = outer
        .expand(&plus, BinderBudget::default())
        .expect("nested expansion succeeds");
    assert_eq!(
        expanded.canonical(),
        "apply(+,apply(+,const(1),const(2)),apply(+,const(1),const(2)))"
    );
    // 2 outer + 2*2 inner instantiations = 6 > 5.
    assert_eq!(
        outer.expand(&plus, BinderBudget { max_terms: 5 }),
        Err(BinderError::BudgetExceeded { limit: 5 })
    );
}

#[test]
fn refusals_are_typed_not_silent() {
    let plus = SymbolId("+".to_string());

    // Conventional never expands.
    let derivative = ScopedBinder {
        kind: BinderKind::Derivative,
        family: BinderFamily::Conventional,
        domain: BinderDomain::Symbolic {
            anchor: "t".to_string(),
        },
        bound: VariableId("t".to_string()),
        body: BinderTerm::Leaf(var("t")),
    };
    assert_eq!(
        derivative.expand(&plus, BinderBudget::default()),
        Err(BinderError::NotExpandable {
            kind: "derivative".to_string(),
            family: BinderFamily::Conventional,
        })
    );

    // Structural over a symbolic domain refuses.
    let symbolic_sum = ScopedBinder {
        domain: BinderDomain::Symbolic {
            anchor: "N".to_string(),
        },
        ..sum_over("x", 0, 0, BinderTerm::Leaf(var("x")))
    };
    assert_eq!(
        symbolic_sum.expand(&plus, BinderBudget::default()),
        Err(BinderError::NonFiniteDomain {
            kind: "sum".to_string(),
            family: BinderFamily::Structural,
        })
    );

    // Empty range refuses; no neutral element is invented.
    let empty = sum_over("x", 3, 2, BinderTerm::Leaf(var("x")));
    assert_eq!(
        empty.expand(&plus, BinderBudget::default()),
        Err(BinderError::EmptyDomain { lower: 3, upper: 2 })
    );

    // Opaque identity is reserved for the opaque-seeded family.
    let structural = sum_over("x", 1, 2, BinderTerm::Leaf(var("x")));
    assert_eq!(
        structural.opaque_identity(7),
        Err(BinderError::WrongFamily {
            operation: "opaque_identity",
            family: BinderFamily::Structural,
        })
    );

    // Unknown schema versions are refused.
    assert_eq!(check_version(BINDER_VERSION), Ok(()));
    assert_eq!(
        check_version(BINDER_VERSION + 1),
        Err(BinderError::UnknownVersion {
            version: BINDER_VERSION + 1
        })
    );
}

#[test]
fn opaque_seeded_identity_is_deterministic_and_seed_sensitive() {
    let limit = ScopedBinder {
        kind: BinderKind::Limit,
        family: BinderFamily::OpaqueSeeded,
        domain: BinderDomain::Symbolic {
            anchor: "x->0".to_string(),
        },
        bound: VariableId("x".to_string()),
        body: BinderTerm::Leaf(var("x")),
    };
    let first = limit.opaque_identity(41).expect("opaque family");
    let again = limit.opaque_identity(41).expect("opaque family");
    let other_seed = limit.opaque_identity(42).expect("opaque family");
    assert_eq!(first, again, "same seed must reproduce the identity");
    assert_ne!(first, other_seed, "seed participates in the identity");
}
