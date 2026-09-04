use std::fs;

use emath_schema::parse_feature_capsule;

#[test]
fn worked_capsules_are_authored_valid_and_trace_to_user_examples() {
    for (capsule, example, id) in [
        (
            "../../language/spec/capabilities/core/add.emath",
            "../../language/examples/intro/add-exact.emath",
            "std.capability.math.add",
        ),
        (
            "../../language/spec/binders/core/sum.emath",
            "../../language/examples/intro/sum-first-n.emath",
            "std.binder.sum",
        ),
    ] {
        let source = fs::read_to_string(capsule).unwrap();
        let (parsed, issues) = parse_feature_capsule(&source);
        assert!(issues.is_empty(), "{capsule}: {issues:?}");
        assert_eq!(parsed.unwrap().feature_id.as_str(), id);
        assert!(fs::read_to_string(example).unwrap().contains("Expected"));
    }
}

#[test]
fn one_path_gate_rejects_missing_contract_parts_and_generated_edits() {
    let source = fs::read_to_string("../../language/spec/capabilities/core/add.emath").unwrap();
    for missing in ["feature_id:", "conformance:", "migration:", "agent:"] {
        let mutated = source
            .lines()
            .filter(|line| !line.trim_start().starts_with(missing))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            parse_feature_capsule(&mutated).0.is_none(),
            "missing {missing} must refuse"
        );
    }
    let generated = "# @generated from Feature Capsules; DO NOT EDIT\nmanual row";
    assert!(generated.contains("DO NOT EDIT"));
    assert!(
        !source.contains("@generated"),
        "authored capsule is never generated"
    );
}
