//! Structural discovery (`emath find`, store tier) —
//!.
//!
//! Search by mathematics over the object graph: exact filters
//! are STRUCTURAL — object kind, outgoing relation kind, relation
//! toward a target meaning, relation authority. Text/embedding
//! similarity may only RANK the compatible set: **rank cannot override
//! compatibility** — a ranking function scores candidates, but the
//! compatible membership is decided by the exact filters alone, so an
//! embedding-similar object that fails the filters is never returned
//! and a ranker is never a filter. Search is not a second type checker:
//! it queries structural metadata (kinds/relations/authority), it never
//! re-derives semantics and never guesses math equivalence.
//!
//! Determinism class: pure sequence — no ranker ⇒ ascending id order;
//! with a ranker ⇒ descending rank, ties broken by id. Rank values are
//! display metadata carried on the hit.

use std::collections::BTreeSet;

use emath_core::{MeaningId, ObjectId};

use crate::object_graph::{LibraryObject, ObjectGraph, ObjectKind, RelationKind};

/// One exact structural filter. All filters on a query must hold
/// (conjunction); the empty query matches every stored object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FindFilter {
    /// Object kind (`theory`, `cell`, …).
    Kind(ObjectKind),
    /// An outgoing relation of the given kind exists.
    Relation(RelationKind),
    /// An outgoing relation of the given kind toward a target with this
    /// meaning id (`implements → MetricSpace`).
    RelationTo(RelationKind, MeaningId),
    /// An outgoing relation carrying this authority token
    /// (`authority proved`).
    Authority(&'static str),
}

/// A discovery hit: the compatible object plus its optional rank
/// (display metadata, never admission).
#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveryHit {
    pub id: ObjectId,
    pub rank: Option<f64>,
}

/// A structural discovery query over an [`crate::object_graph::ObjectGraph`].
#[derive(Clone, Debug, Default)]
pub struct FindQuery {
    filters: Vec<FindFilter>,
}

impl FindQuery {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one exact structural filter (conjunctive).
    #[must_use]
    pub fn filter(mut self, filter: FindFilter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Run without a ranker: the compatible set in ascending id order.
    pub fn run(&self, graph: &crate::object_graph::ObjectGraph) -> Vec<DiscoveryHit> {
        self.run_ranked(graph, |_| None)
    }

    /// Run with a similarity ranker. The ranker is consulted ONLY for
    /// objects that already passed every exact filter — it orders the
    /// compatible set and annotates hits; it can never add an
    /// incompatible object or remove a compatible one (rank is
    /// display-only).
    pub fn run_ranked(
        &self,
        graph: &crate::object_graph::ObjectGraph,
        ranker: impl Fn(&crate::object_graph::LibraryObject) -> Option<f64>,
    ) -> Vec<DiscoveryHit> {
        // Index outgoing relations by source for O(1) filter checks.
        let mut outgoing: std::collections::BTreeMap<
            ObjectId,
            Vec<&crate::object_graph::Relation>,
        > = std::collections::BTreeMap::new();
        for relation in graph.relations() {
            outgoing
                .entry(relation.source.clone())
                .or_default()
                .push(relation);
        }
        let mut hits: Vec<DiscoveryHit> = graph
            .objects()
            .filter(|object| {
                self.filters.iter().all(|filter| match filter {
                    FindFilter::Kind(kind) => object.kind == *kind,
                    FindFilter::Relation(kind) => outgoing
                        .get(&object.id)
                        .is_some_and(|edges| edges.iter().any(|edge| edge.kind == *kind)),
                    FindFilter::RelationTo(kind, target_meaning) => {
                        outgoing.get(&object.id).is_some_and(|edges| {
                            edges.iter().any(|edge| {
                                edge.kind == *kind
                                    && graph
                                        .object(&edge.target)
                                        .is_some_and(|target| target.meaning_id == *target_meaning)
                            })
                        })
                    }
                    FindFilter::Authority(authority) => {
                        outgoing.get(&object.id).is_some_and(|edges| {
                            edges
                                .iter()
                                .any(|edge| edge.authority.as_deref() == Some(*authority))
                                || edges.is_empty()
                        })
                    }
                })
            })
            .map(|object| DiscoveryHit {
                id: object.id.clone(),
                rank: ranker(object),
            })
            .collect();
        // No ranker ⇒ ascending id. Ranker ⇒ descending rank, ties by
        // id: deterministic either way.
        hits.sort_by(|left, right| match (left.rank, right.rank) {
            (Some(left_rank), Some(right_rank)) => right_rank
                .partial_cmp(&left_rank)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.id.cmp(&right.id)),
            _ => left.id.cmp(&right.id),
        });
        hits
    }
}
