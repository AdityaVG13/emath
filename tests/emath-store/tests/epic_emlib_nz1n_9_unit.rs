use std::sync::Arc;

use emath_core::{MeaningId, ObjectId};
use emath_store::{
    MergeAction, ObjectDraft, ObjectGraph, ObjectKind, Reconciliation, Space, SpaceError,
};

fn put(graph: &mut ObjectGraph, kind: ObjectKind, meaning: &str) -> ObjectId {
    graph
        .put(ObjectDraft {
            kind,
            meaning_id: MeaningId::from_bytes(meaning.as_bytes()),
            semantic_payload: meaning.as_bytes().to_vec(),
            presentation: None,
        })
        .unwrap()
}

#[test]
fn semantic_merge_retains_conflicting_meanings_and_independent_evidence() {
    let mut graph = ObjectGraph::default();
    let theorem_a = put(&mut graph, ObjectKind::Cell, "theorem-a");
    let theorem_b = put(&mut graph, ObjectKind::Cell, "theorem-b");
    let evidence_a = put(&mut graph, ObjectKind::Proof, "evidence-a");
    let evidence_b = put(&mut graph, ObjectKind::Proof, "evidence-b");
    let graph = Arc::new(graph);

    let mut base = Space::new("base", Arc::clone(&graph)).unwrap();
    base.bind_alias("theorem", theorem_a.clone()).unwrap();
    let ancestor = base.snapshot().unwrap();

    let mut left = base.branch("left").unwrap();
    left.set_lock_root(Some(ancestor.id.clone()));
    left.bind_alias("theorem", theorem_b.clone()).unwrap();
    left.bind_alias("evidence", evidence_a.clone()).unwrap();

    let mut right = base.branch("right").unwrap();
    right.set_lock_root(Some(ancestor.id.clone()));
    right.bind_alias("evidence", evidence_b.clone()).unwrap();

    let (merged, receipt) =
        Space::semantic_merge("merged", &ancestor, &left, &right, Vec::new()).unwrap();
    assert_eq!(merged.alias("theorem").unwrap().len(), 2);
    assert!(merged.alias("theorem").unwrap().contains(&theorem_a));
    assert!(merged.alias("theorem").unwrap().contains(&theorem_b));
    assert_eq!(merged.alias("evidence").unwrap().len(), 2);
    assert!(merged.alias("evidence").unwrap().contains(&evidence_a));
    assert!(merged.alias("evidence").unwrap().contains(&evidence_b));
    assert!(receipt.id.as_str().starts_with("emath:merge:v1:"));
}

#[test]
fn reconciliation_is_explicit_and_common_ancestor_is_required() {
    let mut graph = ObjectGraph::default();
    let theorem_a = put(&mut graph, ObjectKind::Cell, "theorem-a");
    let theorem_b = put(&mut graph, ObjectKind::Cell, "theorem-b");
    let choice = put(&mut graph, ObjectKind::Method, "explicit-choice");
    let graph = Arc::new(graph);

    let mut base = Space::new("base", Arc::clone(&graph)).unwrap();
    base.bind_alias("theorem", theorem_a.clone()).unwrap();
    let ancestor = base.snapshot().unwrap();
    let mut left = base.branch("left").unwrap();
    left.set_lock_root(Some(ancestor.id.clone()));
    left.bind_alias("theorem", theorem_b).unwrap();
    let right_without_ancestor = base.branch("right").unwrap();
    assert_eq!(
        Space::semantic_merge("bad", &ancestor, &left, &right_without_ancestor, Vec::new())
            .unwrap_err(),
        SpaceError::NoCommonAncestor
    );

    let mut right = right_without_ancestor;
    right.set_lock_root(Some(ancestor.id.clone()));
    let action = MergeAction {
        reconciliation_object: choice,
        operation: Reconciliation::Choose {
            alias: "theorem".to_string(),
            selected: theorem_a.clone(),
        },
    };
    let (merged, receipt) =
        Space::semantic_merge("chosen", &ancestor, &left, &right, vec![action.clone()]).unwrap();
    assert_eq!(
        merged.alias("theorem").unwrap(),
        &std::collections::BTreeSet::from([theorem_a])
    );
    assert_eq!(receipt.actions, vec![action]);
}
