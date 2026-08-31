use emath_core::{EvidenceId, MeaningId, ObjectId};
use emath_store::{
    ObjectDraft, ObjectGraph, ObjectKind, RelationDraft, RelationKind, RelationScope,
    StoreGraphError,
};

fn object(kind: ObjectKind, meaning: &str, presentation: &str) -> ObjectDraft {
    ObjectDraft {
        kind,
        meaning_id: MeaningId::from_bytes(meaning.as_bytes()),
        semantic_payload: meaning.as_bytes().to_vec(),
        presentation: Some(presentation.to_string()),
    }
}

#[test]
fn object_envelope_and_typed_relation_graph_are_deterministic() {
    let theorem_meaning = MeaningId::from_bytes(b"theorem");
    let proof_meaning = MeaningId::from_bytes(b"proof");
    let mut graph = ObjectGraph::default();
    let theorem = graph
        .put(object(ObjectKind::Cell, "theorem", "Theorem, formatted"))
        .unwrap();
    let same_theorem = graph
        .put(object(ObjectKind::Cell, "theorem", "Theorem: other prose"))
        .unwrap();
    assert_eq!(theorem, same_theorem, "presentation is not object identity");
    assert_eq!(graph.object(&theorem).unwrap().meaning_id, theorem_meaning);

    let proof = graph
        .put(object(ObjectKind::Proof, "proof", "A proof"))
        .unwrap();
    let evidence = EvidenceId::from_bytes(b"checked proof");
    let relation = graph
        .add_relation(RelationDraft {
            kind: RelationKind::Proves,
            source: proof,
            target: theorem.clone(),
            scope: RelationScope::Global,
            assumptions: vec![theorem_meaning.clone(), theorem_meaning.clone()],
            authority: Some("emath-checker".to_string()),
            evidence: vec![evidence.clone(), evidence],
        })
        .unwrap();
    assert!(relation.as_str().starts_with("emath:relation:v1:"));
    let stored = graph.relation(&relation).unwrap();
    assert_eq!(stored.assumptions, vec![theorem_meaning.clone()]);
    assert_eq!(stored.evidence.len(), 1);
    assert_eq!(graph.object(&theorem).unwrap().meaning_id, theorem_meaning);
    assert_eq!(graph.objects().count(), 2);
    assert_eq!(graph.relations().count(), 1);
    assert_eq!(
        graph.object(&stored.source).unwrap().meaning_id,
        proof_meaning
    );
}

#[test]
fn relation_refuses_missing_endpoint_and_empty_custom_kinds() {
    let mut graph = ObjectGraph::default();
    assert_eq!(
        graph.put(object(ObjectKind::Custom(" ".to_string()), "x", "x")),
        Err(StoreGraphError::EmptyCustomKind)
    );
    let source = graph
        .put(object(ObjectKind::Method, "method", "method"))
        .unwrap();
    let missing = ObjectId::from_bytes(b"missing");
    assert_eq!(
        graph.add_relation(RelationDraft {
            kind: RelationKind::DependsOn,
            source,
            target: missing.clone(),
            scope: RelationScope::Global,
            assumptions: Vec::new(),
            authority: None,
            evidence: Vec::new(),
        }),
        Err(StoreGraphError::MissingObject(missing))
    );
}
