//! contracts: structural discovery (`emath
//! find` store tier) with semantic filters.
//!
//! Search by mathematics over the object graph: exact filters
//! are STRUCTURAL — object kind, outgoing relation kind, relation
//! toward a target meaning, relation authority. Text/embedding
//! similarity may only RANK the compatible set: **rank cannot override
//! compatibility** (an embedding-similar object that fails the exact
//! filters is never returned), and with no ranker the result is the
//! deterministic id order. Search is not a second type checker: it
//! queries structural metadata, it never re-derives semantics.

use emath_core::MeaningId;
use emath_store::discovery::{FindFilter, FindQuery};
use emath_store::object_graph::{
    ObjectDraft, ObjectGraph, ObjectKind, RelationDraft, RelationKind, RelationScope,
};

fn object(meaning: &str, presentation: &str, kind: ObjectKind) -> ObjectDraft {
    ObjectDraft {
        kind,
        meaning_id: MeaningId::from_bytes(meaning.as_bytes()),
        semantic_payload: meaning.as_bytes().to_vec(),
        presentation: Some(presentation.to_string()),
    }
}

fn relation(
    kind: RelationKind,
    source: emath_core::ObjectId,
    target: emath_core::ObjectId,
    authority: Option<&str>,
) -> RelationDraft {
    RelationDraft {
        kind,
        source,
        target,
        scope: RelationScope::Global,
        assumptions: Vec::new(),
        authority: authority.map(str::to_string),
        evidence: Vec::new(),
    }
}

/// The toy snapshot: a theory implementing a `MetricSpace` carrier
/// (authority: structural-checked), a goal, and a bare cell with no
/// relations.
fn toy_graph() -> (ObjectGraph, Vec<emath_core::ObjectId>) {
    let mut graph = ObjectGraph::default();
    let metric_space = graph
        .put(object(
            "trait:MetricSpace",
            "trait MetricSpace",
            ObjectKind::Theory,
        ))
        .unwrap();
    let solver = graph
        .put(object("goal:solve.ode", "ode solver", ObjectKind::Cell))
        .unwrap();
    let theory = graph
        .put(object(
            "theory:euclid",
            "Euclidean plane",
            ObjectKind::Theory,
        ))
        .unwrap();
    let bare = graph
        .put(object("cell:bare", "no relations", ObjectKind::Cell))
        .unwrap();
    graph
        .add_relation(relation(
            RelationKind::Implements,
            theory.clone(),
            metric_space.clone(),
            Some("structural-checked"),
        ))
        .unwrap();
    graph
        .add_relation(relation(
            RelationKind::Proves,
            theory.clone(),
            solver.clone(),
            None,
        ))
        .unwrap();
    (graph, vec![metric_space, solver, theory, bare])
}

/// Happy path: kind + implements-MetricSpace filters return exactly the
/// compatible theory — the bare cell and the solver are excluded.
#[test]
fn structural_filters_return_exactly_the_compatible_set() {
    let (graph, ids) = toy_graph();
    let hits = FindQuery::new()
        .filter(FindFilter::Kind(ObjectKind::Theory))
        .filter(FindFilter::Relation(RelationKind::Implements))
        .run(&graph);
    assert_eq!(
        hits.iter().map(|hit| hit.id.clone()).collect::<Vec<_>>(),
        vec![ids[2].clone()],
        "only the Euclidean theory implements MetricSpace"
    );
}

/// The authority filter is EXACT-set: an object qualifies only if an
/// outgoing relation carries the named authority. A second,
/// authority-less edge (the Proves edge) does NOT qualify an object
/// whose authority edges were removed — pin by querying an authority
/// name no edge carries: only the empty set comes back, never the
/// objects that merely have edges.
#[test]
fn authority_filter_matches_only_proved_authority() {
    let (graph, ids) = toy_graph();
    let hits = FindQuery::new()
        .filter(FindFilter::Authority("structural-checked"))
        .run(&graph);
    assert_eq!(
        hits.iter().map(|hit| hit.id.clone()).collect::<Vec<_>>(),
        vec![ids[2].clone()]
    );
    // No edge carries this authority: the result is empty. An
    // inverted/loose authority comparison would return the objects that
    // merely have edges (the theory via its authority-less proves edge,
    // etc.).
    let none = FindQuery::new()
        .filter(FindFilter::Authority("no-such-authority"))
        .run(&graph);
    assert!(
        none.is_empty(),
        "an authority name no relation carries must match nothing, got {:?}",
        none.iter().map(|hit| hit.id.clone()).collect::<Vec<_>>()
    );
}

/// Relation-toward-a-target-meaning: find what proves the ode goal.
/// The target-meaning conjunct is load-bearing: a second fixture edge
/// of the same kind toward a DIFFERENT meaning exists (theory →
/// metric-space, plus theory → solver) so dropping the conjunct changes
/// the answer.
#[test]
fn relation_to_target_meaning_filter() {
    let (graph, ids) = toy_graph();
    let goal_meaning = MeaningId::from_bytes(b"goal:solve.ode");
    let hits = FindQuery::new()
        .filter(FindFilter::RelationTo(RelationKind::Proves, goal_meaning))
        .run(&graph);
    assert_eq!(
        hits.iter().map(|hit| hit.id.clone()).collect::<Vec<_>>(),
        vec![ids[2].clone()]
    );

    // The wrong-meaning control: the theory's OUTGOING proves edge
    // points AT the goal; asking for a proves edge toward the METRIC
    // SPACE meaning (no such edge) must return nothing.
    let metric_meaning = MeaningId::from_bytes(b"trait:MetricSpace");
    let none = FindQuery::new()
        .filter(FindFilter::RelationTo(RelationKind::Proves, metric_meaning))
        .run(&graph);
    assert!(
        none.is_empty(),
        "a relation toward a meaning with no such edge must match nothing, got {:?}",
        none.iter().map(|hit| hit.id.clone()).collect::<Vec<_>>()
    );
}

/// NEGATIVE (the pinned rule): an embedding-similar object that
/// FAILS the exact filters is never admitted by rank — the ranker
/// scores the bare cell highest and the result still excludes it. Rank
/// orders the compatible set only.
#[test]
fn embedding_rank_cannot_admit_an_incompatible_object() {
    let (graph, ids) = toy_graph();
    let bare = ids[3].clone();
    let solver = ids[1].clone();
    // Embedding similarity loves the bare cell and the solver.
    let ranker = |object: &emath_store::object_graph::LibraryObject| {
        if object.id == bare {
            Some(0.99)
        } else if object.id == solver {
            Some(0.8)
        } else {
            Some(0.1)
        }
    };
    let hits = FindQuery::new()
        .filter(FindFilter::Kind(ObjectKind::Theory))
        .filter(FindFilter::Relation(RelationKind::Implements))
        .run_ranked(&graph, ranker);
    let returned: Vec<_> = hits.iter().map(|hit| hit.id.clone()).collect();
    assert!(
        !returned.contains(&bare) && !returned.contains(&solver),
        "rank must never admit an incompatible object, got {returned:?}"
    );
    assert_eq!(
        returned,
        vec![ids[2].clone()],
        "the compatible object survives, ranked"
    );
    assert_eq!(hits[0].rank, Some(0.1), "rank is attached, display-only");
}

/// Rank is DISPLAY-ONLY: with or without a ranker, the membership is
/// identical; the ranker only reorders (ties broken by id).
#[test]
fn rank_reorders_but_never_changes_membership() {
    let (graph, _) = toy_graph();
    let unranked = FindQuery::new()
        .filter(FindFilter::Kind(ObjectKind::Cell))
        .run(&graph);
    let ranked = FindQuery::new()
        .filter(FindFilter::Kind(ObjectKind::Cell))
        .run_ranked(&graph, |_| Some(0.5));
    let mut unranked_ids: Vec<_> = unranked.iter().map(|hit| hit.id.clone()).collect();
    unranked_ids.sort();
    let mut ranked_ids: Vec<_> = ranked.iter().map(|hit| hit.id.clone()).collect();
    ranked_ids.sort();
    assert_eq!(unranked_ids, ranked_ids, "rank never changes membership");
    // Without a ranker: deterministic id order.
    let ids: Vec<_> = unranked.iter().map(|hit| hit.id.clone()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
}

/// No filters: the query returns the whole graph (rank-ordered or
/// id-ordered) — nothing is hidden by default.
#[test]
fn empty_filter_set_returns_every_object() {
    let (graph, _) = toy_graph();
    let hits = FindQuery::new().run(&graph);
    assert_eq!(hits.len(), 4, "the toy graph has four objects");
}

/// Text similarity alone is NOT a query: a ranker without exact filters
/// still returns the full compatible set (rank annotates; it does not
/// select). The display-only boundary holds even here.
#[test]
fn ranker_alone_selects_nothing_out() {
    let (graph, _) = toy_graph();
    let hits = FindQuery::new().run_ranked(&graph, |object| {
        Some(if object.presentation.as_deref() == Some("no relations") {
            0.99
        } else {
            0.01
        })
    });
    assert_eq!(hits.len(), 4, "a ranker is not a filter");
    assert_eq!(hits[0].rank, Some(0.99), "the top rank is display metadata");
}
