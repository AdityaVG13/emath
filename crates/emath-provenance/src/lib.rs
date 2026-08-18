//! Provenance/evidence lineage model (goal → plan → artifact), spike pass 2.
//!
//! Feature-gated adapter crate (`CUTOVER_PLAN.md` §5.3 / §9.11): the default
//! build is std-only with zero third-party dependencies and only first-party
//! code. The `graphdb` feature adds the pinned frankengraphdb `fgdb` facade
//! plus the same-rev asupersync runtime that drives it, exposing a blocking
//! [`store::ProvenanceStore`] over the async engine. See CONTRACT.md for
//! invariants, determinism class, and no-claim boundaries.
//!
//! The lineage and cycle algorithms live here against the pure [`Adjacency`]
//! trait so they are std-only, deterministic, and testable without the engine.

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::fmt;

/// Kinds of provenance nodes. Every node in the graph is exactly one kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeKind {
    /// A goal (the why).
    Goal,
    /// A plan (the how), a child of a goal.
    Plan,
    /// An artifact (the what), a child of a plan.
    Artifact,
    /// Evidence (the proof), a child of an artifact.
    Evidence,
}

impl NodeKind {
    /// Vertex label id persisted in the engine (1..=4). Deterministic and
    /// stable for this spike; do not renumber.
    pub const fn label(self) -> u64 {
        match self {
            Self::Goal => 1,
            Self::Plan => 2,
            Self::Artifact => 3,
            Self::Evidence => 4,
        }
    }

    /// Inverse of [`NodeKind::label`].
    pub const fn from_label(label: u64) -> Option<Self> {
        match label {
            1 => Some(Self::Goal),
            2 => Some(Self::Plan),
            3 => Some(Self::Artifact),
            4 => Some(Self::Evidence),
            _ => None,
        }
    }
}

/// Kinds of provenance edges. All point from a child to its parent, so the
/// authored direction *is* the lineage direction (walking an edge visits one
/// ancestor).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeKind {
    /// plan → goal (a plan is a plan of a goal).
    PlanOf,
    /// artifact → plan (an artifact is an artifact of a plan).
    ArtifactOf,
    /// evidence → artifact (evidence attests an artifact).
    EvidenceOf,
}

impl EdgeKind {
    /// Relation id persisted in the engine (1..=3 for PlanOf..=EvidenceOf).
    pub const fn relation(self) -> u64 {
        match self {
            Self::PlanOf => 1,
            Self::ArtifactOf => 2,
            Self::EvidenceOf => 3,
        }
    }

    /// Inverse of [`EdgeKind::relation`].
    pub const fn from_relation(relation: u64) -> Option<Self> {
        match relation {
            1 => Some(Self::PlanOf),
            2 => Some(Self::ArtifactOf),
            3 => Some(Self::EvidenceOf),
            _ => None,
        }
    }

    /// The kind of the destination node of an edge of this kind.
    pub const fn dst_kind(self) -> NodeKind {
        match self {
            Self::PlanOf => NodeKind::Goal,
            Self::ArtifactOf => NodeKind::Plan,
            Self::EvidenceOf => NodeKind::Artifact,
        }
    }
}

/// Sequence-ordered node identity, supplied by the caller (never wall-clock).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

/// Sequence-ordered edge identity, supplied by the caller (never wall-clock).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeId(pub u64);

/// One authored edge in the graph, in adjacency order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthoredEdge {
    /// Edge identity (unique per graph).
    pub id: EdgeId,
    /// Edge kind (which also fixes the destination node kind).
    pub kind: EdgeKind,
    /// Source node (the child).
    pub src: NodeId,
    /// Destination node (the parent).
    pub dst: NodeId,
}

/// Minimal graph read contract the deterministic algorithms are written
/// against. The engine-backed store implements it from its read view; pure
/// tests implement it over a plain vector, keeping the algorithms std-only.
pub trait Adjacency {
    /// Every live authored edge in the graph, in any stable order (the
    /// algorithms sort/visit deterministically regardless).
    fn edges(&self) -> Vec<AuthoredEdge>;
}

/// Result of a lineage query: ancestors of the seed node reachable along
/// authored edges within `max_depth` steps, grouped by kind, each list sorted
/// ascending by [`NodeId`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lineage {
    /// Ancestor goal nodes (children of `PlanOf` edges).
    pub goals: Vec<NodeId>,
    /// Ancestor plan nodes (children of `ArtifactOf` edges).
    pub plans: Vec<NodeId>,
    /// Ancestor artifact nodes (children of `EvidenceOf` edges).
    pub artifacts: Vec<NodeId>,
    /// Ancestor evidence nodes (currently only ever the seed itself; kept for
    /// symmetry with the kind model).
    pub evidences: Vec<NodeId>,
}

impl Lineage {
    /// True when no ancestor was found within the depth budget.
    pub fn is_empty(&self) -> bool {
        self.goals.is_empty()
            && self.plans.is_empty()
            && self.artifacts.is_empty()
            && self.evidences.is_empty()
    }

    /// Total number of discovered ancestors.
    pub fn len(&self) -> usize {
        self.goals.len() + self.plans.len() + self.artifacts.len() + self.evidences.len()
    }

    fn push(&mut self, kind: NodeKind, id: NodeId) {
        match kind {
            NodeKind::Goal => self.goals.push(id),
            NodeKind::Plan => self.plans.push(id),
            NodeKind::Artifact => self.artifacts.push(id),
            NodeKind::Evidence => self.evidences.push(id),
        }
    }

    fn finish(&mut self) {
        self.goals.sort_unstable();
        self.plans.sort_unstable();
        self.artifacts.sort_unstable();
        self.evidences.sort_unstable();
    }
}

/// Provenance store errors. Internal to this crate; no E-* codes are
/// introduced (`ERROR_CODES.md` is untouched).
#[derive(Debug)]
pub enum ProvenanceError {
    /// A node with this id already exists in the graph.
    DuplicateNode(NodeId),
    /// An edge with this id already exists in the graph.
    DuplicateEdge(EdgeId),
    /// An edge references a node that does not exist.
    MissingNode(NodeId),
    /// Adding the edge would make the node reachable from itself.
    Cycle {
        /// The proposed edge source.
        from: NodeId,
        /// The proposed edge destination.
        to: NodeId,
    },
    /// Opening or initializing the graph failed.
    Open(String),
    /// A query or engine step failed.
    Query(String),
}

impl fmt::Display for ProvenanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateNode(id) => write!(formatter, "provenance: duplicate node {id:?}"),
            Self::DuplicateEdge(id) => write!(formatter, "provenance: duplicate edge {id:?}"),
            Self::MissingNode(id) => write!(formatter, "provenance: missing node {id:?}"),
            Self::Cycle { from, to } => write!(
                formatter,
                "provenance: edge {from:?} → {to:?} would create a cycle"
            ),
            Self::Open(message) => write!(formatter, "provenance open: {message}"),
            Self::Query(message) => write!(formatter, "provenance query: {message}"),
        }
    }
}

impl std::error::Error for ProvenanceError {}

/// A lineage must be a directed acyclic walk: `max_depth` is a hard cap on
/// ancestor depth along authored edges. This is the max-depth sentinel (an
/// empty seed lineage is `Lineage::default`).
pub const MAX_LINEAGE_DEPTH: usize = 1024;

/// Compute the lineage of `seed`: every ancestor reachable along authored
/// edges within `max_depth` steps. Deterministic: same edge set → same
/// result, independent of edge ordering, wall clock, or runtime scheduling.
/// The seed itself is never included (query its kind via
/// [`Adjacency`]-independent node metadata where needed).
pub fn lineage_closure<A: Adjacency + ?Sized>(
    graph: &A,
    seed: NodeId,
    max_depth: usize,
) -> Lineage {
    let mut outgoing: HashMap<NodeId, Vec<(NodeId, EdgeKind)>> = HashMap::new();
    for edge in graph.edges() {
        outgoing
            .entry(edge.src)
            .or_default()
            .push((edge.dst, edge.kind));
    }
    let mut result = Lineage::default();
    let mut visited: HashSet<NodeId> = HashSet::new();
    visited.insert(seed);
    // max_depth counts ancestor hops: 0 visits nothing, 1 visits direct
    // parents, 2 adds grandparents, and so on.
    if max_depth == 0 {
        return result;
    }
    let mut frontier: Vec<(NodeId, EdgeKind)> = Vec::new();
    for (dst, kind) in outgoing.get(&seed).into_iter().flatten() {
        if visited.insert(*dst) {
            result.push(kind.dst_kind(), *dst);
            frontier.push((*dst, *kind));
        }
    }
    let mut depth = 1usize;
    while depth < max_depth && !frontier.is_empty() {
        let mut next: Vec<(NodeId, EdgeKind)> = Vec::new();
        for (node, _) in &frontier {
            for (dst, kind) in outgoing.get(node).into_iter().flatten() {
                if visited.insert(*dst) {
                    result.push(kind.dst_kind(), *dst);
                    next.push((*dst, *kind));
                }
            }
        }
        frontier = next;
        depth += 1;
    }
    result.finish();
    result
}

/// Would adding an edge src → dst make the graph cyclic? True iff `dst`
/// already reaches `src` along authored edges (the new edge would close the
/// loop), or src == dst (a self-loop).
pub fn would_create_cycle<A: Adjacency + ?Sized>(graph: &A, src: NodeId, dst: NodeId) -> bool {
    if src == dst {
        return true;
    }
    let mut outgoing: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for edge in graph.edges() {
        outgoing.entry(edge.src).or_default().push(edge.dst);
    }
    let mut seen: HashSet<NodeId> = HashSet::new();
    seen.insert(dst);
    let mut frontier = vec![dst];
    while let Some(node) = frontier.pop() {
        for next in outgoing.get(&node).into_iter().flatten() {
            if *next == src {
                return true;
            }
            if seen.insert(*next) {
                frontier.push(*next);
            }
        }
    }
    false
}

#[cfg(feature = "graphdb")]
pub mod store;

#[cfg(feature = "graphdb")]
pub use store::ProvenanceStore;

#[cfg(test)]
mod tests {
    use super::{
        Adjacency, AuthoredEdge, EdgeId, EdgeKind, Lineage, NodeId, NodeKind, lineage_closure,
        would_create_cycle,
    };

    /// Test adjacency over a plain vector; order is deliberately shuffled per
    /// construction so the algorithms must be order-independent.
    struct Flat {
        edges: Vec<AuthoredEdge>,
    }

    impl Adjacency for Flat {
        fn edges(&self) -> Vec<AuthoredEdge> {
            self.edges.clone()
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
}
