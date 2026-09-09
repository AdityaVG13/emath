//! capability tests migrated from the in-crate `#[cfg(test)]` module.

use emath_core::QualifiedName;
use emath_ir::capability::*;
use emath_test_harness::Probe;

#[test]
fn intent() {
    let mut p = Probe::new("capability tests migrated from the in-crate `#[cfg(test)]` module.");
    p.case("nanopass_projections_unit", |p| {

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
    p.eq("nanopass_projections_unit#1", rows.len(), 11);
    for (phase, (kind, status)) in rows.iter().enumerate() {
        let phase = u8::try_from(phase).unwrap_or(u8::MAX);
        match kind {
            ProjectionKind::Identity | ProjectionKind::Schema => {
                p.eq(format!("phase {phase}"), *status, ProjectionStatus::Generated);
            }
            ProjectionKind::Specification | ProjectionKind::Algorithm => {
                p.eq(format!("phase {phase}: biform rows not required for pure"), *status, ProjectionStatus::NotApplicable);
            }
            _ => {
                p.eq(format!("phase {phase}: missing required projection"), *status, ProjectionStatus::Refused);
            }
        }
    }
    let pass_list = nanopass::pass_list(&schema, CellClass::Pure);
    p.eq("nine required passes for pure", pass_list.len(), 9);
    for (phase, pass) in pass_list.iter().enumerate() {
        p.eq("nanopass_projections_unit#6", pass.phase, u8::try_from(phase).unwrap_or(u8::MAX));
        // Identity-affecting rows are exactly the ones hashed into the
        // cell identity; docs/assurance/evidence/evolution plus
        // reference/compilation are cosmetic.
        p.eq(format!("phase {} identity role", pass.phase), pass.identity_affecting, matches!(
                pass.kind,
                ProjectionKind::Identity | ProjectionKind::Schema | ProjectionKind::Semantics
            ));
    }

    });
    p.finish();
}

