//! Rename must migrate hoist/bind provenance with the section.

use emath_ir::kind_schema::KindSchema;
use emath_schema::{apply_lowering, is_bound, LowerOp};
use emath_test_harness::Probe;

#[test]
fn rename_migrates_hoist_aliases_and_bind() {
    let mut p = Probe::new("rename keeps hoist aliases and bind on the new section name");
    let mut core = KindSchema::core_function();
    core.set_name("lowering-fixture");
    core.insert_section(
        "equations",
        emath_ir::kind_schema::SectionSchema {
            repeat: emath_ir::kind_schema::RepeatPolicy::AtMostOne,
            payload: emath_ir::kind_schema::PayloadPolicy::Suite,
            has_default: false,
        },
    );
    match apply_lowering(
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
    ) {
        Err(error) => { p.fail("lower", format!("hoist+bind+rename admitted, got {error:?}")); }
        Ok(report) => {
            p.demand("old-gone", report.schema.section("equations").is_none(), "old section name must be gone");
            p.demand("new-present", report.schema.section("eqs").is_some(), "renamed section must exist");
            match report.schema.default_for("admission.eqs") {
                None => { p.fail("admission.eqs", "admission.eqs must exist"); }
                Some(admission) => {
                    p.demand(
                        "hoist-alias",
                        admission.split(',').any(|part| part == "rates"),
                        format!("hoist alias `rates` must survive rename, got {admission:?}"),
                    );
                    p.demand(
                        "rename-source",
                        admission.split(',').any(|part| part == "equations"),
                        format!("rename source must be recorded, got {admission:?}"),
                    );
                }
            }
            p.demand(
                "orphan-gone",
                report.schema.default_for("admission.equations").is_none(),
                "orphan admission.equations must be removed",
            );
            p.demand("bound-new", is_bound(&report.schema, "eqs"), "bind must move with the renamed section");
            p.demand("unbound-old", !is_bound(&report.schema, "equations"), "bind must not linger on the removed name");
        }
    }
    p.finish();
}
