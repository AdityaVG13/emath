//! — CAPSTONE: the executable identity gate.

use emath_core::{MeaningId, SourceId};
use emath_store::EvidencePlane;
use emath_store::evidence_plane::EvidenceReceipt;
use emath_store::object_graph::{ObjectDraft, ObjectGraph, ObjectKind};
use emath_store::semantic_diff::{ChangeClass, SemanticSnapshot, classify, decide};
use emath_test_harness::{Probe, Source, boot};

const BASE: &str = "emath function square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n";

fn meaning_of(p: &mut Probe, name: &str, source: &str) -> MeaningId {
    Source::from_str(name, source)
        .must_admit(p)
        .package
        .meaning_id(&[])
        .expect("admitted package carries meaning")
}

#[test]
fn meaning_store() {
    boot();
    let mut p = Probe::new("presentation preserves MeaningID, breaking changes rebuild, evidence never retcons");
    let base_meaning = meaning_of(&mut p, "base", BASE);
    let presentation = format!("# presentation edit: comment + spacing only\n\n{BASE}\n");
    let pres_meaning = meaning_of(&mut p, "presentation", &presentation);
    p.case("presentation", |p| {
        p.eq("stable", pres_meaning.clone(), base_meaning.clone());
        p.ne("source-moved", SourceId::from_bytes(BASE.as_bytes()), SourceId::from_bytes(presentation.as_bytes()));
        let before = SemanticSnapshot::new(SourceId::from_bytes(BASE.as_bytes()), base_meaning.clone(), "specializer-12", &[]);
        let after = SemanticSnapshot::new(SourceId::from_bytes(presentation.as_bytes()), pres_meaning.clone(), "specializer-12", &[]);
        p.eq("class", classify(&before, &after), ChangeClass::Presentation);
        match decide(&before, &after, &[]) {
            emath_store::semantic_diff::DiffOutcome::Cutoff(r) => {
                p.eq("receipt", r.class.clone(), ChangeClass::Presentation);
                p.contains("reason", &r.reason, "meaning stable");
            }
            other => { p.fail("cutoff", format!("presentation must cut off, got {other:?}")); },
        }
    });
    p.case("breaking", |p| {
        let breaking = BASE.replace("y = x * x", "y = x * x + x");
        let breaking_meaning = meaning_of(&mut *p, "breaking", &breaking);
        p.ne("mutates", base_meaning.clone(), breaking_meaning.clone());
        let before = SemanticSnapshot::new(SourceId::from_bytes(BASE.as_bytes()), base_meaning.clone(), "specializer-12", &[]);
        let after = SemanticSnapshot::new(SourceId::from_bytes(breaking.as_bytes()), breaking_meaning, "specializer-12", &[]);
        p.eq("class", classify(&before, &after), ChangeClass::Meaning);
        match decide(&before, &after, &[]) {
            emath_store::semantic_diff::DiffOutcome::Rebuild(r) => {
                p.eq("receipt", r.class.clone(), ChangeClass::Meaning);
                p.contains("scope", &r.reason, "dependents");
            }
            other => { p.fail("rebuild", format!("meaning change must rebuild, got {other:?}")); },
        }
    });
    p.case("evidence", |p| {
        let mut graph = ObjectGraph::default();
        let cell = graph.put(ObjectDraft { kind: ObjectKind::Cell, meaning_id: base_meaning.clone(), semantic_payload: BASE.as_bytes().to_vec(), presentation: Some("square, formatted".to_string()) }).unwrap();
        let before = graph.object(&cell).unwrap().meaning_id.clone();
        let mut plane = EvidencePlane::default();
        let attached = plane.attach(&graph, &cell, EvidenceReceipt::seal("capstone-receipt", b"unit gate run: ok")).unwrap();
        p.eq("no-retcon", graph.object(&cell).unwrap().meaning_id.clone(), before);
        p.eq("attached", plane.attachments_of(&cell), vec![attached.clone()]);
        let snap_before = SemanticSnapshot::new(SourceId::from_bytes(BASE.as_bytes()), base_meaning.clone(), "specializer-12", &[]);
        let snap_after = SemanticSnapshot::new(SourceId::from_bytes(BASE.as_bytes()), base_meaning.clone(), "specializer-12", &[attached.as_str()]);
        p.eq("class", classify(&snap_before, &snap_after), ChangeClass::Evidence);
        match decide(&snap_before, &snap_after, &[]) {
            emath_store::semantic_diff::DiffOutcome::Cutoff(r) => { p.eq("receipt", r.class.clone(), ChangeClass::Evidence); },
            other => { p.fail("cutoff", format!("evidence-only must cut off, got {other:?}")); },
        }
    });
    p.case("gate", |p| {
        let fixture = Source::from_workspace("tests/invalid/meaning_store_presentation_mutates_id.emath");
        p.contains("invariant", fixture.text(), "invariant: whitespace/comment edits preserve MeaningID");
        let fixture_meaning = fixture.must_admit(&mut *p).package.meaning_id(&[]).expect("fixture admits");
        p.eq("fixture-stable", fixture_meaning, base_meaning.clone());
    });
    p.finish();
}
