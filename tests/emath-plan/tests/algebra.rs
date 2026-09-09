//! Resolution algebra: serial associativity, identity neutrality,
//! left-biased alternatives, commutative parallel join, lifted refusal
//! collection, explicit fallback degradation. One probe, every law.

use emath_plan::{Facet, Lifted, QState, Step, fallback, parallel, serial};
use emath_test_harness::Probe;

fn capability(provider: &str, facets: &[Facet]) -> Step {
    Step::Capability {
        provider: provider.to_string(),
        discharges: facets.iter().copied().collect(),
        refusals: Vec::new(),
    }
}

#[test]
fn resolution_algebra() {
    let mut p = Probe::new(
        "serial is associative and identity-neutral; alt is left-biased; parallel commutes; lift names every refusal; fallback is explicit degradation",
    );
    let start = QState::full();
    p.case("serial-assoc", |p| {
        let a = capability("a", &[Facet::Kind]);
        let b = capability("b", &[Facet::Target]);
        let c = capability("c", &[Facet::Determinism]);
        p.eq(
            "left=right",
            serial(serial(a.clone(), b.clone()), c.clone()).apply(&start),
            serial(a, serial(b, c)).apply(&start),
        );
    });
    p.case("identity-neutral", |p| {
        let step = capability("a", &[Facet::Kind, Facet::Target]);
        p.eq(
            "id;step",
            serial(Step::Id, step.clone()).apply(&start),
            step.apply(&start),
        );
        p.eq(
            "step;id",
            serial(step.clone(), Step::Id).apply(&start),
            step.apply(&start),
        );
    });
    p.case("alt-left-biased", |p| {
        let refused = Step::refused("dead", vec!["E-PROV-512: no capability".into()]);
        let live = Step::compatible("live");
        let other = Step::compatible("other");
        match Step::Alt(vec![refused, live, other]).apply(&QState::full()) {
            None => {
                p.fail("apply", "a live arm must apply");
            }
            Some(application) => {
                p.eq("trace", application.trace, vec!["live".to_string()]);
                p.demand("resolved", application.state.is_resolved(), "alt must resolve");
            }
        }
    });
    p.case("parallel-commutes", |p| {
        let a = capability("a", &[Facet::Kind, Facet::Evidence]);
        let b = capability("b", &[Facet::Target]);
        match (
            parallel(a.clone(), b.clone()).apply(&start),
            parallel(b, a).apply(&start),
        ) {
            (Some(left), Some(right)) => {
                p.eq("state", left.state.clone(), right.state);
                p.eq(
                    "open",
                    left.state.open_facets(),
                    vec![Facet::Exactness, Facet::Determinism],
                );
            }
            other => {
                p.fail("apply", format!("both orders must apply, got {other:?}"));
            }
        }
    });
    p.case("lift-collects-refusals", |p| {
        let step = Step::Alt(vec![
            Step::refused("p1", vec!["E-PROV-515: no exactness".into()]),
            Step::refused("p2", vec!["E-PROV-514: wrong target".into()]),
        ]);
        match step.apply_total(&QState::full()) {
            Lifted::Refused { reasons } => {
                p.eq(
                    "reasons",
                    reasons,
                    vec![
                        "p1: E-PROV-515: no exactness".to_string(),
                        "p2: E-PROV-514: wrong target".to_string(),
                    ],
                );
            }
            Lifted::Applied(application) => {
                p.fail("refused", format!("refused arms must not apply: {application:?}"));
            }
        }
    });
    p.case("fallback-degraded", |p| {
        let step = fallback(
            Step::refused("primary", vec!["E-PROV-513: evidence ceiling".into()]),
            Step::compatible("secondary"),
        );
        match step.apply(&QState::full()) {
            None => {
                p.fail("apply", "fallback must apply");
            }
            Some(application) => {
                p.demand("degraded", application.degraded, "fallback must be explicit degradation");
                p.eq("trace", application.trace, vec!["secondary".to_string()]);
            }
        }
    });
    p.finish();
}
