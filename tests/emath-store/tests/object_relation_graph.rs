//! Object envelope and typed relation graph are deterministic: presentation
//! never moves identity, relations dedupe, missing endpoints refuse.

use emath_core::{EvidenceId, MeaningId, ObjectId};
use emath_store::{
    ObjectDraft, ObjectGraph, ObjectKind, RelationDraft, RelationKind, RelationScope,
    StoreGraphError,
};
use emath_test_harness::Probe;

fn object(kind: ObjectKind, meaning: &str, presentation: &str) -> ObjectDraft {
    ObjectDraft {
        kind,
        meaning_id: MeaningId::from_bytes(meaning.as_bytes()),
        semantic_payload: meaning.as_bytes().to_vec(),
        presentation: Some(presentation.to_string()),
    }
}

#[test]
fn object_relation_graph() {
    let mut p = Probe::new("object identity is meaning-derived and relations are typed and deduped");
    p.case("envelope", |p| {
        let theorem_meaning = MeaningId::from_bytes(b"theorem");
        let proof_meaning = MeaningId::from_bytes(b"proof");
        let mut graph = ObjectGraph::default();
        let theorem = graph.put(object(ObjectKind::Cell, "theorem", "Theorem, formatted")).unwrap();
        p.eq("reput", graph.put(object(ObjectKind::Cell, "theorem", "Theorem: other prose")).unwrap(), theorem.clone());
        p.eq("meaning", graph.object(&theorem).unwrap().meaning_id.clone(), theorem_meaning.clone());
        let proof = graph.put(object(ObjectKind::Proof, "proof", "A proof")).unwrap();
        let evidence = EvidenceId::from_bytes(b"checked proof");
        let relation = graph.add_relation(RelationDraft {
            kind: RelationKind::Proves, source: proof.clone(), target: theorem.clone(),
            scope: RelationScope::Global, assumptions: vec![theorem_meaning.clone(), theorem_meaning.clone()],
            authority: Some("emath-checker".to_string()), evidence: vec![evidence.clone(), evidence],
        }).unwrap();
        p.demand("rel-prefix", relation.as_str().starts_with("emath:relation:v1:"), "relation prefix");
        let stored = graph.relation(&relation).unwrap();
        p.eq("assumptions", stored.assumptions.clone(), vec![theorem_meaning.clone()]);
        p.eq("evidence", stored.evidence.len(), 1);
        p.eq("objects", graph.objects().count(), 2);
        p.eq("relations", graph.relations().count(), 1);
        p.eq("source", graph.object(&stored.source).unwrap().meaning_id.clone(), proof_meaning);
    });
    p.case("refuse", |p| {
        let mut graph = ObjectGraph::default();
        p.eq("empty-kind", graph.put(object(ObjectKind::Custom(" ".to_string()), "x", "x")), Err(StoreGraphError::EmptyCustomKind));
        let source = graph.put(object(ObjectKind::Method, "method", "method")).unwrap();
        let missing = ObjectId::from_bytes(b"missing");
        p.eq(
            "missing-endpoint",
            graph.add_relation(RelationDraft { kind: RelationKind::DependsOn, source, target: missing.clone(), scope: RelationScope::Global, assumptions: Vec::new(), authority: None, evidence: Vec::new() }),
            Err(StoreGraphError::MissingObject(missing)),
        );
    });
    p.finish();
}
