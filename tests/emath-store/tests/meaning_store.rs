//! — CAPSTONE: the executable identity gate.
//!
//! Proves the meaning store distinguishes the four change classes end
//! to end, through the landed layers (MeaningID, evidence
//! plane, semantic diff):
//! - presentation edits (whitespace/comments) PRESERVE MeaningID and
//!   cut off the rebuild with a receipt;
//! - breaking changes MUTATE MeaningID and rebuild dependents — never a
//!   silent cutoff;
//! - evidence attachments are INDEPENDENT (no meaning retcon);
//! - a presentation change that mutated MeaningID would FAIL this gate.
//!
//! Identity first, UX capstone later. No incremental-compilation
//! completeness is claimed here.

use emath_core::{MeaningId, SourceId};
use emath_sema::CompilerSession;
use emath_store::EvidencePlane;
use emath_store::evidence_plane::EvidenceReceipt;
use emath_store::object_graph::{ObjectDraft, ObjectGraph, ObjectKind};
use emath_store::semantic_diff::{ChangeClass, SemanticSnapshot, classify, decide};
use emath_syntax::install_source_parser;

const BASE_SOURCE: &str = "emath function square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n";

/// The admitted meaning identity: derived from the ADMITTED package
/// (canonical semantic payload), never from the raw source bytes.
fn meaning_of(source: &str) -> MeaningId {
    install_source_parser();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned("meaning-store.emath", source);
    let errors = result
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "source must admit: {errors:#?}");
    result
        .package
        .meaning_id(&[])
        .expect("admitted package must carry a meaning id")
}

fn source_id_of(source: &str) -> SourceId {
    SourceId::from_bytes(source.as_bytes())
}

fn snapshot(source: &str, evidence: &[&str]) -> SemanticSnapshot {
    SemanticSnapshot::new(
        source_id_of(source),
        meaning_of(source),
        "specializer-12",
        evidence,
    )
}

/// Presentation edit (comment + blank lines) keeps MeaningID: the
/// source ids differ, the meaning ids do not — and the diff classifies
/// Presentation and cuts off with a receipt naming the stable meaning.
#[test]
fn presentation_edit_preserves_meaning_id_and_cuts_off() {
    let presentation = format!("# presentation edit: comment + spacing only\n\n{BASE_SOURCE}\n");
    let base_meaning = meaning_of(BASE_SOURCE);
    let presentation_meaning = meaning_of(&presentation);
    assert_eq!(
        base_meaning, presentation_meaning,
        "a whitespace/comment edit must preserve MeaningID"
    );
    assert_ne!(
        source_id_of(BASE_SOURCE),
        source_id_of(&presentation),
        "the source identity moved (presentation tier)"
    );

    let before = snapshot(BASE_SOURCE, &[]);
    let after = snapshot(&presentation, &[]);
    assert_eq!(classify(&before, &after), ChangeClass::Presentation);
    match decide(&before, &after, &[]) {
        emath_store::semantic_diff::DiffOutcome::Cutoff(receipt) => {
            assert_eq!(receipt.class, ChangeClass::Presentation);
            assert!(receipt.reason.contains("meaning stable"));
        }
        other => panic!("presentation-only must cut off, got {other:?}"),
    }
}

/// A breaking change (y = x*x + x) mutates MeaningID and the diff
/// rebuilds — the semantic change is never silently cut off, even
/// though the source changed too.
#[test]
fn breaking_change_mutates_meaning_id_and_rebuilds() {
    let breaking = BASE_SOURCE.replace("y = x * x", "y = x * x + x");
    assert_ne!(
        meaning_of(BASE_SOURCE),
        meaning_of(&breaking),
        "a semantics change must mutate MeaningID"
    );
    let before = snapshot(BASE_SOURCE, &[]);
    let after = snapshot(&breaking, &[]);
    assert_eq!(classify(&before, &after), ChangeClass::Meaning);
    match decide(&before, &after, &[]) {
        emath_store::semantic_diff::DiffOutcome::Rebuild(receipt) => {
            assert_eq!(receipt.class, ChangeClass::Meaning);
            assert!(receipt.reason.contains("dependents"));
        }
        other => panic!("a meaning change must rebuild, got {other:?}"),
    }
}

/// Evidence attachments are independent: attaching a receipt to a
/// stored object changes NEITHER its MeaningID (no retcon) NOR its
/// ObjectID; the diff sees only an additive evidence change.
#[test]
fn evidence_attach_does_not_retcon_meaning() {
    let mut graph = ObjectGraph::default();
    let draft = ObjectDraft {
        kind: ObjectKind::Cell,
        meaning_id: meaning_of(BASE_SOURCE),
        semantic_payload: BASE_SOURCE.as_bytes().to_vec(),
        presentation: Some("square, formatted".to_string()),
    };
    let cell = graph.put(draft).expect("object must store");
    let meaning_before = graph.object(&cell).unwrap().meaning_id.clone();

    let mut plane = EvidencePlane::default();
    let receipt = EvidenceReceipt::seal("capstone-receipt", b"unit gate run: ok");
    let attached = plane
        .attach(&graph, &cell, receipt)
        .expect("attach must work");

    let stored = graph.object(&cell).unwrap();
    assert_eq!(stored.meaning_id, meaning_before, "no meaning retcon");
    assert_eq!(plane.attachments_of(&cell), vec![attached.clone()]);

    // The diff sees the attachment as ADDITIVE evidence only.
    let before = SemanticSnapshot::new(
        source_id_of(BASE_SOURCE),
        meaning_of(BASE_SOURCE),
        "specializer-12",
        &[],
    );
    let after = SemanticSnapshot::new(
        source_id_of(BASE_SOURCE),
        meaning_of(BASE_SOURCE),
        "specializer-12",
        &[attached.as_str()],
    );
    assert_eq!(classify(&before, &after), ChangeClass::Evidence);
    match decide(&before, &after, &[]) {
        emath_store::semantic_diff::DiffOutcome::Cutoff(receipt) => {
            assert_eq!(receipt.class, ChangeClass::Evidence);
        }
        other => panic!("evidence-only change must cut off, got {other:?}"),
    }
}

/// NEGATIVE (the executable gate): the fixture IS a whitespace/comment
/// edit of the base source. If any admission path ever retcons
/// presentation into meaning, this gate FAILS — the fixture's meaning
/// must equal the base source's meaning.
#[test]
fn presentation_change_that_mutates_meaning_id_fails_the_gate() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/meaning_store_presentation_mutates_id.emath"
    ));
    assert!(
        fixture.contains("invariant: whitespace/comment edits preserve MeaningID"),
        "fixture must document the capstone invariant"
    );
    // The gate: the presentation edit preserves the meaning identity.
    // (If MeaningID mutated, this assert fails — the negative has no
    // silent-success path.)
    assert_eq!(
        meaning_of(BASE_SOURCE),
        meaning_of(fixture),
        "a whitespace/comment edit mutated MeaningID — the identity gate FAILS"
    );
}
