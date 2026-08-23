//! Rename must migrate hoist/bind provenance with the section.

use emath_ir::kind_schema::KindSchema;
use emath_schema::{apply_lowering, is_bound, LowerOp};

#[test]
fn rename_migrates_hoist_aliases_and_bind() {
    let core = KindSchema::core_model();
    let report = apply_lowering(
        &core,
        &[
            LowerOp::Hoist {
                from: "rates".into(),
                into: "equations".into(),
            },
            LowerOp::Bind {
                section: "equations".into(),
                to: "Eq".into(),
            },
            LowerOp::Rename {
                from: "equations".into(),
                to: "eqs".into(),
            },
        ],
    )
    .expect("hoist+bind+rename admitted");

    assert!(
        report.schema.section("equations").is_none(),
        "old section name must be gone"
    );
    assert!(
        report.schema.section("eqs").is_some(),
        "renamed section must exist"
    );

    let admission = report
        .schema
        .default_for("admission.eqs")
        .expect("admission.eqs");
    assert!(
        admission.split(',').any(|part| part == "rates"),
        "hoist alias `rates` must survive rename, got {admission:?}"
    );
    assert!(
        admission.split(',').any(|part| part == "equations"),
        "rename source must be recorded, got {admission:?}"
    );
    assert!(
        report.schema.default_for("admission.equations").is_none(),
        "orphan admission.equations must be removed"
    );

    assert!(
        is_bound(&report.schema, "eqs"),
        "bind must move with the renamed section"
    );
    assert!(
        !is_bound(&report.schema, "equations"),
        "bind must not linger on the removed name"
    );
}
