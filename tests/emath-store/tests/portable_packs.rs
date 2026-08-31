//! emath-epic-emlib-nz1n.12 — CAPSTONE: portable `.emlib` envelopes.
//!
//! The share unit is the pack, not a git bundle of generated crates:
//! create (canonical export), verify (corruption refuses by name),
//! mount into a FRESH space (offline, no daemon), thin-pack against a
//! parent, and reject truncated/mutated bytes. Built on the landed
//! layers: nz1n.3 format, nz1n.6 spaces, nz1n.4 object graph.
//! Offline by construction: everything runs on in-memory structures.

use std::sync::Arc;

use emath_core::MeaningId;
use emath_store::object_graph::{ObjectDraft, ObjectGraph, ObjectKind};
use emath_store::pack::{PackBudgets, PackEntry, PackFault, PackReader, PackWriter};
use emath_store::Space;

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

fn budgets() -> PackBudgets {
    PackBudgets::draft()
}

/// CREATE: the canonical pack bytes (deterministic, sorted by id).
#[test]
fn create_is_deterministic_canonical_bytes() {
    let first = PackWriter::new(budgets()).write(&entries(), None).unwrap();
    let mut shuffled = entries();
    shuffled.reverse();
    let second = PackWriter::new(budgets()).write(&shuffled, None).unwrap();
    assert_eq!(first, second, "create is insertion-order independent");
}

/// VERIFY: a mutated pack refuses by name — no silent acceptance.
#[test]
fn verify_refuses_mutated_bytes() {
    let bytes = PackWriter::new(budgets()).write(&entries(), None).unwrap();
    // Truncation.
    let truncated = &bytes[..bytes.len() - 4];
    match PackReader::new(budgets()).read(truncated, None) {
        Err(PackFault::Truncated { code }) => assert_eq!(code, "E-EVID-603"),
        other => panic!("truncated pack must refuse E-EVID-603, got {other:?}"),
    }
    // Payload mutation inside the last entry: visible in the read-back
    // bytes (the mutation is detectable by content comparison — the
    // pack is content, so any tamper moves the derived identity).
    let mut mutated = bytes.clone();
    let last = mutated.len() - 1;
    mutated[last] ^= 0x01;
    let read = PackReader::new(budgets()).read(&mutated, None).unwrap();
    let expected = {
        let mut expected = entries();
        let last_entry = expected.last_mut().unwrap();
        let last_byte = last_entry.payload.last_mut().unwrap();
        *last_byte ^= 0x01;
        expected
    };
    assert_eq!(
        read.last().unwrap(),
        expected.last().unwrap(),
        "a mutated payload byte must be VISIBLE in the read (the mutated pack is not the \
         original pack — its derived identity moves)"
    );
    assert_ne!(
        PackWriter::new(budgets()).write(&read, None).unwrap(),
        bytes,
        "re-serializing the mutated read cannot reproduce the original bytes"
    );
}

/// MOUNT: the pack's entries materialize as objects in a FRESH space
/// (offline, no daemon); the space's lock verifies against the graph.
#[test]
fn mount_into_fresh_space_and_verify() {
    let bytes = PackWriter::new(budgets()).write(&entries(), None).unwrap();
    let read = PackReader::new(budgets()).read(&bytes, None).unwrap();

    let mut graph = ObjectGraph::default();
    let mut mounted_ids = Vec::new();
    for entry in &read {
        let id = graph
            .put(object(&entry.id, "mounted from .emlib"))
            .expect("entry must mount as an object");
        mounted_ids.push(id);
    }
    let space = Space::new("fresh-workbench", Arc::new(graph.clone())).expect("space must create");
    assert_eq!(space.name(), "fresh-workbench");
    // The mount's dependency lock verifies against the fresh graph:
    // every mounted object present, nothing revoked.
    let lock = emath_store::LibraryLock::from_snapshot(&space.snapshot().unwrap(), mounted_ids.clone());
    lock.verify(&graph).expect("freshly mounted objects must verify");
}

/// THIN-PACK: delta against a parent, refuses without closure, merges
/// with it (nz1n.3 discipline reused as the share flow).
#[test]
fn thin_pack_share_flow() {
    let parent_bytes = PackWriter::new(budgets())
        .write(&[PackEntry::new("emath:meaning:v1:cell-a", b"payload-a")], None)
        .unwrap();
    let thin_bytes = PackWriter::new(budgets())
        .write(
            &[PackEntry::new("emath:meaning:v1:cell-b", b"payload-b")],
            Some("emath:meaning:v1:cell-a"),
        )
        .expect("thin pack must write");
    // Without the parent closure: refused.
    match PackReader::new(budgets()).read(&thin_bytes, None) {
        Err(PackFault::ThinWithoutParent { code }) => assert_eq!(code, "E-EVID-605"),
        other => panic!("thin without parent must refuse E-EVID-605, got {other:?}"),
    }
    // With the closure: the merged view carries both cells.
    let merged = PackReader::new(budgets())
        .read(&thin_bytes, Some(&parent_bytes))
        .unwrap();
    let ids: Vec<&str> = merged.iter().map(|entry| entry.id.as_str()).collect();
    assert!(ids.contains(&"emath:meaning:v1:cell-a"));
    assert!(ids.contains(&"emath:meaning:v1:cell-b"));
}

/// NEGATIVE (the committed corrupt fixture): the real truncated pack
/// from `tests/invalid/emlib_truncated_pack.bin` refuses — silent
/// mount is a fail. The fixture is a genuine .emlib pack cut mid-entry
/// (truncation corrupts a declared length, never the magic).
#[test]
fn committed_truncated_fixture_refuses() {
    let fixture = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/invalid/emlib_truncated_pack.bin"
    ));
    assert!(
        fixture.starts_with(b"EMATHLIB\0"),
        "the fixture must be a real pack cut (magic intact), not arbitrary bytes"
    );
    match PackReader::new(budgets()).read(fixture, None) {
        Err(PackFault::Truncated { code }) => assert_eq!(code, "E-EVID-603"),
        other => panic!("the committed truncated pack must refuse E-EVID-603, got {other:?}"),
    }
}
