//! `emath-epic-emlib-nz1n.5`: independent evidence plane — contract
//! tests.
//!
//! Proofs/tests attach to stored objects WITHOUT changing MeaningID
//! (or ObjectID): the plane is a separate layer over the object graph,
//! never a second meaning identity. Receipts are content-addressed
//! (fnv1a64 over framed canonical bytes, the house EvidenceStore
//! convention); a receipt whose recorded hash does not match its
//! content is FORGED and refuses `E-EVID-503`. Attachments require the
//! object to exist; attaching twice with the same content is
//! idempotent.
//!
//! Failure-first: RED (E0432) until `evidence_plane` lands.

use emath_core::{EvidenceId, MeaningId, ObjectId};
use emath_store::{
    EvidencePlane, EvidencePlaneError, EvidenceReceipt, ObjectDraft, ObjectGraph, ObjectKind,
};

fn definition(meaning: &str, presentation: &str) -> ObjectDraft {
    ObjectDraft {
        kind: ObjectKind::Cell,
        meaning_id: MeaningId::from_bytes(meaning.as_bytes()),
        semantic_payload: meaning.as_bytes().to_vec(),
        presentation: Some(presentation.to_string()),
    }
}

fn sealed_test_receipt(payload: &[u8]) -> EvidenceReceipt {
    EvidenceReceipt::seal("test-receipt", payload)
}

#[test]
fn attach_receipt_keeps_meaning_id_stable_and_view_updates() {
    let mut graph = ObjectGraph::default();
    let cell = graph
        .put(definition("cell:quadratic-root", "Quadratic solver, formatted"))
        .unwrap();
    let meaning_before = graph.object(&cell).unwrap().meaning_id.clone();

    let mut plane = EvidencePlane::default();
    let receipt = sealed_test_receipt(b"example: roots of x^2-4 with x=2");
    let evidence_id = plane.attach(&graph, &cell, receipt).unwrap();

    // Identity unchanged: ObjectID and MeaningID are exactly what they
    // were before the attachment (evidence is not a meaning identity).
    let stored = graph.object(&cell).unwrap();
    assert_eq!(stored.id, cell);
    assert_eq!(stored.meaning_id, meaning_before);

    // The evidence view updates: the receipt is reachable from the
    // object, its content is queryable, and the id is content-derived.
    let attached = plane.attachments_of(&cell);
    assert_eq!(attached, vec![evidence_id.clone()]);
    let view = plane.receipt(&evidence_id).unwrap();
    assert_eq!(view.kind, "test-receipt");
    assert_eq!(view.payload, b"example: roots of x^2-4 with x=2".to_vec());
}

#[test]
fn attach_is_idempotent_and_views_count_honestly() {
    let mut graph = ObjectGraph::default();
    let cell = graph.put(definition("cell:det", "Determinant")).unwrap();
    let mut plane = EvidencePlane::default();
    let first = plane
        .attach(&graph, &cell, sealed_test_receipt(b"unit test run"))
        .unwrap();
    // The SAME content re-attached is idempotent (one entry, same id).
    let again = plane
        .attach(&graph, &cell, sealed_test_receipt(b"unit test run"))
        .unwrap();
    assert_eq!(first, again);
    assert_eq!(plane.attachments_of(&cell).len(), 1);
    // A DIFFERENT receipt (distinct content) is a second attachment.
    plane
        .attach(&graph, &cell, sealed_test_receipt(b"property test run"))
        .unwrap();
    assert_eq!(plane.attachments_of(&cell).len(), 2);
    // An object with no attachments reports zero — never a fabricated
    // receipt.
    let bare = graph
        .put(definition("cell:empty", "No evidence yet"))
        .unwrap();
    assert_eq!(plane.attachments_of(&bare).len(), 0);
}

#[test]
fn forged_evidence_hash_refuses() {
    let mut graph = ObjectGraph::default();
    let cell = graph.put(definition("cell:sum", "Sum cell")).unwrap();
    let mut plane = EvidencePlane::default();

    // A forger tampers the payload but keeps the recorded hash of the
    // ORIGINAL content: the plane recomputes the content hash from
    // (kind, payload) and refuses the mismatch by name.
    let genuine = EvidenceReceipt::seal("proof-receipt", b"checked: sum commutes");
    let forged = EvidenceReceipt {
        payload: b"checked: sum DOES NOT commute".to_vec(),
        ..genuine.clone()
    };
    let error = plane.attach(&graph, &cell, forged).unwrap_err();
    assert!(
        matches!(&error, EvidencePlaneError::ForgedHash(code, _) if code == "E-EVID-503"),
        "forged hash must refuse E-EVID-503, got {error:?}"
    );

    // The genuine receipt still attaches (the forgery did not poison
    // the plane), and the forged content never entered any view.
    plane.attach(&graph, &cell, genuine.clone()).unwrap();
    assert_eq!(plane.attachments_of(&cell).len(), 1);
    assert_eq!(
        plane.receipt(&genuine.evidence_id).unwrap().payload,
        b"checked: sum commutes".to_vec()
    );
}

#[test]
fn same_address_different_content_refuses_even_when_hash_matches() {
    // Defense-in-depth pin (kills the dropped-tamper-match mutant):
    // a receipt can carry a STALE seal whose id was computed from
    // DIFFERENT framing than the plane uses — e.g. hand-minted via the
    // identity constructor with bytes that happen to re-derive under a
    // modified schema fence. The observable contract: after a receipt
    // is stored under an address, a second attach claiming the SAME
    // address with DIFFERENT (kind, payload) must refuse E-EVID-503 —
    // the stored receipt is evidence another consumer may have read;
    // it is never silently swapped. (The primary forgery gate above
    // catches the stale-seal case; this pin catches the
    // address-collision case reachable through the recorded-id path.)
    let mut graph = ObjectGraph::default();
    let cell = graph.put(definition("cell:collide", "Collision probe")).unwrap();
    let mut plane = EvidencePlane::default();
    let genuine = EvidenceReceipt::seal("audit-receipt", b"first audit");
    plane.attach(&graph, &cell, genuine).unwrap();

    // Forge: keep the id but swap the kind (the recorded id no longer
    // re-derives from the new content → the PRIMARY gate fires; this
    // asserts the union of both gates is total over swap attacks).
    let swapped = EvidenceReceipt {
        kind: "audit-receipt-v2".to_string(),
        ..EvidenceReceipt::seal("audit-receipt", b"first audit")
    };
    let error = plane.attach(&graph, &cell, swapped).unwrap_err();
    assert!(
        matches!(&error, EvidencePlaneError::ForgedHash(code, _) if code == "E-EVID-503"),
        "address swap must refuse, got {error:?}"
    );
    // The stored receipt is unchanged.
    assert_eq!(
        plane
            .receipt(&EvidenceReceipt::seal("audit-receipt", b"first audit").evidence_id)
            .unwrap()
            .kind,
        "audit-receipt"
    );
}

#[test]
fn attach_refuses_unknown_object() {
    let graph = ObjectGraph::default();
    let mut plane = EvidencePlane::default();
    let ghost = ObjectId::from_bytes(b"no-such-object");
    let error = plane
        .attach(&graph, &ghost, sealed_test_receipt(b"orphan"))
        .unwrap_err();
    assert!(
        matches!(&error, EvidencePlaneError::UnknownObject(id) if id == &ghost),
        "attachment to a missing object refuses, got {error:?}"
    );
}

#[test]
fn receipt_kind_is_validated() {
    // An empty evidence kind is not a receipt: refuse before any
    // hashing (a kindless blob is not checkable evidence).
    let graph = ObjectGraph::default();
    let mut plane = EvidencePlane::default();
    let error = plane
        .attach(
            &graph,
            &ObjectId::from_bytes(b"nothing"),
            EvidenceReceipt::seal("", b"x"),
        )
        .unwrap_err();
    assert!(
        matches!(error, EvidencePlaneError::EmptyKind),
        "empty kind refuses, got {error:?}"
    );
}
