//! Binder capture, identity, expansion, and refusal tests.

use emath_genesis::binder::{
    BINDER_VERSION, BinderBudget, BinderDomain, BinderError, BinderFamily, BinderKind, BinderTerm,
    ScopedBinder, binder_id, check_version,
};
use emath_term::{SymbolId, Term, VariableId};
use emath_test_harness::Probe;

fn var(name: &str) -> Term {
    Term::Variable(VariableId(name.to_string()))
}

fn sum_over(bound: &str, lower: i64, upper: i64, body: BinderTerm) -> ScopedBinder {
    ScopedBinder { kind: BinderKind::Sum, family: BinderFamily::Structural, domain: BinderDomain::FiniteRange { lower, upper }, bound: VariableId(bound.to_string()), body }
}

fn naive_substitute(term: &BinderTerm, variable: &VariableId, replacement: &Term) -> BinderTerm {
    match term {
        BinderTerm::Leaf(leaf) => BinderTerm::Leaf(emath_genesis::binder::substitute_term(leaf, variable, replacement)),
        BinderTerm::Bind(binder) => BinderTerm::Bind(Box::new(ScopedBinder { kind: binder.kind.clone(), family: binder.family, domain: binder.domain.clone(), bound: binder.bound.clone(), body: naive_substitute(&binder.body, variable, replacement) })),
    }
}

#[test]
fn binder() {
    let mut p = Probe::new("binders substitute without capture and refuse without invention");
    p.case("capture-avoiding", |p| {
        let binder = BinderTerm::Bind(Box::new(sum_over("x", 1, 2, BinderTerm::Leaf(var("y")))));
        let safe = binder.substitute(&VariableId("y".to_string()), &var("x"));
        let naive = naive_substitute(&binder, &VariableId("y".to_string()), &var("x"));
        p.demand("free", safe.free_variables().contains(&VariableId("x".to_string())), "x stays free");
        p.demand("captured", naive.free_variables().is_empty(), "naive capture loses x");
        p.ne("differs", safe.canonical(), naive.canonical());
        let BinderTerm::Bind(safe_binder) = safe else { p.fail("binder", "substitution must preserve the binder"); return; };
        let expanded = safe_binder.expand(&SymbolId("+".to_string()), BinderBudget::default()).expect("structural expansion succeeds");
        p.eq("instances", expanded.canonical(), "apply(+,var(x),var(x))".to_string());
    });
    p.case("alpha-identity", |p| {
        let with_x = sum_over("x", 1, 3, BinderTerm::Leaf(var("x")));
        let with_z = sum_over("z", 1, 3, BinderTerm::Leaf(var("z")));
        p.eq("canonical", with_x.canonical(), with_z.canonical());
        p.eq("id", binder_id(&with_x), binder_id(&with_z));
        p.ne("domain-binds", binder_id(&with_x), binder_id(&sum_over("x", 1, 4, BinderTerm::Leaf(var("x")))));
    });
    p.case("nested-budget", |p| {
        let inner = sum_over("y", 1, 2, BinderTerm::Leaf(var("y")));
        let outer = sum_over("x", 1, 2, BinderTerm::Bind(Box::new(inner)));
        let plus = SymbolId("+".to_string());
        let expanded = outer.expand(&plus, BinderBudget::default()).expect("nested expansion succeeds");
        p.eq("shape", expanded.canonical(), "apply(+,apply(+,const(1),const(2)),apply(+,const(1),const(2)))".to_string());
        p.eq("capped", outer.expand(&plus, BinderBudget { max_terms: 5 }), Err(BinderError::BudgetExceeded { limit: 5 }));
    });
    p.case("typed-refusals", |p| {
        let plus = SymbolId("+".to_string());
        let derivative = ScopedBinder { kind: BinderKind::Derivative, family: BinderFamily::Conventional, domain: BinderDomain::Symbolic { anchor: "t".to_string() }, bound: VariableId("t".to_string()), body: BinderTerm::Leaf(var("t")) };
        p.eq("conventional", derivative.expand(&plus, BinderBudget::default()), Err(BinderError::NotExpandable { kind: "derivative".to_string(), family: BinderFamily::Conventional }));
        let symbolic_sum = ScopedBinder { domain: BinderDomain::Symbolic { anchor: "N".to_string() }, ..sum_over("x", 0, 0, BinderTerm::Leaf(var("x"))) };
        p.eq("symbolic", symbolic_sum.expand(&plus, BinderBudget::default()), Err(BinderError::NonFiniteDomain { kind: "sum".to_string(), family: BinderFamily::Structural }));
        p.eq("empty", sum_over("x", 3, 2, BinderTerm::Leaf(var("x"))).expand(&plus, BinderBudget::default()), Err(BinderError::EmptyDomain { lower: 3, upper: 2 }));
        p.eq("family", sum_over("x", 1, 2, BinderTerm::Leaf(var("x"))).opaque_identity(7), Err(BinderError::WrongFamily { operation: "opaque_identity", family: BinderFamily::Structural }));
        p.eq("version-ok", check_version(BINDER_VERSION), Ok(()));
        p.eq("version-unknown", check_version(BINDER_VERSION + 1), Err(BinderError::UnknownVersion { version: BINDER_VERSION + 1 }));
    });
    p.case("opaque-seed", |p| {
        let limit = ScopedBinder { kind: BinderKind::Limit, family: BinderFamily::OpaqueSeeded, domain: BinderDomain::Symbolic { anchor: "x->0".to_string() }, bound: VariableId("x".to_string()), body: BinderTerm::Leaf(var("x")) };
        let first = limit.opaque_identity(41).expect("opaque family");
        p.eq("deterministic", first, limit.opaque_identity(41).expect("opaque family"));
        p.ne("seed-binds", first, limit.opaque_identity(42).expect("opaque family"));
    });
    p.finish();
}
