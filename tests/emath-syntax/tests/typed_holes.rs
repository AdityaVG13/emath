//! Typed holes as durable objects (`f(x) = ?` with constraints).

use emath_syntax::{ExactnessStatus, HoleContinuation, expand_scratch, parse_str};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

use emath_test_harness::{Probe, boot};

#[test]
fn typed_holes() {
    boot();
    let mut probe = Probe::new("Typed holes as durable objects (`f(x) = ?` with constraints).");
    probe.case("typed_hole_parses_and_stays_open", |p| {
    let f0 = p.failures().len();

    let source = "f(x) = ?\nrequire f(0)=1\nrequire derivative(f)=f\nfind f\n";
    let expansion = expand_scratch(source);
    p.demand("1",expansion.expanded.contains("Hole") || expansion.expanded.contains("open hole"), format!(
        "{}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    p.demand("2",expansion
            .notes
            .iter()
            .any(|note| note.stability == ExactnessStatus::Open || note.inferred.contains("hole")), format!(
        "{:?}",
        expansion.notes
    ));
    if p.failures().len() != f0 { return; }
    let hole = expansion
        .holes
        .iter()
        .find(|hole| hole.name == "f")
        .expect("durable hole object for f");
    p.demand("3",hole.constraints.iter().any(|c| c.contains("f(0)")), format!(
        "{:?}",
        hole.constraints
    ));
    if p.failures().len() != f0 { return; }
    p.demand("4",hole.constraints.iter().any(|c| c.contains("derivative")), format!(
        "{:?}",
        hole.constraints
    ));
    if p.failures().len() != f0 { return; }
    p.demand("5",!hole.candidates.is_empty(), format!(
        "constrained hole must label candidates, not invent a solution"
    ));
    if p.failures().len() != f0 { return; }
    p.demand("6",matches!(hole.continuation, HoleContinuation::Search { .. }), format!(
        "{:?}",
        hole.continuation
    ));
    if p.failures().len() != f0 { return; }
    p.demand("7",!expansion.expanded.contains("exp("), format!(
        "must not invent f(x)=exp(x): {}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }
    let (_, diagnostics) = parse_str(source);
    p.demand("8",!diagnostics.has_errors(), format!(
        "hole must not be a parse bomb, got {:?}",
        diagnostics
            .errors()
            .map(|error| error.code)
            .collect::<Vec<_>>()
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("unconstrained_hole_records_rejection_not_invention", |p| {
    let f0 = p.failures().len();

    let expansion = expand_scratch("g(x) = ?\n");
    let hole = expansion
        .holes
        .iter()
        .find(|hole| hole.name == "g")
        .expect("g");
    p.demand("1",hole.constraints.is_empty(), stringify!(hole.constraints.is_empty()));
    if p.failures().len() != f0 { return; }
    p.demand("2",hole.candidates.is_empty(), format!( "{:?}", hole.candidates));
    if p.failures().len() != f0 { return; }
    p.demand("3",hole.rejections
            .iter()
            .any(|rejection| rejection.reason.contains("no solution is invented")), format!(
        "{:?}",
        hole.rejections
    ));
    if p.failures().len() != f0 { return; }
    p.eq("4", hole.continuation.clone(), HoleContinuation::Open);

    });
    probe.case("unconstrained_hole_claimed_exact_is_refused", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/typed_holes.emath");
    p.demand("1",has_error(source, "E-SYN-147"), stringify!(has_error(source, "E-SYN-147")));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
