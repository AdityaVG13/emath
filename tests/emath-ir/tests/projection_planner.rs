//! Projection planner and per-class closure
//! matrix — a cell is not done because it compiles.
//!
//! The planner lives in the capability layer (zero core delta: no core
//! enum grows). It emits the five statuses
//! (generated/provided/provider/not-applicable/refused) over the closed
//! projection set (identity, schema, semantics, docs, assurance, evidence,
//! evolution; plus reference/compilation for pure cells). Missing required
//! projections block stable as visible `E-CELL-007` refusals, and docs
//! cannot drift from the cell identity (`E-CELL-008`).

use emath_core::QualifiedName;
use emath_ir::{
    AdmissionRefusal, CellClass, CellSchema, MigrationPolicy, ProjectionKind, ProjectionStatus,
    admit_cell, cell_id, missing_required, plan_cell_closure, required_projections,
};

fn pure_cell() -> CellSchema {
    CellSchema {
        name: QualifiedName::single("std.tensor.softmax"),
        class: CellClass::Pure,
        version: "1.0.0".into(),
        migration: MigrationPolicy::Frozen,
        arity: 1,
        about: Some("stable maximum of exp(x - max(x))".into()),
    }
}

fn provider_cell() -> CellSchema {
    CellSchema {
        name: QualifiedName::single("sim.engine.integrate"),
        class: CellClass::Provider,
        version: "1.0.0".into(),
        migration: MigrationPolicy::Frozen,
        arity: 2,
        about: Some("delegates to a named integration provider".into()),
    }
}

/// Full supplied set for the pure cell: everything the planner does not
/// generate itself, bound to the cell identity where docs are concerned.
fn pure_supplied() -> Vec<(ProjectionKind, ProjectionStatus, Option<String>)> {
    let id = cell_id(&pure_cell()).0;
    vec![
        (ProjectionKind::Semantics, ProjectionStatus::Provided, None),
        (ProjectionKind::Docs, ProjectionStatus::Provided, Some(id)),
        (ProjectionKind::Assurance, ProjectionStatus::Provided, None),
        (ProjectionKind::Evidence, ProjectionStatus::Provided, None),
        (ProjectionKind::Evolution, ProjectionStatus::Provided, None),
        (ProjectionKind::Reference, ProjectionStatus::Generated, None),
        (
            ProjectionKind::Compilation,
            ProjectionStatus::Generated,
            None,
        ),
    ]
}

#[test]
fn pure_cell_full_closure_is_stable() {
    let schema = pure_cell();
    admit_cell(&schema).expect("pure cell admits (fjxh.2 seam)");

    let refusals = missing_required(&schema, &pure_supplied());
    assert!(
        refusals.is_empty(),
        "full closure must be stable, got: {refusals:?}"
    );

    // Planner output over the closed set: identity/schema are minted by
    // the planner itself (Generated); the supplied statuses pass through;
    // nothing is missing, so nothing is Refused.
    let plan = plan_cell_closure(&schema, &pure_supplied());
    assert_eq!(
        plan.iter().find(|(k, _)| *k == ProjectionKind::Identity),
        Some(&(ProjectionKind::Identity, ProjectionStatus::Generated))
    );
    assert_eq!(
        plan.iter().find(|(k, _)| *k == ProjectionKind::Schema),
        Some(&(ProjectionKind::Schema, ProjectionStatus::Generated))
    );
    assert_eq!(
        plan.iter().find(|(k, _)| *k == ProjectionKind::Semantics),
        Some(&(ProjectionKind::Semantics, ProjectionStatus::Provided))
    );
    assert!(
        !plan.iter().any(|(_, s)| *s == ProjectionStatus::Refused),
        "closed plan carries no refusals"
    );
}

#[test]
fn pure_cell_without_reference_blocks_stable() {
    // THE unit: a pure cell without its reference projection is a
    // visible refusal, never a silent success. Drop Reference AND
    // Compilation (both required for pure) and both gaps surface.
    let schema = pure_cell();
    let supplied: Vec<_> = pure_supplied()
        .into_iter()
        .filter(|(k, _, _)| !matches!(k, ProjectionKind::Reference | ProjectionKind::Compilation))
        .collect();

    let refusals = missing_required(&schema, &supplied);
    assert_eq!(
        refusals.len(),
        2,
        "both pure-only gaps surface: {refusals:?}"
    );
    assert!(refusals.iter().any(|r| r.code() == "E-CELL-007"));
    assert!(
        refusals
            .iter()
            .any(|r| r.cell_name() == "std.tensor.softmax"),
        "refusal names the cell"
    );

    // The plan marks the gaps Refused (blocks stable) while the rest of
    // the closure stays closed.
    let plan = plan_cell_closure(&schema, &supplied);
    assert_eq!(
        plan.iter().find(|(k, _)| *k == ProjectionKind::Reference),
        Some(&(ProjectionKind::Reference, ProjectionStatus::Refused))
    );
}

#[test]
fn drifted_docs_rejected() {
    // Docs cannot drift from the cell identity: a docs projection bound to
    // a stale CellId refuses (E-CELL-008), and unbound docs refuse too.
    let schema = pure_cell();
    let stale = "fnv1a64:0000000000000000".to_string();

    let mut supplied = pure_supplied();
    for entry in supplied.iter_mut() {
        if entry.0 == ProjectionKind::Docs {
            entry.2 = Some(stale.clone());
        }
    }
    let refusals = missing_required(&schema, &supplied);
    assert!(
        refusals.iter().any(|r| r.code() == "E-CELL-008"),
        "stale docs binding drifts from cell id: {refusals:?}"
    );

    let mut unbound = pure_supplied();
    for entry in unbound.iter_mut() {
        if entry.0 == ProjectionKind::Docs {
            entry.2 = None;
        }
    }
    assert!(
        missing_required(&schema, &unbound)
            .iter()
            .any(|r| r.code() == "E-CELL-008"),
        "docs must declare the cell id they document"
    );
}

#[test]
fn provider_matrix_not_applicable_and_boundary() {
    // Per-class closure matrix: only pure cells require reference and
    // compilation. A provider cell's plan shows them NotApplicable (not
    // refused), its semantics may be Provider-delegated, and the seven
    // universal projections stay required.
    let schema = provider_cell();
    admit_cell(&schema).expect("provider cell admits");

    assert_eq!(
        required_projections(CellClass::Pure).len(),
        9,
        "pure: 7 universal + reference + compilation"
    );
    assert_eq!(
        required_projections(CellClass::Provider).len(),
        7,
        "provider: 7 universal only"
    );
    assert_eq!(
        required_projections(CellClass::Theory).len(),
        7,
        "theory: 7 universal only"
    );

    let id = cell_id(&schema).0;
    let supplied = vec![
        (ProjectionKind::Semantics, ProjectionStatus::Provider, None),
        (ProjectionKind::Docs, ProjectionStatus::Provided, Some(id)),
        (ProjectionKind::Assurance, ProjectionStatus::Provided, None),
        (ProjectionKind::Evidence, ProjectionStatus::Provided, None),
        (ProjectionKind::Evolution, ProjectionStatus::Provided, None),
    ];
    assert!(
        missing_required(&schema, &supplied).is_empty(),
        "provider closure is stable without reference/compilation"
    );

    let plan = plan_cell_closure(&schema, &supplied);
    assert_eq!(
        plan.iter().find(|(k, _)| *k == ProjectionKind::Reference),
        Some(&(ProjectionKind::Reference, ProjectionStatus::NotApplicable))
    );
    assert_eq!(
        plan.iter().find(|(k, _)| *k == ProjectionKind::Compilation),
        Some(&(ProjectionKind::Compilation, ProjectionStatus::NotApplicable))
    );
    assert_eq!(
        plan.iter().find(|(k, _)| *k == ProjectionKind::Semantics),
        Some(&(ProjectionKind::Semantics, ProjectionStatus::Provider))
    );

    // Boundary: dropping a universal projection (evidence) from a provider
    // cell is still a visible refusal — NotApplicable never swallows a
    // required projection.
    let hole: Vec<_> = supplied
        .iter()
        .cloned()
        .filter(|(k, _, _)| k != &ProjectionKind::Evidence)
        .collect();
    let refusals = missing_required(&schema, &hole);
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert_eq!(refusals[0].code(), "E-CELL-007");
}
