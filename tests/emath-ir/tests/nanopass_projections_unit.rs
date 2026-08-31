//! Bead `emath-nanopass-projections-1d5jy` — positive unit test.
//!
//! Each projection pass is named, ordered, and replayable (task, not a
//! crate). Targeted verify: `cargo test -p emath-ir-tests
//! nanopass_projections`.

#[test]
fn nanopass_projections_unit() {
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
    assert_eq!(rows.len(), 11);
    for (phase, (kind, status)) in rows.iter().enumerate() {
        match kind {
            emath_ir::capability::ProjectionKind::Identity
            | emath_ir::capability::ProjectionKind::Schema => {
                assert_eq!(
                    *status,
                    emath_ir::capability::ProjectionStatus::Generated,
                    "phase {phase}"
                );
            }
            emath_ir::capability::ProjectionKind::Specification
            | emath_ir::capability::ProjectionKind::Algorithm => {
                assert_eq!(
                    *status,
                    emath_ir::capability::ProjectionStatus::NotApplicable,
                    "phase {phase}: biform rows not required for pure"
                );
            }
            _ => assert_eq!(
                *status,
                emath_ir::capability::ProjectionStatus::Refused,
                "phase {phase}: missing required projection"
            ),
        }
    }
    let pass_list = emath_ir::capability::nanopass::pass_list(
        &schema,
        emath_ir::capability::CellClass::Pure,
    );
    assert_eq!(pass_list.len(), 9, "replayable pass list");
}
