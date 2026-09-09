//! Intent-completion: `solve x^2 = 2` is a labeled candidate set, not a naked float.

use emath_syntax::{SolveIntent, SolveWorld, apply_solve_candidate, expand_scratch, parse_str};

fn has_error(text: &str, code: &str) -> bool {
    let (_, diagnostics) = parse_str(text);
    diagnostics.errors().any(|error| error.code == code)
}

use emath_test_harness::{Probe, boot};

#[test]
fn intent_completion_solve() {
    boot();
    let mut probe = Probe::new("Intent-completion: `solve x^2 = 2` is a labeled candidate set, not a naked float.");
    probe.case("solve_x2_eq_2_labels_real_complex_symbolic_numeric_modular", |p| {
    let f0 = p.failures().len();

    let expansion = expand_scratch("solve x^2 = 2\n");
    let labels: Vec<&str> = expansion.solve.menu().iter().map(|w| w.as_str()).collect();
    p.demand("1", (labels) == (vec!["real-pm", "complex", "modular", "symbolic", "numeric"]), format!("expected {:?}, got {:?}", (vec!["real-pm", "complex", "modular", "symbolic", "numeric"]), (labels)));
    if p.failures().len() != f0 { return; }
    p.eq("2", expansion.solve.menu().len(), 5);
    p.eq("3", SolveWorld::ALL.len(), 5);
    p.demand("4", SolveWorld::parse_label("quaternion") == None, format!("expected None, got {:?}", (SolveWorld::parse_label("quaternion"))));
    if p.failures().len() != f0 { return; }
    for world in SolveWorld::ALL {
        p.eq("5", SolveWorld::parse_label(world.as_str()), Some(world));
    }
    p.eq("6", SolveWorld::parse_label("real"), Some(SolveWorld::RealPm));
    p.eq("7", SolveWorld::parse_label("ℝ"), Some(SolveWorld::RealPm));
    p.eq("8", expansion.solve, SolveIntent::Unlabeled);
    let beginner: Vec<_> = expansion
        .solve
        .menu()
        .iter()
        .copied()
        .filter(|w| w.beginner_default())
        .collect();
    p.demand("9", (beginner) == (vec![SolveWorld::RealPm]), format!("expected {:?}, got {:?}", (vec![SolveWorld::RealPm]), (beginner)));
    if p.failures().len() != f0 { return; }
    p.demand("10",expansion
            .solve
            .menu()
            .iter()
            .all(|w| !expansion.solve.selected(*w)), format!(
        "unspecified domain must not silently select a candidate"
    ));
    if p.failures().len() != f0 { return; }
    p.eq("11", SolveWorld::Modular.holes(), &["modulus"]);
    p.demand("12",!expansion.expanded.contains("1.414"), format!(
        "must not emit a naked numeric root: {}",
        expansion.expanded
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.case("over_real_selects_the_beginner_candidate", |p| {
    let f0 = p.failures().len();

    let expansion = expand_scratch("solve x^2 = 2 over Real\n");
    p.eq("1", expansion.solve, SolveIntent::Over(SolveWorld::RealPm));
    p.eq("2", expansion.solve.menu().len(), 5);
    p.demand("3",SolveWorld::ALL
            .iter()
            .all(|w| expansion.solve.selected(*w) == (*w == SolveWorld::RealPm)), stringify!(SolveWorld::ALL
            .iter()
            .all(|w| expansion.solve.selected(*w) == (*w == SolveWorld::RealPm))));
    if p.failures().len() != f0 { return; }
    p.demand("4",SolveWorld::RealPm.beginner_default(), stringify!(SolveWorld::RealPm.beginner_default()));
    if p.failures().len() != f0 { return; }

    });
    probe.case("apply_real_pm_writes_domain_and_meaning_delta", |p| {
    let f0 = p.failures().len();

    let (rewritten, delta) =
        apply_solve_candidate("solve x^2 = 2\n", SolveWorld::RealPm).expect("apply");
    p.demand("1",rewritten.contains("over Real"), format!( "{rewritten}"));
    if p.failures().len() != f0 { return; }
    p.demand("2",delta.contains("real-pm"), format!( "{delta}"));
    if p.failures().len() != f0 { return; }
    p.demand("3",delta.contains("meaning:"), format!( "{delta}"));
    if p.failures().len() != f0 { return; }
    let expansion = expand_scratch(&rewritten);
    p.eq("4", expansion.solve, SolveIntent::Over(SolveWorld::RealPm));
    for world in SolveWorld::ALL {
        let (pinned, _) = apply_solve_candidate("solve x^2 = 2\n", world).expect("apply");
        let exp = expand_scratch(&pinned);
        p.eq("5", (exp.solve, parse_str(&pinned).0), (SolveIntent::Over(world), parse_str(&exp.expanded).0));
    }

    });
    probe.case("apply_modular_inserts_modulus_hole_not_mod_2", |p| {
    let f0 = p.failures().len();

    let (rewritten, _) =
        apply_solve_candidate("solve x^2 = 2\n", SolveWorld::Modular).expect("apply");
    p.demand("1",rewritten.contains("modulus = ?"), format!( "{rewritten}"));
    if p.failures().len() != f0 { return; }
    p.demand("2",rewritten.contains("over modular"), format!( "{rewritten}"));
    if p.failures().len() != f0 { return; }
    p.demand("3",!rewritten.contains("mod 2"), format!( "{rewritten}"));
    if p.failures().len() != f0 { return; }
    let expansion = expand_scratch(&rewritten);
    p.eq("4", expansion.solve, SolveIntent::Over(SolveWorld::Modular));
    p.eq("5", SolveWorld::Modular.holes(), &["modulus"]);

    });
    probe.case("unlabeled_unique_numeric_is_e_syn_151", |p| {
    let f0 = p.failures().len();

    let source = include_str!("../../../tests/invalid/solve_x2_eq_2_unlabeled.emath");
    p.demand("1",has_error(source, "E-SYN-151"), format!(
        "unlabeled unique numeric must refuse"
    ));
    if p.failures().len() != f0 { return; }

    });
    probe.finish();
}
