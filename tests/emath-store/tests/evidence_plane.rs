//!: independent evidence plane — contract
//! tests.

use emath_core::{MeaningId, ObjectId};
use emath_store::{
    EvidencePlane, EvidencePlaneError, EvidenceReceipt, ObjectDraft, ObjectGraph, ObjectKind,
};
use emath_test_harness::Probe;

fn definition(meaning: &str, presentation: &str) -> ObjectDraft {
    ObjectDraft {
        kind: ObjectKind::Cell,
        meaning_id: MeaningId::from_bytes(meaning.as_bytes()),
        semantic_payload: meaning.as_bytes().to_vec(),
        presentation: Some(presentation.to_string()),
    }
}

fn sealed(payload: &[u8]) -> EvidenceReceipt {
    EvidenceReceipt::seal("test-receipt", payload)
}

fn forged_code(err: &EvidencePlaneError) -> &str {
    match err {
        EvidencePlaneError::ForgedHash(code, _) => code,
        other => panic!("expected ForgedHash, got {other:?}"),
    }
}

#[test]
fn evidence_plane() {
    let mut p = Probe::new("evidence attaches without moving meaning and forgeries refuse E-EVID-503");
    p.case("attach-stable", |p| {
        let mut graph = ObjectGraph::default();
        let cell = graph.put(definition("cell:quadratic-root", "Quadratic solver, formatted")).unwrap();
        let before = graph.object(&cell).unwrap().meaning_id.clone();
        let mut plane = EvidencePlane::default();
        let id = plane.attach(&graph, &cell, sealed(b"example: roots of x^2-4 with x=2")).unwrap();
        p.eq("id-stable", graph.object(&cell).unwrap().id.clone(), cell.clone());
        p.eq("meaning-stable", graph.object(&cell).unwrap().meaning_id.clone(), before);
        p.eq("attached", plane.attachments_of(&cell), vec![id.clone()]);
        let view = plane.receipt(&id).unwrap();
        p.eq("kind", view.kind, "test-receipt");
        p.eq("payload", view.payload, b"example: roots of x^2-4 with x=2" as &[u8]);
    });
    p.case("idempotent", |p| {
        let mut graph = ObjectGraph::default();
        let cell = graph.put(definition("cell:det", "Determinant")).unwrap();
        let mut plane = EvidencePlane::default();
        let first = plane.attach(&graph, &cell, sealed(b"unit test run")).unwrap();
        p.eq("reattach", plane.attach(&graph, &cell, sealed(b"unit test run")).unwrap(), first);
        p.eq("count1", plane.attachments_of(&cell).len(), 1);
        plane.attach(&graph, &cell, sealed(b"property test run")).unwrap();
        p.eq("count2", plane.attachments_of(&cell).len(), 2);
        let bare = graph.put(definition("cell:empty", "No evidence yet")).unwrap();
        p.eq("bare", plane.attachments_of(&bare).len(), 0);
    });
    p.case("forged", |p| {
        let mut graph = ObjectGraph::default();
        let cell = graph.put(definition("cell:sum", "Sum cell")).unwrap();
        let mut plane = EvidencePlane::default();
        let genuine = EvidenceReceipt::seal("proof-receipt", b"checked: sum commutes");
        let forged = EvidenceReceipt { payload: b"checked: sum DOES NOT commute".to_vec(), ..genuine.clone() };
        p.eq("hash", forged_code(&plane.attach(&graph, &cell, forged).unwrap_err()), "E-EVID-503");
        plane.attach(&graph, &cell, genuine.clone()).unwrap();
        p.eq("survives", plane.attachments_of(&cell).len(), 1);
        p.eq("payload", plane.receipt(&genuine.evidence_id).unwrap().payload, b"checked: sum commutes" as &[u8]);
        let mut plane2 = EvidencePlane::default();
        let cell2 = graph.put(definition("cell:collide", "Collision probe")).unwrap();
        plane2.attach(&graph, &cell2, EvidenceReceipt::seal("audit-receipt", b"first audit")).unwrap();
        let swapped = EvidenceReceipt { kind: "audit-receipt-v2".to_string(), ..EvidenceReceipt::seal("audit-receipt", b"first audit") };
        p.eq("swap", forged_code(&plane2.attach(&graph, &cell2, swapped).unwrap_err()), "E-EVID-503");
    });
    p.case("gates", |p| {
        let graph = ObjectGraph::default();
        let mut plane = EvidencePlane::default();
        let ghost = ObjectId::from_bytes(b"no-such-object");
        p.demand(
            "unknown",
            matches!(plane.attach(&graph, &ghost, sealed(b"orphan")).unwrap_err(), EvidencePlaneError::UnknownObject(id) if id == ghost),
            "missing object refuses",
        );
        p.demand(
            "empty-kind",
            matches!(plane.attach(&graph, &ObjectId::from_bytes(b"nothing"), EvidenceReceipt::seal("", b"x")).unwrap_err(), EvidencePlaneError::EmptyKind),
            "empty kind refuses",
        );
    });
    p.finish();
}
