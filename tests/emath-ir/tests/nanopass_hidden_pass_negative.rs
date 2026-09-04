//! Negative control.
//!
//! A required projection skipped from the closure refuses with
//! `E-CELL-007` — one typed refusal per gap — so nothing is silently
//! left out of the pipeline lock (per-gap semantics, matching
//! `projection_planner.rs`).

#[test]
fn nanopass_hidden_pass_negative() {
    let schema = emath_ir::capability::CellSchema {
        name: emath_core::id::QualifiedName::single("std.math.softmax"),
        class: emath_ir::capability::CellClass::Pure,
        version: "1.0.0".into(),
        migration: emath_ir::capability::MigrationPolicy::Frozen,
        arity: 1,
        about: None,
    };
    // Supply only the schema projection; every other required row (the
    // seven universal rows minus planner-minted identity/schema, plus
    // reference and compilation for pure) is required-but-missing and
    // must refuse: one `E-CELL-007` per gap, never silently dropped.
    let refusals = emath_ir::capability::missing_required(
        &schema,
        &[(
            emath_ir::capability::ProjectionKind::Schema,
            emath_ir::capability::ProjectionStatus::Provided,
            None,
        )],
    );
    let wanted: std::collections::BTreeSet<_> = [
        emath_ir::capability::ProjectionKind::Semantics,
        emath_ir::capability::ProjectionKind::Docs,
        emath_ir::capability::ProjectionKind::Assurance,
        emath_ir::capability::ProjectionKind::Evidence,
        emath_ir::capability::ProjectionKind::Evolution,
        emath_ir::capability::ProjectionKind::Reference,
        emath_ir::capability::ProjectionKind::Compilation,
    ]
    .into_iter()
    .collect();
    assert_eq!(refusals.len(), 7, "one E-CELL-007 per missing required row");
    let got: std::collections::BTreeSet<_> = refusals
        .iter()
        .map(|r| {
            assert_eq!(r.code(), "E-CELL-007");
            match r {
                emath_ir::capability::ClosureRefusal::MissingRequired { projection, .. } => {
                    *projection
                }
                other => panic!("unexpected refusal: {other:?}"),
            }
        })
        .collect();
    assert_eq!(
        got, wanted,
        "every required-but-missing projection surfaces its refusal"
    );
}
