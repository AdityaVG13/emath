//! capability tests migrated from the in-crate `#[cfg(test)]` module.

use emath_core::QualifiedName;
use emath_ir::capability::*;

#[test]
fn nanopass_projections_unit() {
    let schema = CellSchema {
        name: QualifiedName::single("std.math.softmax"),
        class: CellClass::Pure,
        version: "1.0.0".into(),
        migration: MigrationPolicy::BumpAndNote {
            note: String::new(),
        },
        arity: 1,
        about: None,
    };
    let supplied = Vec::new();
    let rows = plan_cell_closure(&schema, &supplied);
    // All eleven rows of the closed projection set are visible for the
    // pure cell: identity/schema are planner-minted `Generated`; every
    // required row (including reference and compilation) is `Refused` —
    // never silently swallowed; the biform spec/algorithm rows are
    // `NotApplicable` (pure does not require them), shown, never hidden.
    assert_eq!(rows.len(), 11);
    for (phase, (kind, status)) in rows.iter().enumerate() {
        let phase = u8::try_from(phase).unwrap_or(u8::MAX);
        match kind {
            ProjectionKind::Identity | ProjectionKind::Schema => {
                assert_eq!(*status, ProjectionStatus::Generated, "phase {phase}");
            }
            ProjectionKind::Specification | ProjectionKind::Algorithm => {
                assert_eq!(
                    *status,
                    ProjectionStatus::NotApplicable,
                    "phase {phase}: biform rows not required for pure"
                );
            }
            _ => {
                assert_eq!(
                    *status,
                    ProjectionStatus::Refused,
                    "phase {phase}: missing required projection"
                );
            }
        }
    }
    let pass_list = nanopass::pass_list(&schema, CellClass::Pure);
    assert_eq!(pass_list.len(), 9, "nine required passes for pure");
    for (phase, pass) in pass_list.iter().enumerate() {
        assert_eq!(pass.phase, u8::try_from(phase).unwrap_or(u8::MAX));
        // Identity-affecting rows are exactly the ones hashed into the
        // cell identity; docs/assurance/evidence/evolution plus
        // reference/compilation are cosmetic.
        assert_eq!(
            pass.identity_affecting,
            matches!(
                pass.kind,
                ProjectionKind::Identity | ProjectionKind::Schema | ProjectionKind::Semantics
            ),
            "phase {} identity role",
            pass.phase
        );
    }
}
