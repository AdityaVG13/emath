//! Semantic merge retains conflicting meanings and requires explicit reconciliation.

use std::sync::Arc;

use emath_core::{MeaningId, ObjectId};
use emath_store::{
    MergeAction, ObjectDraft, ObjectGraph, ObjectKind, Reconciliation, Space, SpaceError,
};
use emath_test_harness::Probe;

fn put(graph: &mut ObjectGraph, kind: ObjectKind, meaning: &str) -> ObjectId {
    graph.put(ObjectDraft { kind, meaning_id: MeaningId::from_bytes(meaning.as_bytes()), semantic_payload: meaning.as_bytes().to_vec(), presentation: None }).unwrap()
}

#[test]
fn semantic_merge() {
    let mut p = Probe::new("merge retains conflicts and reconciliation is explicit");
    p.case("retain", |p| {
        let mut graph = ObjectGraph::default();
        let (ta, tb) = (put(&mut graph, ObjectKind::Cell, "theorem-a"), put(&mut graph, ObjectKind::Cell, "theorem-b"));
        let (ea, eb) = (put(&mut graph, ObjectKind::Proof, "evidence-a"), put(&mut graph, ObjectKind::Proof, "evidence-b"));
        let graph = Arc::new(graph);
        let mut base = Space::new("base", Arc::clone(&graph)).unwrap();
        base.bind_alias("theorem", ta.clone()).unwrap();
        let ancestor = base.snapshot().unwrap();
        let mut left = base.branch("left").unwrap();
        left.set_lock_root(Some(ancestor.id.clone()));
        left.bind_alias("theorem", tb.clone()).unwrap();
        left.bind_alias("evidence", ea.clone()).unwrap();
        let mut right = base.branch("right").unwrap();
        right.set_lock_root(Some(ancestor.id.clone()));
        right.bind_alias("evidence", eb.clone()).unwrap();
        let (merged, receipt) = Space::semantic_merge("merged", &ancestor, &left, &right, Vec::new()).unwrap();
        p.eq("theorem-n", merged.alias("theorem").unwrap().len(), 2);
        p.demand("has-a", merged.alias("theorem").unwrap().contains(&ta), "keeps left");
        p.demand("has-b", merged.alias("theorem").unwrap().contains(&tb), "keeps right");
        p.eq("evidence-n", merged.alias("evidence").unwrap().len(), 2);
        p.demand("merge-prefix", receipt.id.as_str().starts_with("emath:merge:v1:"), "merge prefix");
    });
    p.case("reconcile", |p| {
        let mut graph = ObjectGraph::default();
        let (ta, tb) = (put(&mut graph, ObjectKind::Cell, "theorem-a"), put(&mut graph, ObjectKind::Cell, "theorem-b"));
        let choice = put(&mut graph, ObjectKind::Method, "explicit-choice");
        let graph = Arc::new(graph);
        let mut base = Space::new("base", Arc::clone(&graph)).unwrap();
        base.bind_alias("theorem", ta.clone()).unwrap();
        let ancestor = base.snapshot().unwrap();
        let mut left = base.branch("left").unwrap();
        left.set_lock_root(Some(ancestor.id.clone()));
        left.bind_alias("theorem", tb).unwrap();
        let right_bare = base.branch("right").unwrap();
        p.eq("no-ancestor", Space::semantic_merge("bad", &ancestor, &left, &right_bare, Vec::new()).unwrap_err(), SpaceError::NoCommonAncestor);
        let mut right = right_bare;
        right.set_lock_root(Some(ancestor.id.clone()));
        let action = MergeAction { reconciliation_object: choice, operation: Reconciliation::Choose { alias: "theorem".to_string(), selected: ta.clone() } };
        let (merged, receipt) = Space::semantic_merge("chosen", &ancestor, &left, &right, vec![action.clone()]).unwrap();
        p.eq("chosen", merged.alias("theorem").unwrap().clone(), std::collections::BTreeSet::from([ta]));
        p.eq("actions", receipt.actions.clone(), vec![action]);
    });
    p.finish();
}
