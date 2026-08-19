//! Engine-backed provenance store tests, moved from
//! `crates/emath-provenance/src/store.rs`. Compile with the package's
//! `graphdb` feature (previously `cfg(all(test, feature = "graphdb"))`).

#![cfg(feature = "graphdb")]

use emath_provenance::{
    EdgeId, EdgeKind, Lineage, NodeId, NodeKind, ProvenanceError, ProvenanceStore,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static SCRATCH: AtomicU64 = AtomicU64::new(0);

fn scratch_dir(name: &str) -> PathBuf {
    let n = SCRATCH.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "emath-provenance-{}-{name}-{n}",
        std::process::id()
    ))
}

fn make_chain(store: &ProvenanceStore) -> Result<(), ProvenanceError> {
    store.insert_node(NodeId(1), NodeKind::Goal)?;
    store.insert_node(NodeId(2), NodeKind::Plan)?;
    store.insert_node(NodeId(3), NodeKind::Artifact)?;
    store.insert_edge(EdgeId(1), EdgeKind::PlanOf, NodeId(2), NodeId(1))?;
    store.insert_edge(EdgeId(2), EdgeKind::ArtifactOf, NodeId(3), NodeId(2))?;
    Ok(())
}

#[test]
fn happy_path_lineage_from_artifact() {
    let dir = scratch_dir("happy");
    let store = ProvenanceStore::open(&dir).expect("open");
    make_chain(&store).expect("chain");
    let lineage = store.lineage(NodeId(3), 16).expect("lineage");
    assert_eq!(
        lineage,
        Lineage {
            goals: vec![NodeId(1)],
            plans: vec![NodeId(2)],
            ..Lineage::default()
        }
    );
    assert_eq!(
        store.node_kind(NodeId(1)).expect("kind"),
        Some(NodeKind::Goal)
    );
    assert_eq!(
        store.node_kind(NodeId(3)).expect("kind"),
        Some(NodeKind::Artifact)
    );
    assert_eq!(store.node_kind(NodeId(9)).expect("kind"), None);
}

#[test]
fn empty_graph_lineage_errors_missing_node() {
    let dir = scratch_dir("empty");
    let store = ProvenanceStore::open(&dir).expect("open");
    assert!(matches!(
        store.lineage(NodeId(7), 8),
        Err(ProvenanceError::MissingNode(NodeId(7)))
    ));
}

#[test]
fn single_node_has_empty_lineage() {
    let dir = scratch_dir("single");
    let store = ProvenanceStore::open(&dir).expect("open");
    store.insert_node(NodeId(1), NodeKind::Goal).expect("node");
    let lineage = store.lineage(NodeId(1), 8).expect("lineage");
    assert!(lineage.is_empty());
}

#[test]
fn duplicate_node_id_is_refused() {
    let dir = scratch_dir("dupnode");
    let store = ProvenanceStore::open(&dir).expect("open");
    store.insert_node(NodeId(1), NodeKind::Goal).expect("node");
    assert!(matches!(
        store.insert_node(NodeId(1), NodeKind::Plan),
        Err(ProvenanceError::DuplicateNode(NodeId(1)))
    ));
}

#[test]
fn duplicate_edge_id_is_refused() {
    let dir = scratch_dir("dupedge");
    let store = ProvenanceStore::open(&dir).expect("open");
    store.insert_node(NodeId(1), NodeKind::Goal).expect("node");
    store.insert_node(NodeId(2), NodeKind::Plan).expect("node");
    store
        .insert_edge(EdgeId(1), EdgeKind::PlanOf, NodeId(2), NodeId(1))
        .expect("edge");
    assert!(matches!(
        store.insert_edge(EdgeId(1), EdgeKind::PlanOf, NodeId(2), NodeId(1)),
        Err(ProvenanceError::DuplicateEdge(EdgeId(1)))
    ));
}

#[test]
fn cyclic_edge_is_rejected_before_write() {
    let dir = scratch_dir("cycle");
    let store = ProvenanceStore::open(&dir).expect("open");
    make_chain(&store).expect("chain");
    // goal → artifact closes artifact → plan → goal → artifact.
    let result = store.insert_edge(EdgeId(3), EdgeKind::PlanOf, NodeId(1), NodeId(3));
    assert!(matches!(
        result,
        Err(ProvenanceError::Cycle {
            from: NodeId(1),
            to: NodeId(3),
        })
    ));
    // A self-loop is also a cycle.
    assert!(matches!(
        store.insert_edge(EdgeId(4), EdgeKind::PlanOf, NodeId(1), NodeId(1)),
        Err(ProvenanceError::Cycle { .. })
    ));
    assert_eq!(store.lineage(NodeId(3), 16).expect("lineage").len(), 2);
}

#[test]
fn edge_to_missing_node_is_refused() {
    let dir = scratch_dir("missing");
    let store = ProvenanceStore::open(&dir).expect("open");
    store.insert_node(NodeId(1), NodeKind::Goal).expect("node");
    assert!(matches!(
        store.insert_edge(EdgeId(1), EdgeKind::PlanOf, NodeId(2), NodeId(1)),
        Err(ProvenanceError::MissingNode(NodeId(2)))
    ));
    assert!(matches!(
        store.insert_edge(EdgeId(2), EdgeKind::PlanOf, NodeId(1), NodeId(9)),
        Err(ProvenanceError::MissingNode(NodeId(9)))
    ));
}

#[test]
fn identical_inserts_produce_identical_results_across_stores() {
    let first = scratch_dir("det-a");
    let second = scratch_dir("det-b");
    let store_a = ProvenanceStore::open(&first).expect("open a");
    let store_b = ProvenanceStore::open(&second).expect("open b");
    make_chain(&store_a).expect("chain a");
    make_chain(&store_b).expect("chain b");
    assert_eq!(
        store_a.lineage(NodeId(3), 16).expect("lineage a"),
        store_b.lineage(NodeId(3), 16).expect("lineage b")
    );
    assert_eq!(
        store_a.node_kind(NodeId(2)).expect("kind a"),
        store_b.node_kind(NodeId(2)).expect("kind b")
    );
}

#[test]
fn reopen_folds_identical_history() {
    let dir = scratch_dir("reopen");
    let lineage_before;
    let kinds_before;
    {
        let store = ProvenanceStore::open(&dir).expect("open");
        make_chain(&store).expect("chain");
        lineage_before = store.lineage(NodeId(3), 16).expect("lineage");
        kinds_before = store.node_kind(NodeId(2)).expect("kind");
    }
    let reopened = ProvenanceStore::open(&dir).expect("reopen");
    assert_eq!(
        reopened.lineage(NodeId(3), 16).expect("lineage after"),
        lineage_before
    );
    assert_eq!(
        reopened.node_kind(NodeId(2)).expect("kind after"),
        kinds_before
    );
}

#[test]
fn max_depth_limits_lineage() {
    let dir = scratch_dir("depth");
    let store = ProvenanceStore::open(&dir).expect("open");
    make_chain(&store).expect("chain");
    store
        .insert_node(NodeId(4), NodeKind::Evidence)
        .expect("node");
    store
        .insert_edge(EdgeId(3), EdgeKind::EvidenceOf, NodeId(4), NodeId(3))
        .expect("edge");
    // evidence(4) → artifact(3) → plan(2) → goal(1).
    let depth1 = store.lineage(NodeId(4), 1).expect("depth 1");
    assert_eq!(depth1.artifacts, vec![NodeId(3)]);
    assert!(depth1.plans.is_empty());
    let depth2 = store.lineage(NodeId(4), 2).expect("depth 2");
    assert_eq!(depth2.artifacts, vec![NodeId(3)]);
    assert_eq!(depth2.plans, vec![NodeId(2)]);
    let depth3 = store.lineage(NodeId(4), 3).expect("depth 3");
    assert_eq!(depth3.goals, vec![NodeId(1)]);
    assert_eq!(depth3, store.lineage(NodeId(4), 16).expect("depth far"));
}
