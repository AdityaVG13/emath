use std::path::Path;

use emath_exec_ir::language_image::load_language_distribution;
use emath_ir::ExprNode;
use emath_sema::CompilerSession;

#[test]
fn capsule_active_addition_resolves_by_feature_id_and_lowers_to_apply() {
    emath_syntax::install_source_parser();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = load_language_distribution(&root).unwrap();
    emath_sema::language::install_language_distribution(&distribution).unwrap();

    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned(
        "AddExact.emath",
        "emath function AddExact:\n    inputs:\n        u: Int\n    outputs:\n        result: Int\n    definitions:\n        result = 2 + 1\n",
    );
    assert!(!result.diagnostics.has_errors(), "{:?}", result.diagnostics);
    let capability = result
        .package
        .capabilities
        .iter()
        .position(|capability| capability.name.0 == "std.capability.math.add")
        .expect("capsule-active FeatureID is mounted");
    assert!(result.package.exprs.iter().any(|expression| {
        matches!(expression, ExprNode::Apply { capability: id, arguments } if id.0 as usize == capability && arguments.len() == 2)
    }));
}
