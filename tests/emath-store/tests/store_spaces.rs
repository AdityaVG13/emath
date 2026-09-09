//! Spaces share objects across branches and locks refuse dangling or revoked deps.

use std::sync::Arc;

use emath_core::{MeaningId, ObjectId};
use emath_store::{
    LibraryLock, ObjectDraft, ObjectGraph, ObjectKind, Space, SpaceError, SpacePolicy,
};
use emath_test_harness::Probe;

fn two_cells() -> (Arc<ObjectGraph>, ObjectId, ObjectId) {
    let mut graph = ObjectGraph::default();
    let first = graph.put(ObjectDraft { kind: ObjectKind::Cell, meaning_id: MeaningId::from_bytes(b"first meaning"), semantic_payload: b"first".to_vec(), presentation: Some("first presentation".to_string()) }).unwrap();
    let second = graph.put(ObjectDraft { kind: ObjectKind::Cell, meaning_id: MeaningId::from_bytes(b"second meaning"), semantic_payload: b"second".to_vec(), presentation: Some("second presentation".to_string()) }).unwrap();
    (Arc::new(graph), first, second)
}

#[test]
fn store_spaces() {
    let mut p = Probe::new("spaces branch by sharing and locks verify honestly");
    p.case("branch", |p| {
        let (graph, first, second) = two_cells();
        let mut space = Space::new("main", Arc::clone(&graph)).unwrap();
        space.bind_alias("theorem", first.clone()).unwrap();
        space.bind_alias("theorem", second.clone()).unwrap();
        space.set_policy(SpacePolicy { lens: Some("compact".to_string()), provider: Some("local".to_string()), trust: Some("checked".to_string()) });
        let snapshot = space.snapshot().unwrap();
        p.eq("conflicts", snapshot.aliases["theorem"].len(), 2);
        let branch = space.branch("experiment").unwrap();
        p.demand("shares", space.shares_objects_with(&branch), "branch shares");
        p.eq("alias", branch.alias("theorem"), space.alias("theorem"));
        p.eq("snapshot", branch.snapshot().unwrap().id.clone(), snapshot.id.clone());
        p.demand("verify", LibraryLock::from_snapshot(&snapshot, Vec::new()).verify(&graph).is_ok(), "lock verifies");
    });
    p.case("refuse", |p| {
        let (graph, first, _) = two_cells();
        let mut space = Space::new("main", Arc::clone(&graph)).unwrap();
        space.bind_alias("cell", first.clone()).unwrap();
        let snapshot = space.snapshot().unwrap();
        let missing = ObjectId::from_bytes(b"dangling");
        p.eq("dangling", LibraryLock::from_snapshot(&snapshot, [missing.clone()]).verify(&graph), Err(SpaceError::MissingObject(missing)));
        let mut revoked = LibraryLock::from_snapshot(&snapshot, Vec::new());
        revoked.revoke(first.clone());
        p.eq("revoked", revoked.verify(&graph), Err(SpaceError::RevokedObject(first)));
    });
    p.finish();
}
