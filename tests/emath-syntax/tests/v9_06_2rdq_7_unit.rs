//! Typed holes as durable objects (`f(x) = ?` with constraints).

use emath_syntax::{ExactnessStatus, HoleContinuation, expand_scratch, parse_str};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

#[test]
fn typed_hole_parses_and_stays_open() {
    let source = "f(x) = ?\nrequire f(0)=1\nrequire derivative(f)=f\nfind f\n";
    let expansion = expand_scratch(source);
    assert!(
        expansion.expanded.contains("Hole") || expansion.expanded.contains("open hole"),
        "{}",
        expansion.expanded
    );
    assert!(
        expansion
            .notes
            .iter()
            .any(|note| note.stability == ExactnessStatus::Open || note.inferred.contains("hole")),
        "{:?}",
        expansion.notes
    );
    let hole = expansion
        .holes
        .iter()
        .find(|hole| hole.name == "f")
        .expect("durable hole object for f");
    assert!(
        hole.constraints.iter().any(|c| c.contains("f(0)")),
        "{:?}",
        hole.constraints
    );
    assert!(
        hole.constraints.iter().any(|c| c.contains("derivative")),
        "{:?}",
        hole.constraints
    );
    assert!(
        !hole.candidates.is_empty(),
        "constrained hole must label candidates, not invent a solution"
    );
    assert!(
        matches!(hole.continuation, HoleContinuation::Search { .. }),
        "{:?}",
        hole.continuation
    );
    assert!(
        !expansion.expanded.contains("exp("),
        "must not invent f(x)=exp(x): {}",
        expansion.expanded
    );
    let (_, diagnostics) = parse_str(source);
    assert!(
        !diagnostics.has_errors(),
        "hole must not be a parse bomb, got {:?}",
        diagnostics
            .errors()
            .map(|error| error.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn unconstrained_hole_records_rejection_not_invention() {
    let expansion = expand_scratch("g(x) = ?\n");
    let hole = expansion
        .holes
        .iter()
        .find(|hole| hole.name == "g")
        .expect("g");
    assert!(hole.constraints.is_empty());
    assert!(hole.candidates.is_empty(), "{:?}", hole.candidates);
    assert!(
        hole.rejections
            .iter()
            .any(|rejection| rejection.reason.contains("no solution is invented")),
        "{:?}",
        hole.rejections
    );
    assert_eq!(hole.continuation, HoleContinuation::Open);
}

#[test]
fn unconstrained_hole_claimed_exact_is_refused() {
    let source = include_str!("../../../tests/invalid/v9_06_2rdq_7.emath");
    assert!(has_error(source, "E-SYN-147"));
}
