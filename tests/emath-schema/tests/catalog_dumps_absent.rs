use emath_test_harness::{Probe, workspace_path};

#[test]
fn catalog_dumps_do_not_ship_under_language_spec() {
    let mut p = Probe::new(
        "catalog field packs, worlds, kinds, and MSC dumps stay out of language/spec",
    );
    let dumps = [
        "language/spec/field_packs/wave16-algebra-number-theory.emath",
        "language/spec/field_packs/msc2020.emath",
        "language/spec/kinds/geometry.emath",
        "language/spec/kinds/physics.emath",
        "language/spec/kinds/causal-social.emath",
        "language/spec/kinds/workflow.emath",
    ];
    for relative in dumps {
        p.demand(relative, !workspace_path(relative).exists(), "must not ship");
    }
    for number in 7..=20 {
        let relative = format!("language/spec/field_packs/wave16-math-{number}.emath");
        p.demand(&relative, !workspace_path(&relative).exists(), "must not ship");
    }
    for number in 21..=31 {
        let relative = format!("language/spec/worlds/wave16-worlds-{number}.emath");
        p.demand(&relative, !workspace_path(&relative).exists(), "must not ship");
    }
    p.finish();
}
