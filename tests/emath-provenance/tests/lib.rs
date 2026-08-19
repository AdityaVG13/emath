//! Provenance lineage/cycle tests, moved from
//! `crates/emath-provenance/src/lib.rs`.

use emath_provenance::{
    Adjacency, AuthoredEdge, EdgeId, EdgeKind, Lineage, NodeId, NodeKind, lineage_closure,
    would_create_cycle,
};

/// Test adjacency over a plain vector; order is deliberately shuffled per
/// construction so the algorithms must be order-independent.
struct Flat {
    edges: Vec<AuthoredEdge>,
}

impl Adjacency for Flat {
    fn edges(&self) -> &[AuthoredEdge] {
        &self.edges
    }
}

fn edge(id: u64, kind: EdgeKind, src: u64, dst: u64) -> AuthoredEdge {
    AuthoredEdge {
        id: EdgeId(id),
        kind,
        src: NodeId(src),
        dst: NodeId(dst),
    }
}

fn goal_chain() -> Flat {
    // artifact(3) --ArtifactOf--> plan(2) --PlanOf--> goal(1)
    Flat {
        edges: vec![
            edge(1, EdgeKind::PlanOf, 2, 1),
            edge(2, EdgeKind::ArtifactOf, 3, 2),
        ],
    }
}

#[test]
fn happy_path_lineage_is_goal_plan_chain() {
    let lineage = lineage_closure(&goal_chain(), NodeId(3), 8);
    assert_eq!(
        lineage,
        Lineage {
            goals: vec![NodeId(1)],
            plans: vec![NodeId(2)],
            ..Lineage::default()
        }
    );
}

#[test]
fn empty_graph_lineage_is_empty() {
    let flat = Flat { edges: vec![] };
    assert!(lineage_closure(&flat, NodeId(7), 8).is_empty());
}

#[test]
fn single_node_has_no_ancestors() {
    let flat = Flat { edges: vec![] };
    let lineage = lineage_closure(&flat, NodeId(1), 8);
    assert!(lineage.is_empty());
    assert_eq!(lineage.len(), 0);
}

#[test]
fn boundary_max_depth_zero_returns_nothing() {
    let lineage = lineage_closure(&goal_chain(), NodeId(3), 0);
    assert!(lineage.is_empty());
}

#[test]
fn max_depth_one_sees_only_the_parent() {
    let lineage = lineage_closure(&goal_chain(), NodeId(3), 1);
    assert_eq!(lineage.plans, vec![NodeId(2)]);
    assert!(lineage.goals.is_empty());
}

#[test]
fn lineage_is_order_independent() {
    let shuffled = Flat {
        edges: vec![
            edge(2, EdgeKind::ArtifactOf, 3, 2),
            edge(1, EdgeKind::PlanOf, 2, 1),
        ],
    };
    assert_eq!(
        lineage_closure(&shuffled, NodeId(3), 8),
        lineage_closure(&goal_chain(), NodeId(3), 8)
    );
}

#[test]
fn evidence_edges_report_artifact_ancestors() {
    let flat = Flat {
        edges: vec![
            edge(1, EdgeKind::PlanOf, 2, 1),
            edge(2, EdgeKind::ArtifactOf, 3, 2),
            edge(3, EdgeKind::EvidenceOf, 4, 3),
        ],
    };
    let lineage = lineage_closure(&flat, NodeId(4), 8);
    assert_eq!(
        lineage,
        Lineage {
            goals: vec![NodeId(1)],
            plans: vec![NodeId(2)],
            artifacts: vec![NodeId(3)],
            ..Lineage::default()
        }
    );
}

#[test]
fn cycle_detection_self_loop() {
    let flat = Flat { edges: vec![] };
    assert!(would_create_cycle(&flat, NodeId(1), NodeId(1)));
}

#[test]
fn cycle_detection_through_existing_path() {
    // artifact → plan → goal already exists; adding goal → artifact
    // closes artifact → plan → goal → artifact.
    assert!(would_create_cycle(&goal_chain(), NodeId(1), NodeId(3)));
}

#[test]
fn cycle_detection_accepts_acyclic_extension() {
    // Adding a second plan under the same goal is acyclic.
    assert!(!would_create_cycle(&goal_chain(), NodeId(5), NodeId(1)));
}

#[test]
fn duplicate_kind_labels_are_stable() {
    assert_eq!(NodeKind::Goal.label(), 1);
    assert_eq!(NodeKind::Plan.label(), 2);
    assert_eq!(NodeKind::Artifact.label(), 3);
    assert_eq!(NodeKind::Evidence.label(), 4);
    assert_eq!(EdgeKind::PlanOf.relation(), 1);
    assert_eq!(EdgeKind::ArtifactOf.relation(), 2);
    assert_eq!(EdgeKind::EvidenceOf.relation(), 3);
    assert_eq!(NodeKind::from_label(4), Some(NodeKind::Evidence));
    assert_eq!(NodeKind::from_label(0), None);
    assert_eq!(EdgeKind::from_relation(3), Some(EdgeKind::EvidenceOf));
    assert_eq!(EdgeKind::from_relation(9), None);
    assert_eq!(EdgeKind::PlanOf.dst_kind(), NodeKind::Goal);
}
