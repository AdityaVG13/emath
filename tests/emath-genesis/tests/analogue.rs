//! Analogue-identity tests migrated from the in-crate `#[cfg(test)]`
//! module: every symbol they exercise is public crate surface.

use emath_genesis::analogue::{
    ANALOGUE_NO_CLAIM, ANALOGUE_VERSION, AnalogueDomain, AnalogueError, AnalogueRequest,
    AnalogueVerdict, analogue_id, check_version,
};
use emath_genesis::binder::{BinderBudget, BinderKind, BinderTerm};
use emath_term::{SymbolId, Term, VariableId};

fn var(name: &str) -> Term {
    Term::Variable(VariableId(name.to_string()))
}

fn constant(text: &str) -> Term {
    Term::Constant(SymbolId(text.to_string()))
}

fn apply(op: &str, arguments: Vec<Term>) -> Term {
    Term::Apply {
        operator: SymbolId(op.to_string()),
        arguments,
    }
}

fn identity(kind: BinderKind, domain: AnalogueDomain) -> AnalogueRequest {
    AnalogueRequest {
        kind,
        domain,
        budget: BinderBudget::default(),
        bound: VariableId("x".to_string()),
        body: BinderTerm::Leaf(var("x")),
    }
}

fn bits(value: f64) -> u64 {
    value.to_bits()
}

#[test]
fn happy_path_per_kind() {
    let sum = identity(
        BinderKind::Sum,
        AnalogueDomain::IntegerRange { lower: 1, upper: 4 },
    )
    .evaluate()
    .expect("sum");
    assert_eq!(sum.value_bits, Some(bits(10.0)));
    assert_eq!(sum.rule, "left-fold-sum");
    assert_eq!(sum.verdict, AnalogueVerdict::Computed);
    assert_eq!(sum.budget_spent, 4);
    assert_eq!(sum.partials.len(), 4);

    let product = identity(
        BinderKind::Product,
        AnalogueDomain::IntegerRange { lower: 1, upper: 4 },
    )
    .evaluate()
    .expect("product");
    assert_eq!(product.value_bits, Some(bits(24.0)));
    assert_eq!(product.rule, "left-fold-product");

    let integral = identity(
        BinderKind::Integral,
        AnalogueDomain::Interval {
            lower: 0.0,
            upper: 1.0,
            n: 4,
        },
    )
    .evaluate()
    .expect("integral");
    assert_eq!(integral.value_bits, Some(bits(0.5)));
    assert_eq!(integral.rule, "composite-trapezoid");
    assert_eq!(integral.budget_spent, 5);

    let square = AnalogueRequest {
        kind: BinderKind::Derivative,
        domain: AnalogueDomain::Difference {
            point: 3.0,
            h: 0.25,
        },
        budget: BinderBudget::default(),
        bound: VariableId("x".to_string()),
        body: BinderTerm::Leaf(apply("*", vec![var("x"), var("x")])),
    }
    .evaluate()
    .expect("derivative");
    assert_eq!(square.value_bits, Some(bits(6.0)));
    assert_eq!(square.rule, "central-difference");
    assert_eq!(square.budget_spent, 2);

    let limit = identity(
        BinderKind::Limit,
        AnalogueDomain::Approach {
            point: 0.0,
            samples: 4,
        },
    )
    .evaluate()
    .expect("limit");
    assert_eq!(limit.verdict, AnalogueVerdict::NoClaim);
    assert_eq!(limit.verdict.canonical(), ANALOGUE_NO_CLAIM);
    assert_eq!(limit.value_bits, None);
    assert_eq!(limit.samples.len(), 4);
    assert_eq!(limit.samples[0].x_bits, bits(0.5));
    assert_eq!(limit.samples[1].x_bits, bits(0.25));
    assert_eq!(limit.samples[2].x_bits, bits(0.125));
    assert_eq!(limit.samples[3].x_bits, bits(0.0625));
}

#[test]
fn boundary_empty_range_and_single_interval() {
    assert_eq!(
        identity(
            BinderKind::Sum,
            AnalogueDomain::IntegerRange { lower: 3, upper: 2 },
        )
        .evaluate(),
        Err(AnalogueError::InvalidDomain { reason: "a>b" })
    );

    let single = identity(
        BinderKind::Sum,
        AnalogueDomain::IntegerRange { lower: 7, upper: 7 },
    )
    .evaluate()
    .expect("single-point fold");
    assert_eq!(single.value_bits, Some(bits(7.0)));
    assert_eq!(single.budget_spent, 1);

    let empty_width = identity(
        BinderKind::Integral,
        AnalogueDomain::Interval {
            lower: 2.0,
            upper: 2.0,
            n: 1,
        },
    )
    .evaluate()
    .expect("zero-width interval");
    assert_eq!(empty_width.value_bits, Some(bits(0.0)));

    let one_panel = identity(
        BinderKind::Integral,
        AnalogueDomain::Interval {
            lower: 0.0,
            upper: 2.0,
            n: 1,
        },
    )
    .evaluate()
    .expect("single interval");
    assert_eq!(one_panel.value_bits, Some(bits(2.0)));
    assert_eq!(one_panel.budget_spent, 2);
}

#[test]
fn refusals_are_typed() {
    assert_eq!(
        identity(
            BinderKind::Sum,
            AnalogueDomain::IntegerRange {
                lower: 1,
                upper: 1000,
            },
        )
        .evaluate(),
        Err(AnalogueError::BudgetExceeded { limit: 64 })
    );
    let tight = AnalogueRequest {
        budget: BinderBudget { max_terms: 8 },
        ..identity(
            BinderKind::Sum,
            AnalogueDomain::IntegerRange {
                lower: 1,
                upper: 1000,
            },
        )
    };
    assert_eq!(
        tight.evaluate(),
        Err(AnalogueError::BudgetExceeded { limit: 8 })
    );

    assert_eq!(
        identity(
            BinderKind::Integral,
            AnalogueDomain::Interval {
                lower: 0.0,
                upper: 1.0,
                n: 0,
            },
        )
        .evaluate(),
        Err(AnalogueError::InvalidDomain { reason: "n=0" })
    );
    assert_eq!(
        identity(
            BinderKind::Derivative,
            AnalogueDomain::Difference { point: 0.0, h: 0.0 },
        )
        .evaluate(),
        Err(AnalogueError::InvalidDomain { reason: "h<=0" })
    );
    assert_eq!(
        identity(
            BinderKind::Sum,
            AnalogueDomain::Interval {
                lower: 0.0,
                upper: 1.0,
                n: 2,
            },
        )
        .evaluate(),
        Err(AnalogueError::InvalidDomain {
            reason: "kind-domain-mismatch"
        })
    );
    assert_eq!(
        identity(
            BinderKind::Custom("bigjoin".to_string()),
            AnalogueDomain::IntegerRange { lower: 1, upper: 2 },
        )
        .evaluate(),
        Err(AnalogueError::UnsupportedKind {
            kind: "bigjoin".to_string()
        })
    );
    assert_eq!(check_version(ANALOGUE_VERSION), Ok(()));
    assert_eq!(
        check_version(ANALOGUE_VERSION + 1),
        Err(AnalogueError::UnknownVersion {
            version: ANALOGUE_VERSION + 1
        })
    );
}

#[test]
fn malformed_nan_inf_bounds_and_huge_n_are_refused() {
    assert_eq!(
        identity(
            BinderKind::Integral,
            AnalogueDomain::Interval {
                lower: f64::NAN,
                upper: 1.0,
                n: 2,
            },
        )
        .evaluate(),
        Err(AnalogueError::InvalidDomain {
            reason: "non-finite-bounds"
        })
    );
    assert_eq!(
        identity(
            BinderKind::Integral,
            AnalogueDomain::Interval {
                lower: 0.0,
                upper: f64::INFINITY,
                n: 2,
            },
        )
        .evaluate(),
        Err(AnalogueError::InvalidDomain {
            reason: "non-finite-bounds"
        })
    );
    assert_eq!(
        identity(
            BinderKind::Derivative,
            AnalogueDomain::Difference {
                point: f64::NEG_INFINITY,
                h: 1.0,
            },
        )
        .evaluate(),
        Err(AnalogueError::InvalidDomain {
            reason: "non-finite-bounds"
        })
    );
    let huge = AnalogueRequest {
        budget: BinderBudget { max_terms: 16 },
        ..identity(
            BinderKind::Integral,
            AnalogueDomain::Interval {
                lower: 0.0,
                upper: 1.0,
                n: u32::MAX,
            },
        )
    };
    assert_eq!(
        huge.evaluate(),
        Err(AnalogueError::BudgetExceeded { limit: 16 })
    );
}

#[test]
fn limit_approach_refuses_instead_of_sampling_the_point() {
    // ulp absorption: 1.0 + 2^-53 rounds back to 1.0 (ties-to-even),
    // so a long enough approach toward 1.0 would evaluate the point
    // itself. The invariant is "strictly right of the point": refuse.
    let absorbed = AnalogueRequest {
        budget: BinderBudget { max_terms: 64 },
        ..identity(
            BinderKind::Limit,
            AnalogueDomain::Approach {
                point: 1.0,
                samples: 64,
            },
        )
    };
    assert_eq!(
        absorbed.evaluate(),
        Err(AnalogueError::InvalidDomain {
            reason: "approach-underflow"
        })
    );
}

#[test]
fn receipts_are_byte_identical_across_runs() {
    let request = identity(
        BinderKind::Integral,
        AnalogueDomain::Interval {
            lower: 0.0,
            upper: 1.0,
            n: 8,
        },
    );
    let first = request.evaluate().expect("first").to_json();
    let second = request.evaluate().expect("second").to_json();
    assert_eq!(first, second);
    assert!(first.starts_with('{'));
    assert!(first.contains("\"schema\":\"emath.analogue\""));
    assert_eq!(analogue_id(&request), analogue_id(&request));
}

#[test]
fn sum_of_identity_matches_closed_form() {
    for n in 1_i64..=20 {
        let receipt = identity(
            BinderKind::Sum,
            AnalogueDomain::IntegerRange { lower: 1, upper: n },
        )
        .evaluate()
        .expect("sum");
        let expected = (n * (n + 1) / 2) as f64;
        assert_eq!(
            receipt.value_bits,
            Some(bits(expected)),
            "sum 1..={n} must equal n(n+1)/2"
        );
    }
}

#[test]
fn trapezoid_of_linear_is_exact() {
    // f(x) = 2x + 3 on [1, 4]; ∫ = [x² + 3x]₁⁴ = 24.
    let body = BinderTerm::Leaf(apply(
        "+",
        vec![apply("*", vec![constant("2"), var("x")]), constant("3")],
    ));
    for n in 1_u32..=8 {
        let receipt = AnalogueRequest {
            kind: BinderKind::Integral,
            domain: AnalogueDomain::Interval {
                lower: 1.0,
                upper: 4.0,
                n,
            },
            budget: BinderBudget::default(),
            bound: VariableId("x".to_string()),
            body: body.clone(),
        }
        .evaluate()
        .expect("trapezoid");
        assert_eq!(
            receipt.value_bits,
            Some(bits(24.0)),
            "trapezoid of a line must be exact at n={n}"
        );
    }
}
