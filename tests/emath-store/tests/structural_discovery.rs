//! contracts: structural discovery (`emath
//! find` store tier) with semantic filters.

use emath_core::MeaningId;
use emath_store::discovery::{FindFilter, FindQuery};
use emath_store::object_graph::{
    ObjectDraft, ObjectGraph, ObjectKind, RelationDraft, RelationKind, RelationScope,
};
use emath_test_harness::Probe;

fn object(meaning: &str, presentation: &str, kind: ObjectKind) -> ObjectDraft {
    ObjectDraft {
        kind,
        meaning_id: MeaningId::from_bytes(meaning.as_bytes()),
        semantic_payload: meaning.as_bytes().to_vec(),
        presentation: Some(presentation.to_string()),
    }
}

fn relation(kind: RelationKind, source: emath_core::ObjectId, target: emath_core::ObjectId, authority: Option<&str>) -> RelationDraft {
    RelationDraft { kind, source, target, scope: RelationScope::Global, assumptions: Vec::new(), authority: authority.map(str::to_string), evidence: Vec::new() }
}

fn toy_graph() -> (ObjectGraph, Vec<emath_core::ObjectId>) {
    let mut graph = ObjectGraph::default();
    let metric = graph.put(object("trait:MetricSpace", "trait MetricSpace", ObjectKind::Theory)).unwrap();
    let solver = graph.put(object("goal:solve.ode", "ode solver", ObjectKind::Cell)).unwrap();
    let theory = graph.put(object("theory:euclid", "Euclidean plane", ObjectKind::Theory)).unwrap();
    let bare = graph.put(object("cell:bare", "no relations", ObjectKind::Cell)).unwrap();
    graph.add_relation(relation(RelationKind::Implements, theory.clone(), metric.clone(), Some("structural-checked"))).unwrap();
    graph.add_relation(relation(RelationKind::Proves, theory.clone(), solver.clone(), None)).unwrap();
    (graph, vec![metric, solver, theory, bare])
}

fn ids_of(hits: &[emath_store::discovery::DiscoveryHit]) -> Vec<emath_core::ObjectId> {
    hits.iter().map(|h| h.id.clone()).collect()
}

#[test]
fn structural_discovery() {
    let mut p = Probe::new("structural filters select exactly, rank only orders");
    p.case("filters", |p| {
        let (graph, ids) = toy_graph();
        p.eq("kind+relation", ids_of(&FindQuery::new().filter(FindFilter::Kind(ObjectKind::Theory)).filter(FindFilter::Relation(RelationKind::Implements)).run(&graph)), vec![ids[2].clone()]);
        p.eq("authority", ids_of(&FindQuery::new().filter(FindFilter::Authority("structural-checked")).run(&graph)), vec![ids[2].clone()]);
        p.demand("no-authority", FindQuery::new().filter(FindFilter::Authority("no-such-authority")).run(&graph).is_empty(), "unknown authority matches nothing");
        p.eq("to-goal", ids_of(&FindQuery::new().filter(FindFilter::RelationTo(RelationKind::Proves, MeaningId::from_bytes(b"goal:solve.ode"))).run(&graph)), vec![ids[2].clone()]);
        p.demand("wrong-target", FindQuery::new().filter(FindFilter::RelationTo(RelationKind::Proves, MeaningId::from_bytes(b"trait:MetricSpace"))).run(&graph).is_empty(), "wrong meaning matches nothing");
        p.eq("all", FindQuery::new().run(&graph).len(), 4);
    });
    p.case("rank", |p| {
        let (graph, ids) = toy_graph();
        let (bare, solver) = (ids[3].clone(), ids[1].clone());
        let hits = FindQuery::new()
            .filter(FindFilter::Kind(ObjectKind::Theory))
            .filter(FindFilter::Relation(RelationKind::Implements))
            .run_ranked(&graph, |o| Some(if o.id == bare { 0.99 } else if o.id == solver { 0.8 } else { 0.1 }));
        let returned = ids_of(&hits);
        p.demand("no-admit", !returned.contains(&bare) && !returned.contains(&solver), "rank never admits");
        p.eq("survives", returned, vec![ids[2].clone()]);
        p.eq("rank-value", hits[0].rank, Some(0.1));
        let unranked = FindQuery::new().filter(FindFilter::Kind(ObjectKind::Cell)).run(&graph);
        let ranked = FindQuery::new().filter(FindFilter::Kind(ObjectKind::Cell)).run_ranked(&graph, |_| Some(0.5));
        let mut u = ids_of(&unranked);
        u.sort();
        let mut r = ids_of(&ranked);
        r.sort();
        p.eq("membership", u.clone(), r);
        let order: Vec<_> = ids_of(&unranked);
        let mut sorted = order.clone();
        sorted.sort();
        p.eq("id-order", order, sorted);
        let all_ranked = FindQuery::new().run_ranked(&graph, |o| Some(if o.presentation.as_deref() == Some("no relations") { 0.99 } else { 0.01 }));
        p.eq("ranker-not-filter", all_ranked.len(), 4);
        p.eq("top-rank", all_ranked[0].rank, Some(0.99));
    });
    p.finish();
}
