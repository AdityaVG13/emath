//! Analogue-identity evaluation tests.

use emath_genesis::analogue::{
    ANALOGUE_NO_CLAIM, ANALOGUE_VERSION, AnalogueDomain, AnalogueError, AnalogueRequest,
    AnalogueVerdict, analogue_id, check_version,
};
use emath_genesis::binder::{BinderBudget, BinderKind, BinderTerm};
use emath_term::{SymbolId, Term, VariableId};
use emath_test_harness::Probe;

fn var(name: &str) -> Term {
    Term::Variable(VariableId(name.to_string()))
}

fn constant(text: &str) -> Term {
    Term::Constant(SymbolId(text.to_string()))
}

fn apply(op: &str, arguments: Vec<Term>) -> Term {
    Term::Apply { operator: SymbolId(op.to_string()), arguments }
}

fn identity(kind: BinderKind, domain: AnalogueDomain) -> AnalogueRequest {
    AnalogueRequest { kind, domain, budget: BinderBudget::default(), bound: VariableId("x".to_string()), body: BinderTerm::Leaf(var("x")) }
}

fn bits(value: f64) -> u64 {
    value.to_bits()
}

#[test]
fn analogue() {
    let mut p = Probe::new("analogue folds, quadratures, and differences compute; misuse refuses");
    p.case("happy-path", |p| {
        let sum = identity(BinderKind::Sum, AnalogueDomain::IntegerRange { lower: 1, upper: 4 }).evaluate().expect("sum");
        p.eq("sum", sum.value_bits, Some(bits(10.0)));
        p.eq("rule", sum.rule, "left-fold-sum");
        p.eq("verdict", sum.verdict, AnalogueVerdict::Computed);
        p.eq("spent", sum.budget_spent, 4);
        p.eq("partials", sum.partials.len(), 4);
        let product = identity(BinderKind::Product, AnalogueDomain::IntegerRange { lower: 1, upper: 4 }).evaluate().expect("product");
        p.eq("product", product.value_bits, Some(bits(24.0)));
        p.eq("rule", product.rule, "left-fold-product");
        let integral = identity(BinderKind::Integral, AnalogueDomain::Interval { lower: 0.0, upper: 1.0, n: 4 }).evaluate().expect("integral");
        p.eq("integral", integral.value_bits, Some(bits(0.5)));
        p.eq("rule", integral.rule, "composite-trapezoid");
        p.eq("spent", integral.budget_spent, 5);
        let square = AnalogueRequest { kind: BinderKind::Derivative, domain: AnalogueDomain::Difference { point: 3.0, h: 0.25 }, budget: BinderBudget::default(), bound: VariableId("x".to_string()), body: BinderTerm::Leaf(apply("*", vec![var("x"), var("x")])) }.evaluate().expect("derivative");
        p.eq("derivative", square.value_bits, Some(bits(6.0)));
        p.eq("rule", square.rule, "central-difference");
        p.eq("spent", square.budget_spent, 2);
        let limit = identity(BinderKind::Limit, AnalogueDomain::Approach { point: 0.0, samples: 4 }).evaluate().expect("limit");
        p.eq("verdict", limit.verdict, AnalogueVerdict::NoClaim);
        p.eq("canonical", limit.verdict.canonical(), ANALOGUE_NO_CLAIM);
        p.eq("value", limit.value_bits, None);
        p.eq("samples", limit.samples.len(), 4);
        p.eq("xs", limit.samples.iter().map(|s| s.x_bits).collect::<Vec<_>>(), vec![bits(0.5), bits(0.25), bits(0.125), bits(0.0625)]);
    });
    p.case("boundaries", |p| {
        p.eq("empty-range", identity(BinderKind::Sum, AnalogueDomain::IntegerRange { lower: 3, upper: 2 }).evaluate(), Err(AnalogueError::InvalidDomain { reason: "a>b" }));
        let single = identity(BinderKind::Sum, AnalogueDomain::IntegerRange { lower: 7, upper: 7 }).evaluate().expect("single-point fold");
        p.eq("single", single.value_bits, Some(bits(7.0)));
        p.eq("spent", single.budget_spent, 1);
        let empty_width = identity(BinderKind::Integral, AnalogueDomain::Interval { lower: 2.0, upper: 2.0, n: 1 }).evaluate().expect("zero-width interval");
        p.eq("zero-width", empty_width.value_bits, Some(bits(0.0)));
        let one_panel = identity(BinderKind::Integral, AnalogueDomain::Interval { lower: 0.0, upper: 2.0, n: 1 }).evaluate().expect("single interval");
        p.eq("panel", one_panel.value_bits, Some(bits(2.0)));
        p.eq("spent", one_panel.budget_spent, 2);
    });
    p.case("typed-refusals", |p| {
        p.eq("budget", identity(BinderKind::Sum, AnalogueDomain::IntegerRange { lower: 1, upper: 1000 }).evaluate(), Err(AnalogueError::BudgetExceeded { limit: 64 }));
        let tight = AnalogueRequest { budget: BinderBudget { max_terms: 8 }, ..identity(BinderKind::Sum, AnalogueDomain::IntegerRange { lower: 1, upper: 1000 }) };
        p.eq("tight", tight.evaluate(), Err(AnalogueError::BudgetExceeded { limit: 8 }));
        p.eq("n0", identity(BinderKind::Integral, AnalogueDomain::Interval { lower: 0.0, upper: 1.0, n: 0 }).evaluate(), Err(AnalogueError::InvalidDomain { reason: "n=0" }));
        p.eq("h0", identity(BinderKind::Derivative, AnalogueDomain::Difference { point: 0.0, h: 0.0 }).evaluate(), Err(AnalogueError::InvalidDomain { reason: "h<=0" }));
        p.eq("mismatch", identity(BinderKind::Sum, AnalogueDomain::Interval { lower: 0.0, upper: 1.0, n: 2 }).evaluate(), Err(AnalogueError::InvalidDomain { reason: "kind-domain-mismatch" }));
        p.eq("kind", identity(BinderKind::Custom("bigjoin".to_string()), AnalogueDomain::IntegerRange { lower: 1, upper: 2 }).evaluate(), Err(AnalogueError::UnsupportedKind { kind: "bigjoin".to_string() }));
        p.eq("version-ok", check_version(ANALOGUE_VERSION), Ok(()));
        p.eq("version-unknown", check_version(ANALOGUE_VERSION + 1), Err(AnalogueError::UnknownVersion { version: ANALOGUE_VERSION + 1 }));
    });
    p.case("non-finite-refused", |p| {
        p.eq("nan", identity(BinderKind::Integral, AnalogueDomain::Interval { lower: f64::NAN, upper: 1.0, n: 2 }).evaluate(), Err(AnalogueError::InvalidDomain { reason: "non-finite-bounds" }));
        p.eq("inf", identity(BinderKind::Integral, AnalogueDomain::Interval { lower: 0.0, upper: f64::INFINITY, n: 2 }).evaluate(), Err(AnalogueError::InvalidDomain { reason: "non-finite-bounds" }));
        p.eq("neg-inf", identity(BinderKind::Derivative, AnalogueDomain::Difference { point: f64::NEG_INFINITY, h: 1.0 }).evaluate(), Err(AnalogueError::InvalidDomain { reason: "non-finite-bounds" }));
        let huge = AnalogueRequest { budget: BinderBudget { max_terms: 16 }, ..identity(BinderKind::Integral, AnalogueDomain::Interval { lower: 0.0, upper: 1.0, n: u32::MAX }) };
        p.eq("huge", huge.evaluate(), Err(AnalogueError::BudgetExceeded { limit: 16 }));
    });
    p.case("approach-underflow", |p| {
        let absorbed = AnalogueRequest { budget: BinderBudget { max_terms: 64 }, ..identity(BinderKind::Limit, AnalogueDomain::Approach { point: 1.0, samples: 64 }) };
        p.eq("refuses", absorbed.evaluate(), Err(AnalogueError::InvalidDomain { reason: "approach-underflow" }));
    });
    p.case("receipts-deterministic", |p| {
        let request = identity(BinderKind::Integral, AnalogueDomain::Interval { lower: 0.0, upper: 1.0, n: 8 });
        let first = request.evaluate().expect("first").to_json();
        p.eq("stable", first.clone(), request.evaluate().expect("second").to_json());
        p.demand("brace", first.starts_with('{'), "receipt is JSON");
        p.contains("schema", &first, "\"schema\":\"emath.analogue\"");
        p.eq("id", analogue_id(&request), analogue_id(&request));
    });
    p.case("closed-forms", |p| {
        for n in 1_i64..=20 {
            let receipt = identity(BinderKind::Sum, AnalogueDomain::IntegerRange { lower: 1, upper: n }).evaluate().expect("sum");
            p.eq(format!("sum-{n}"), receipt.value_bits, Some(bits((n * (n + 1) / 2) as f64)));
        }
        let body = BinderTerm::Leaf(apply("+", vec![apply("*", vec![constant("2"), var("x")]), constant("3")]));
        for n in 1_u32..=8 {
            let receipt = AnalogueRequest { kind: BinderKind::Integral, domain: AnalogueDomain::Interval { lower: 1.0, upper: 4.0, n }, budget: BinderBudget::default(), bound: VariableId("x".to_string()), body: body.clone() }.evaluate().expect("trapezoid");
            p.eq(format!("trap-{n}"), receipt.value_bits, Some(bits(24.0)));
        }
    });
    p.finish();
}
