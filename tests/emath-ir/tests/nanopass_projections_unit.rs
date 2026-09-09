//! Positive unit test.
//!
//! Each projection pass is named, ordered, and replayable (task, not a
//! crate). Targeted verify: `cargo test -p emath-ir-tests
//! nanopass_projections`.

use emath_test_harness::Probe;
#[test]
fn intent() {
    let mut p = Probe::new("Positive unit test.");
    p.case("nanopass_projections_unit", |p| {

    let schema = emath_ir::capability::CellSchema {
        name: emath_core::id::QualifiedName::single("std.math.softmax"),
        class: emath_ir::capability::CellClass::Pure,
        version: "1.0.0".into(),
        migration: emath_ir::capability::MigrationPolicy::BumpAndNote {
            note: String::new(),
        },
        arity: 1,
        about: None,
    };
    let rows = emath_ir::capability::plan_cell_closure(&schema, &[]);
    // All eleven rows of the closed projection set are visible for the
    // pure cell: identity/schema are planner-minted `Generated`; every
    // other required row (including reference/compilation) is `Refused`;
    // the biform spec/algorithm rows are `NotApplicable` — visible, never
    // silently swallowed.
    p.eq("nanopass_projections_unit#1", rows.len(), 11);
    for (phase, (kind, status)) in rows.iter().enumerate() {
        match kind {
            emath_ir::capability::ProjectionKind::Identity
            | emath_ir::capability::ProjectionKind::Schema => {
                p.eq(format!("phase {phase}"), *status, emath_ir::capability::ProjectionStatus::Generated);
            }
            emath_ir::capability::ProjectionKind::Specification
            | emath_ir::capability::ProjectionKind::Algorithm => {
                p.eq(format!("phase {phase}: biform rows not required for pure"), *status, emath_ir::capability::ProjectionStatus::NotApplicable);
            }
            _ => { p.eq(format!("phase {phase}: missing required projection"), *status, emath_ir::capability::ProjectionStatus::Refused); }
        }
    }
    let pass_list =
        emath_ir::capability::nanopass::pass_list(&schema, emath_ir::capability::CellClass::Pure);
    p.eq("replayable pass list", pass_list.len(), 9);

    });
    p.finish();
}

