use emath_schema::parse_feature_capsule;
use emath_test_harness::{Probe, workspace_file};

#[test]
fn worked_capsules_and_one_path_gate() {
    let mut p = Probe::new(
        "worked capsules parse with their feature_id and trace to teaching examples; missing contract parts refuse; authored is never @generated",
    );
    for (capsule, example, id) in [
        (
            "language/spec/capabilities/core/add.emath",
            "language/examples/intro/add-exact.emath",
            "std.capability.math.add",
        ),
        (
            "language/spec/binders/core/sum.emath",
            "tests/fixtures/language/intro/sum-first-n.emath",
            "std.binder.sum",
        ),
    ] {
        let source = workspace_file(capsule);
        let (parsed, issues) = parse_feature_capsule(&source);
        p.demand(format!("{capsule}/issues"), issues.is_empty(), format!("{issues:?}"));
        match parsed {
            Some(parsed) => { p.eq(format!("{capsule}/id"), parsed.feature_id.as_str(), id); }
            None => { p.fail(format!("{capsule}/parse"), "capsule must parse"); }
        }
        p.contains(format!("{capsule}/example"), &workspace_file(example), "Expected");
    }
    let source = workspace_file("language/spec/capabilities/core/add.emath");
    for missing in ["feature_id:", "conformance:", "migration:", "agent:"] {
        let mutated = source
            .lines()
            .filter(|line| !line.trim_start().starts_with(missing))
            .collect::<Vec<_>>()
            .join("\n");
        p.demand(
            format!("missing-{missing}"),
            parse_feature_capsule(&mutated).0.is_none(),
            format!("missing {missing} must refuse"),
        );
    }
    p.demand("authored-not-generated", !source.contains("@generated"), "authored capsule is never generated");
    p.finish();
}
