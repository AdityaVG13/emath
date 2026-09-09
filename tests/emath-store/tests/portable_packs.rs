//! — CAPSTONE: portable `.emlib` envelopes.

use std::sync::Arc;

use emath_core::MeaningId;
use emath_store::Space;
use emath_store::object_graph::{ObjectDraft, ObjectGraph, ObjectKind};
use emath_store::pack::{PackBudgets, PackEntry, PackFault, PackReader, PackWriter};
use emath_test_harness::Probe;

fn object(meaning: &str, presentation: &str) -> ObjectDraft {
    ObjectDraft {
        kind: ObjectKind::Cell,
        meaning_id: MeaningId::from_bytes(meaning.as_bytes()),
        semantic_payload: meaning.as_bytes().to_vec(),
        presentation: Some(presentation.to_string()),
    }
}

fn entries() -> Vec<PackEntry> {
    vec![
        PackEntry::new("emath:meaning:v1:cell-a", b"payload-a"),
        PackEntry::new("emath:meaning:v1:cell-b", b"payload-b"),
    ]
}

#[test]
fn portable_packs() {
    let mut p = Probe::new("packs create canonically, verify by name, and mount offline");
    let budgets = PackBudgets::draft();
    p.case("create", |p| {
        let first = PackWriter::new(budgets.clone()).write(&entries(), None).unwrap();
        let mut shuffled = entries();
        shuffled.reverse();
        p.eq("canonical", PackWriter::new(budgets.clone()).write(&shuffled, None).unwrap(), first);
    });
    p.case("verify", |p| {
        let bytes = PackWriter::new(budgets.clone()).write(&entries(), None).unwrap();
        match PackReader::new(budgets.clone()).read(&bytes[..bytes.len() - 4], None) {
            Err(PackFault::Truncated { code }) => { p.eq("truncated", code, "E-EVID-603".to_string()); },
            other => { p.fail("truncated", format!("must refuse E-EVID-603, got {other:?}")); },
        }
        let mut mutated = bytes.clone();
        let last = mutated.len() - 1;
        mutated[last] ^= 0x01;
        let read = PackReader::new(budgets.clone()).read(&mutated, None).unwrap();
        let mut expected = entries();
        *expected.last_mut().unwrap().payload.last_mut().unwrap() ^= 0x01;
        p.eq("visible", read.last().unwrap().clone(), expected.last().unwrap().clone());
        p.ne("rebytes", PackWriter::new(budgets.clone()).write(&read, None).unwrap(), bytes);
    });
    p.case("mount", |p| {
        let bytes = PackWriter::new(budgets.clone()).write(&entries(), None).unwrap();
        let read = PackReader::new(budgets.clone()).read(&bytes, None).unwrap();
        let mut graph = ObjectGraph::default();
        let mut mounted = Vec::new();
        for entry in &read {
            mounted.push(graph.put(object(&entry.id, "mounted from .emlib")).unwrap());
        }
        let space = Space::new("fresh-workbench", Arc::new(graph.clone())).unwrap();
        p.eq("name", space.name().to_string(), "fresh-workbench".to_string());
        p.demand("verify", emath_store::LibraryLock::from_snapshot(&space.snapshot().unwrap(), mounted).verify(&graph).is_ok(), "fresh mount verifies");
    });
    p.case("thin-fixture", |p| {
        let parent = PackWriter::new(budgets.clone()).write(&[PackEntry::new("emath:meaning:v1:cell-a", b"payload-a")], None).unwrap();
        let thin = PackWriter::new(budgets.clone()).write(&[PackEntry::new("emath:meaning:v1:cell-b", b"payload-b")], Some("emath:meaning:v1:cell-a")).unwrap();
        match PackReader::new(budgets.clone()).read(&thin, None) {
            Err(PackFault::ThinWithoutParent { code }) => { p.eq("no-parent", code, "E-EVID-605".to_string()); },
            other => { p.fail("no-parent", format!("must refuse E-EVID-605, got {other:?}")); },
        }
        let merged = PackReader::new(budgets.clone()).read(&thin, Some(&parent)).unwrap();
        let ids: Vec<&str> = merged.iter().map(|e| e.id.as_str()).collect();
        p.demand("merged-a", ids.contains(&"emath:meaning:v1:cell-a"), "parent present");
        p.demand("merged-b", ids.contains(&"emath:meaning:v1:cell-b"), "delta present");
        let fixture = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/invalid/emlib_truncated_pack.bin"));
        p.demand("magic", fixture.starts_with(b"EMATHLIB\0"), "fixture is a cut pack");
        match PackReader::new(budgets.clone()).read(fixture, None) {
            Err(PackFault::Truncated { code }) => { p.eq("fixture", code, "E-EVID-603".to_string()); },
            other => { p.fail("fixture", format!("must refuse E-EVID-603, got {other:?}")); },
        }
    });
    p.finish();
}
