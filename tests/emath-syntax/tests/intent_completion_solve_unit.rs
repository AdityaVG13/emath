//! Intent-completion: `solve x^2 = 2` is a labeled candidate set, not a naked float.

use emath_syntax::{apply_solve_candidate, expand_scratch, parse_str};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

#[test]
fn solve_x2_eq_2_labels_real_complex_symbolic_numeric_modular() {
    let expansion = expand_scratch("solve x^2 = 2\n");
    let labels: Vec<&str> = expansion
        .solve_candidates
        .iter()
        .map(|c| c.label.as_str())
        .collect();
    for need in ["real-pm", "complex", "modular", "symbolic", "numeric"] {
        assert!(labels.contains(&need), "missing `{need}` in {labels:?}");
    }
    assert!(
        expansion
            .solve_candidates
            .iter()
            .any(|c| c.beginner_default && c.label == "real-pm"),
        "beginner default must be visible, got {:?}",
        expansion.solve_candidates
    );
    assert!(
        expansion.solve_candidates.iter().all(|c| !c.selected),
        "unspecified domain must not silently select a candidate"
    );
    let modular = expansion
        .solve_candidates
        .iter()
        .find(|c| c.label == "modular")
        .expect("modular");
    assert_eq!(modular.holes, vec!["modulus".to_string()]);
    assert!(
        !expansion.expanded.contains("1.414"),
        "must not emit a naked numeric root: {}",
        expansion.expanded
    );
}

#[test]
fn over_real_selects_the_beginner_candidate() {
    let expansion = expand_scratch("solve x^2 = 2 over Real\n");
    let real = expansion
        .solve_candidates
        .iter()
        .find(|c| c.label == "real-pm")
        .expect("real-pm");
    assert!(real.selected);
    assert!(real.beginner_default);
}

#[test]
fn apply_real_pm_writes_domain_and_meaning_delta() {
    let (rewritten, delta) = apply_solve_candidate("solve x^2 = 2\n", "real-pm").expect("apply");
    assert!(rewritten.contains("over Real"), "{rewritten}");
    assert!(delta.contains("real-pm"), "{delta}");
    assert!(delta.contains("meaning:"), "{delta}");
    let expansion = expand_scratch(&rewritten);
    assert!(
        expansion
            .solve_candidates
            .iter()
            .any(|c| c.label == "real-pm" && c.selected)
    );
}

#[test]
fn apply_modular_inserts_modulus_hole_not_mod_2() {
    let (rewritten, _) = apply_solve_candidate("solve x^2 = 2\n", "modular").expect("apply");
    assert!(rewritten.contains("modulus = ?"), "{rewritten}");
    assert!(rewritten.contains("over modular"), "{rewritten}");
    assert!(!rewritten.contains("mod 2"), "{rewritten}");
    let expansion = expand_scratch(&rewritten);
    let modular = expansion
        .solve_candidates
        .iter()
        .find(|c| c.label == "modular")
        .expect("modular");
    assert!(modular.selected);
    assert_eq!(modular.holes, vec!["modulus".to_string()]);
}

#[test]
fn unlabeled_unique_numeric_is_e_syn_151() {
    let source = include_str!("../../../tests/invalid/solve_x2_eq_2_unlabeled.emath");
    assert!(
        has_error(source, "E-SYN-151"),
        "unlabeled unique numeric must refuse"
    );
}
