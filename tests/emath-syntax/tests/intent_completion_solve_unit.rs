//! Intent-completion: `solve x^2 = 2` is a labeled candidate set, not a naked float.

use emath_syntax::{apply_solve_candidate, expand_scratch, parse_str, SolveIntent, SolveWorld};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

#[test]
fn solve_x2_eq_2_labels_real_complex_symbolic_numeric_modular() {
    let expansion = expand_scratch("solve x^2 = 2\n");
    let labels: Vec<&str> = expansion.solve.menu().iter().map(|w| w.as_str()).collect();
    assert_eq!(
        labels,
        vec!["real-pm", "complex", "modular", "symbolic", "numeric"]
    );
    assert_eq!(expansion.solve.menu().len(), 5);
    assert_eq!(SolveWorld::ALL.len(), 5);
    assert_eq!(SolveWorld::parse_label("quaternion"), None);
    for world in SolveWorld::ALL {
        assert_eq!(SolveWorld::parse_label(world.as_str()), Some(world));
    }
    assert_eq!(SolveWorld::parse_label("real"), Some(SolveWorld::RealPm));
    assert_eq!(SolveWorld::parse_label("ℝ"), Some(SolveWorld::RealPm));
    assert_eq!(expansion.solve, SolveIntent::Unlabeled);
    let beginner: Vec<_> = expansion
        .solve
        .menu()
        .iter()
        .copied()
        .filter(|w| w.beginner_default())
        .collect();
    assert_eq!(beginner, vec![SolveWorld::RealPm], "{:?}", expansion.solve);
    assert!(
        expansion
            .solve
            .menu()
            .iter()
            .all(|w| !expansion.solve.selected(*w)),
        "unspecified domain must not silently select a candidate"
    );
    assert_eq!(SolveWorld::Modular.holes(), &["modulus"]);
    assert!(
        !expansion.expanded.contains("1.414"),
        "must not emit a naked numeric root: {}",
        expansion.expanded
    );
}

#[test]
fn over_real_selects_the_beginner_candidate() {
    let expansion = expand_scratch("solve x^2 = 2 over Real\n");
    assert_eq!(expansion.solve, SolveIntent::Over(SolveWorld::RealPm));
    assert_eq!(expansion.solve.menu().len(), 5);
    assert!(SolveWorld::ALL
        .iter()
        .all(|w| expansion.solve.selected(*w) == (*w == SolveWorld::RealPm)));
    assert!(SolveWorld::RealPm.beginner_default());
}

#[test]
fn apply_real_pm_writes_domain_and_meaning_delta() {
    let (rewritten, delta) =
        apply_solve_candidate("solve x^2 = 2\n", SolveWorld::RealPm).expect("apply");
    assert!(rewritten.contains("over Real"), "{rewritten}");
    assert!(delta.contains("real-pm"), "{delta}");
    assert!(delta.contains("meaning:"), "{delta}");
    let expansion = expand_scratch(&rewritten);
    assert_eq!(expansion.solve, SolveIntent::Over(SolveWorld::RealPm));
    for world in SolveWorld::ALL {
        let (pinned, _) = apply_solve_candidate("solve x^2 = 2\n", world).expect("apply");
        let exp = expand_scratch(&pinned);
        assert_eq!(
            (exp.solve, parse_str(&pinned).0),
            (SolveIntent::Over(world), parse_str(&exp.expanded).0),
            "{world:?}\nexpanded:\n{}",
            exp.expanded
        );
    }
}

#[test]
fn apply_modular_inserts_modulus_hole_not_mod_2() {
    let (rewritten, _) =
        apply_solve_candidate("solve x^2 = 2\n", SolveWorld::Modular).expect("apply");
    assert!(rewritten.contains("modulus = ?"), "{rewritten}");
    assert!(rewritten.contains("over modular"), "{rewritten}");
    assert!(!rewritten.contains("mod 2"), "{rewritten}");
    let expansion = expand_scratch(&rewritten);
    assert_eq!(expansion.solve, SolveIntent::Over(SolveWorld::Modular));
    assert_eq!(SolveWorld::Modular.holes(), &["modulus"]);
}

#[test]
fn unlabeled_unique_numeric_is_e_syn_151() {
    let source = include_str!("../../../tests/invalid/solve_x2_eq_2_unlabeled.emath");
    assert!(
        has_error(source, "E-SYN-151"),
        "unlabeled unique numeric must refuse"
    );
}
