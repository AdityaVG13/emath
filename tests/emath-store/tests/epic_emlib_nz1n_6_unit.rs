use std::sync::Arc;

use emath_core::{MeaningId, ObjectId};
use emath_store::{
    LibraryLock, ObjectDraft, ObjectGraph, ObjectKind, Space, SpaceError, SpacePolicy,
};

fn graph_with_two_cells() -> (Arc<ObjectGraph>, ObjectId, ObjectId) {
    let mut graph = ObjectGraph::default();
    let first = graph
        .put(ObjectDraft {
            kind: ObjectKind::Cell,
            meaning_id: MeaningId::from_bytes(b"first meaning"),
            semantic_payload: b"first".to_vec(),
            presentation: Some("first presentation".to_string()),
        })
        .unwrap();
    let second = graph
        .put(ObjectDraft {
            kind: ObjectKind::Cell,
            meaning_id: MeaningId::from_bytes(b"second meaning"),
            semantic_payload: b"second".to_vec(),
            presentation: Some("second presentation".to_string()),
        })
        .unwrap();
    (Arc::new(graph), first, second)
}

#[test]
fn spaces_keep_alias_conflicts_and_branch_by_sharing_objects() {
    let (graph, first, second) = graph_with_two_cells();
    let mut space = Space::new("main", Arc::clone(&graph)).unwrap();
    space
        .bind_alias("theorem", first.clone())
        .expect("first binding");
    space
        .bind_alias("theorem", second.clone())
        .expect("conflicting meaning remains addressable");
    space.set_policy(SpacePolicy {
        lens: Some("compact".to_string()),
        provider: Some("local".to_string()),
        trust: Some("checked".to_string()),
    });

    let snapshot = space.snapshot().unwrap();
    assert_eq!(snapshot.aliases["theorem"].len(), 2);
    let branch = space.branch("experiment").unwrap();
    assert!(space.shares_objects_with(&branch));
    assert_eq!(branch.alias("theorem"), space.alias("theorem"));
    assert_eq!(branch.snapshot().unwrap().id, snapshot.id);

    let lock = LibraryLock::from_snapshot(&snapshot, Vec::new());
    assert_eq!(lock.dependencies.len(), 2);
    lock.verify(&graph).unwrap();
}

#[test]
fn library_lock_refuses_dangling_and_revoked_dependencies() {
    let (graph, first, _) = graph_with_two_cells();
    let mut space = Space::new("main", Arc::clone(&graph)).unwrap();
    space.bind_alias("cell", first.clone()).unwrap();
    let snapshot = space.snapshot().unwrap();

    let missing = ObjectId::from_bytes(b"dangling");
    let dangling = LibraryLock::from_snapshot(&snapshot, [missing.clone()]);
    assert_eq!(
        dangling.verify(&graph),
        Err(SpaceError::MissingObject(missing))
    );

    let mut revoked = LibraryLock::from_snapshot(&snapshot, Vec::new());
    revoked.revoke(first.clone());
    assert_eq!(
        revoked.verify(&graph),
        Err(SpaceError::RevokedObject(first))
    );
}
