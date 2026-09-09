//! Capsule-active `+` resolves by `FeatureID` and lowers to a two-argument `Apply`.
use emath_ir::ExprNode;
use emath_test_harness::{boot, Probe, Source};

#[test]
fn capsule_feature_lowering() {
    boot();
    let mut p = Probe::new("capsule-active addition resolves by FeatureID and lowers to Apply");
    let result = Source::from_str(
        "AddExact",
        "emath function AddExact:\n    inputs:\n        u: Int\n    outputs:\n        result: Int\n    definitions:\n        result = 2 + 1\n",
    )
    .must_admit(&mut p);
    if !result.diagnostics.has_errors() {
        match result
            .package
            .capabilities
            .iter()
            .position(|capability| capability.name.0 == "std.capability.math.add")
        {
            Some(index) => {
                p.demand(
                    "lowers-to-apply",
                    result.package.exprs.iter().any(|expression| {
                        matches!(expression, ExprNode::Apply { capability: id, arguments } if id.0 as usize == index && arguments.len() == 2)
                    }),
                    "2 + 1 must lower to a two-argument Apply of the mounted capability",
                );
            }
            None => {
                p.fail(
                    "add-mounted",
                    "capsule-active FeatureID std.capability.math.add is not mounted",
                );
            }
        }
    }
    p.finish();
}
