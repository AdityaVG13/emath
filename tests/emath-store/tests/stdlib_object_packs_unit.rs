//! Bead `emath-stdlib-object-packs-hpzgf` — standard library as
//! executable object packs.
//!
//! `std.core` objects load as `.emlib` packs with MeaningIDs, not
//! source-only files: one pack carries the census (a theory object, a
//! cell object, an independent evidence receipt), export is
//! deterministic, a second workspace mounts the same pack with zero
//! source duplication, forged evidence or a forged object refuses
//! typed, presentation edits never move MeaningID while law edits do,
//! and stdlib names smuggled as nucleus operation enums fail the
//! core-growth gate.
//!
//! Targeted verify: `cargo test -p emath-store-tests --test
//! stdlib_object_packs_unit` (a bare name filter matches no test names
//! and would report a vacuous green).

use emath_core::{EvidenceId, MeaningId, ObjectId, PackId};
use emath_sema::CompilerSession;
use emath_store::evidence_plane::EvidenceReceipt;
use emath_store::object_graph::{ObjectGraph, ObjectKind};
use emath_store::pack::PackEntry;
use emath_store::semantic_diff::{ChangeClass, SemanticSnapshot, classify};
use emath_store::stdlib::{
    StdMountError, StdObject, StdReceipt, export_std_pack, mount_stdlib,
};
use emath_syntax::install_source_parser;
use std::str::FromStr;

/// The std.core theory: what the shape law claims.
const THEORY_SOURCE: &str = "emath function shape_law:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n";
/// The std.core cell: the reference algorithm — the same law, computed
/// as a distinct expression tree, so theory and algorithm carry
/// independent MeaningIDs (semantic meaning, not source text).
const CELL_SOURCE: &str = "emath function square_ref:\n    inputs:\n        x: Float64\n    definitions:\n        y = (x * x) + 0.0\n";

/// Admitted meaning identity plus its canonical semantic payload: both
/// derived from the ADMITTED package, never from raw source bytes.
fn meaning_of(source: &str) -> (MeaningId, Vec<u8>) {
    install_source_parser();
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned("stdlib-pack.emath", source);
    let errors = result
        .diagnostics
        .errors()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "source must admit: {errors:#?}");
    let id = result
        .package
        .meaning_id(&[])
        .expect("admitted package must carry a meaning id");
    let payload = emath_ir::meaning::canonical_meaning_bytes(&result.package, &[]).unwrap();
    (id, payload)
}

/// The std.core census as a pack entry list: one theory object, one
/// cell object, one independent evidence receipt attached to the cell.
/// Insertion order varies by caller; export canonicalizes.
fn std_core_entries<F>(mut on_ids: F) -> Vec<PackEntry>
where
    F: FnMut(&ObjectId, &ObjectId, &EvidenceId),
{
    let (theory_meaning, theory_payload) = meaning_of(THEORY_SOURCE);
    let (cell_meaning, cell_payload) = meaning_of(CELL_SOURCE);
    let theory = StdObject {
        kind: ObjectKind::Theory,
        meaning_id: theory_meaning,
        semantic_payload: theory_payload,
        presentation: Some("std.core theory: shape law".into()),
    };
    let cell = StdObject {
        kind: ObjectKind::Cell,
        meaning_id: cell_meaning,
        semantic_payload: cell_payload,
        presentation: Some("std.core.cells.square: reference algorithm".into()),
    };
    // ObjectIds are content-derived; mint them through the same graph
    // the mount will build (deterministic).
    let mut scratch = ObjectGraph::default();
    let theory_id = scratch.put(theory.to_draft()).expect("theory drafts");
    let cell_id = scratch.put(cell.to_draft()).expect("cell drafts");
    let receipt = StdReceipt {
        kind: "algorithm-test".into(),
        payload: b"square(3) == 9".to_vec(),
        object_id: cell_id.clone(),
    };
    let receipt_id = EvidenceReceipt::seal("algorithm-test", b"square(3) == 9").evidence_id;
    on_ids(&theory_id, &cell_id, &receipt_id);
    vec![
        PackEntry::new(theory_id.as_str(), &theory.encode()),
        PackEntry::new(cell_id.as_str(), &cell.encode()),
        PackEntry::new(receipt_id.as_str(), &receipt.encode()),
    ]
}

/// The canonical pack bytes for the std.core census.
fn std_core_pack() -> Vec<u8> {
    export_std_pack(&std_core_entries(|_, _, _| {})).expect("census pack writes")
}

/// Hand-build container bytes in the draft `.emlib` framing with an
/// explicit physical entry order (the writer always sorts; the reader
/// preserves what is there — a third-party pack may not be sorted).
fn pack_bytes_with_physical_order(entries: &[PackEntry]) -> Vec<u8> {
    fn frame(bytes: &mut Vec<u8>, value: &[u8]) {
        bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
        bytes.extend_from_slice(value);
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"EMATHLIB\0");
    bytes.push(1);
    frame(&mut bytes, &[]);
    frame(&mut bytes, &u64::try_from(entries.len()).unwrap_or(u64::MAX).to_be_bytes());
    for entry in entries {
        frame(&mut bytes, entry.id.as_bytes());
        frame(&mut bytes, &entry.payload);
    }
    bytes
}

#[test]
fn mount_is_independent_of_container_entry_order() {
    // The container preserves physical entry order; only the writer
    // sorts. A third-party pack may list an evidence receipt BEFORE the
    // object it attaches to. Mount must be order-independent: objects
    // are interned before receipts are attached, so a physically
    // reordered pack mounts to the identical graph and evidence view —
    // never a spurious "unknown object" refusal.
    let mut ids_out = None;
    let entries = std_core_entries(|theory, cell, receipt| {
        ids_out = Some((
            theory.as_str().to_string(),
            cell.as_str().to_string(),
            receipt.as_str().to_string(),
        ));
    });
    let (theory_id, cell_id, receipt_id) = ids_out.expect("callback captured the ids");

    // Same entry set, physical order receipt-first (before BOTH objects).
    let mut reordered: Vec<PackEntry> = entries.clone();
    reordered.sort_by_key(|entry| {
        // receipt first, then the rest by id for a deterministic order
        (entry.id != receipt_id, entry.id.clone())
    });
    assert_eq!(reordered[0].id, receipt_id, "receipt is physically first");
    let bytes = pack_bytes_with_physical_order(&reordered);

    let reordered_mount = mount_stdlib(&bytes).expect("receipt-first pack must mount");
    let canonical_mount = mount_stdlib(&std_core_pack()).expect("canonical pack mounts");

    let ids = |mount: &emath_store::stdlib::StdMount| -> Vec<String> {
        mount
            .graph
            .objects()
            .map(|o| o.id.as_str().to_string())
            .collect()
    };
    assert_eq!(ids(&reordered_mount), ids(&canonical_mount));
    assert_eq!(ids(&reordered_mount).len(), 2);

    // The receipt attaches to the cell in both mounts — identical
    // evidence view regardless of container order.
    let cell = emath_core::ObjectId::from_str(&cell_id).expect("cell id");
    assert_eq!(
        reordered_mount.evidence.attachments_of(&cell),
        canonical_mount.evidence.attachments_of(&cell),
    );
    assert_eq!(
        reordered_mount.evidence.attachments_of(&cell).len(),
        1,
        "the receipt attached to the cell despite arriving first"
    );
    assert_eq!(reordered_mount.pack_id, canonical_mount.pack_id);
    // The ids used above are the mounted ones; sanity-check the
    // callback ids match the mounted graph.
    assert!(ids(&reordered_mount).contains(&cell_id));
    assert!(ids(&reordered_mount).contains(&theory_id));
}

#[test]
fn std_core_pack_mounts_with_meaning_ids() {
    // The positive unit: std.core objects load as .emlib pack entries
    // with MeaningIDs, not source-only files — the mounted graph carries
    // the same admitted meaning ids and canonical payloads the census
    // derived from compiled packages, and the algorithm receipt is an
    // independent evidence object attached to the cell only.
    let bytes = std_core_pack();
    let mount = mount_stdlib(&bytes).expect("census pack must mount");
    assert_eq!(mount.pack_id, PackId::from_bytes(&bytes));

    let objects: Vec<_> = mount.graph.objects().collect();
    assert_eq!(objects.len(), 2, "theory + cell");
    let (theory_meaning, theory_payload) = meaning_of(THEORY_SOURCE);
    let (cell_meaning, cell_payload) = meaning_of(CELL_SOURCE);

    let theory = objects
        .iter()
        .find(|o| o.kind == ObjectKind::Theory)
        .expect("theory object");
    assert_eq!(theory.meaning_id, theory_meaning);
    assert_eq!(theory.semantic_payload, theory_payload);

    let cell = objects
        .iter()
        .find(|o| o.kind == ObjectKind::Cell)
        .expect("cell object");
    assert_eq!(cell.meaning_id, cell_meaning);
    assert_eq!(cell.semantic_payload, cell_payload);
    assert_ne!(
        theory_meaning, cell_meaning,
        "theory and algorithm carry independent MeaningIDs"
    );

    // Evidence bound to the cell only; the theory carries none — a
    // green algorithm test never stamps the theory proved.
    let attachments = mount.evidence.attachments_of(&cell.id);
    assert_eq!(attachments.len(), 1, "one algorithm-test receipt");
    assert_eq!(
        mount.evidence.attachments_of(&theory.id),
        Vec::<EvidenceId>::new(),
        "theory has no evidence attached"
    );
}

#[test]
fn deterministic_export_import_round_trip() {
    // Canonical export: insertion order never changes the bytes; import
    // returns the identical entry set; re-export reproduces the bytes —
    // a pure function of the object set.
    let forward = std_core_entries(|_, _, _| {});
    let mut reversed = forward.clone();
    reversed.reverse();
    let bytes = export_std_pack(&forward).expect("forward writes");
    let reversed_bytes = export_std_pack(&reversed).expect("reversed writes");
    assert_eq!(bytes, reversed_bytes, "canonical export ignores order");

    let read = emath_store::pack::PackReader::new(emath_store::pack::PackBudgets::draft())
        .read(&bytes, None)
        .expect("import reads");
    // The reader materializes entries in canonical (sorted) id order;
    // the written entry set is order-irrelevant by design.
    let mut expected = forward.clone();
    expected.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(read, expected, "round trip preserves the entry set");
    assert_eq!(
        export_std_pack(&read).expect("re-export writes"),
        bytes,
        "re-export is byte-identical"
    );
}

#[test]
fn second_workspace_mounts_same_pack_no_source_duplication() {
    // Two workspaces mount ONE pack: the bytes are the single source —
    // identical PackId, identical object id sets, identical meaning ids
    // and payloads — with no per-workspace source copy or compiler
    // branch.
    let bytes = std_core_pack();
    let first = mount_stdlib(&bytes).expect("workspace one mounts");
    let second = mount_stdlib(&bytes).expect("workspace two mounts");

    assert_eq!(first.pack_id, second.pack_id);
    let ids = |mount: &emath_store::stdlib::StdMount| -> Vec<String> {
        let mut ids: Vec<String> = mount
            .graph
            .objects()
            .map(|o| o.id.as_str().to_string())
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(&first), ids(&second), "identical object identity");

    let by_kind = |mount: &emath_store::stdlib::StdMount, kind: ObjectKind| {
        mount
            .graph
            .objects()
            .find(|o| o.kind == kind)
            .expect("object of kind")
            .clone()
    };
    let first_cell = by_kind(&first, ObjectKind::Cell);
    let second_cell = by_kind(&second, ObjectKind::Cell);
    assert_eq!(first_cell.meaning_id, second_cell.meaning_id);
    assert_eq!(first_cell.semantic_payload, second_cell.semantic_payload);

    // Mounting from the re-exported (imported) bytes yields the same
    // pack identity — the canonical form is the shareable artifact.
    let read = emath_store::pack::PackReader::new(emath_store::pack::PackBudgets::draft())
        .read(&bytes, None)
        .expect("import");
    let rebytes = export_std_pack(&read).expect("re-export");
    assert_eq!(rebytes, bytes);
}

#[test]
fn forged_evidence_and_forged_object_refuse() {
    // Negative: forged evidence — a receipt entry whose id seals payload
    // A but whose envelope carries payload B — refuses E-EVID-503 before
    // anything is stored; a forged object (entry id minted from one
    // draft, payload encoding a different object) refuses E-STD-002.
    let (cell_meaning, cell_payload) = meaning_of(CELL_SOURCE);
    let cell = StdObject {
        kind: ObjectKind::Cell,
        meaning_id: cell_meaning.clone(),
        semantic_payload: cell_payload.clone(),
        presentation: None,
    };
    let mut scratch = ObjectGraph::default();
    let cell_id = scratch.put(cell.to_draft()).expect("cell drafts");

    // Forged evidence: seal over payload A, encode payload B under that
    // id, in a pack that also carries the genuine target object (the
    // receipt must be refused for its HASH, not for a missing target).
    let genuine = EvidenceReceipt::seal("algorithm-test", b"square(3) == 9");
    let forged = StdReceipt {
        kind: "algorithm-test".into(),
        payload: b"square(4) == 16  /* tampered */".to_vec(),
        object_id: cell_id.clone(),
    };
    let forged_pack = export_std_pack(&[
        PackEntry::new(cell_id.as_str(), &cell.encode()),
        PackEntry::new(genuine.evidence_id.as_str(), &forged.encode()),
    ])
    .expect("forged pack writes");
    match mount_stdlib(&forged_pack) {
        Err(StdMountError::ForgedEvidence { code }) => assert_eq!(code, "E-EVID-503"),
        other => panic!("forged evidence must refuse E-EVID-503, got {other:?}"),
    }

    // Forgery detected at the plane even without a pack: tampered
    // payload under a stale seal.
    let mut tampered = genuine.clone();
    tampered.payload = b"square(999) == 999  /* tampered */".to_vec();
    let mut plane = emath_store::EvidencePlane::default();
    match plane.attach(&scratch, &cell_id, tampered) {
        Err(emath_store::evidence_plane::EvidencePlaneError::ForgedHash(code, _)) => {
            assert_eq!(code, "E-EVID-503")
        }
        other => panic!("tampered receipt must refuse E-EVID-503, got {other:?}"),
    }

    // Forged object: entry id minted from the cell draft, envelope
    // encoding a DIFFERENT object (mutated law payload) under a third
    // meaning.
    let (mutated_meaning, mutated_payload) = meaning_of(
        "emath function square_ref:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x * x\n",
    );
    let forged_object = StdObject {
        kind: ObjectKind::Cell,
        meaning_id: mutated_meaning,
        semantic_payload: mutated_payload,
        presentation: None,
    };
    let forged_object_pack = export_std_pack(&[PackEntry::new(
        cell_id.as_str(),
        &forged_object.encode(),
    )])
    .expect("forged object pack writes");
    match mount_stdlib(&forged_object_pack) {
        Err(StdMountError::ForgedObject {
            code,
            entry_id,
            recomputed,
        }) => {
            assert_eq!(code, "E-STD-002");
            assert_eq!(entry_id, cell_id.as_str());
            assert_ne!(recomputed, cell_id.as_str());
        }
        other => panic!("forged object must refuse E-STD-002, got {other:?}"),
    }
}

#[test]
fn presentation_vs_law_meaning_id_laws() {
    // Law: presentation edits (comments, spacing) PRESERVE MeaningID
    // (ChangeClass::Presentation); law edits MUTATE it
    // (ChangeClass::Meaning). Object update follows the same law: a
    // presentation-only re-put returns the same ObjectId and keeps the
    // first presentation — presentation never moves identity.
    let base = "emath function square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n";
    let presentation = format!("# docs only\n\n{base}\n");
    let law = "emath function square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x * x\n";

    let (base_meaning, _) = meaning_of(base);
    let (presentation_meaning, _) = meaning_of(&presentation);
    let (law_meaning, _) = meaning_of(law);
    assert_eq!(base_meaning, presentation_meaning, "presentation keeps meaning");
    assert_ne!(base_meaning, law_meaning, "law edit moves meaning");

    let before = SemanticSnapshot::new(
        emath_core::SourceId::from_bytes(base.as_bytes()),
        base_meaning.clone(),
        "stdlib-pack",
        &[],
    );
    let after_presentation = SemanticSnapshot::new(
        emath_core::SourceId::from_bytes(presentation.as_bytes()),
        presentation_meaning,
        "stdlib-pack",
        &[],
    );
    assert_eq!(
        classify(&before, &after_presentation),
        ChangeClass::Presentation
    );
    let after_law = SemanticSnapshot::new(
        emath_core::SourceId::from_bytes(law.as_bytes()),
        law_meaning,
        "stdlib-pack",
        &[],
    );
    assert_eq!(classify(&before, &after_law), ChangeClass::Meaning);

    // ObjectId stability under presentation-only changes.
    let object = |meaning: &MeaningId, presentation: &str| StdObject {
        kind: ObjectKind::Cell,
        meaning_id: meaning.clone(),
        semantic_payload: vec![],
        presentation: Some(presentation.into()),
    };
    let mut graph = ObjectGraph::default();
    let id_once = graph
        .put(object(&base_meaning, "first presentation").to_draft())
        .expect("first put");
    let id_twice = graph
        .put(object(&base_meaning, "second presentation").to_draft())
        .expect("second put");
    assert_eq!(id_once, id_twice, "presentation never moves ObjectId");
    assert_eq!(
        graph.object(&id_once).unwrap().presentation.as_deref(),
        Some("first presentation"),
        "first presentation wins deterministically"
    );
}

#[test]
fn stdlib_as_core_enum_negative() {
    // Negative: stdlib names smuggled as nucleus operation enums fail
    // the core-growth gate (E-GROWTH-001) — the committed fixture is
    // scanned as a seeded backend source, and the gate reports the
    // operation-name branch typed.
    let fixture = include_str!("../../../tests/invalid/stdlib_as_core_enum.emath");
    let report = emath_exec_ir::growth::growth_gate(
        &[("stdlib_as_core_enum.emath", fixture)],
        &["std.tensor.softmax"],
    );
    assert!(!report.passed(), "the gate must fail the seeded branch");
    assert_eq!(report.violations.len(), 1, "{:?}", report.violations);
    let violation = &report.violations[0];
    assert_eq!(violation.file, "stdlib_as_core_enum.emath");
    assert_eq!(violation.token, "std.tensor.softmax");
}

#[test]
fn custom_kind_names_survive_encode_decode_round_trip() {
    // Alias law: the `custom:` prefix is a NAMESPACE marker stripped
    // exactly once on decode. A custom kind whose own name contains
    // the prefix (`custom:proxy` -> wire name `custom:custom:proxy`)
    // must round-trip identically — a repeated-prefix trim would
    // collapse nested namespaces and silently alias two distinct kinds
    // onto one.
    let object = StdObject {
        kind: ObjectKind::Custom("custom:proxy".into()),
        meaning_id: MeaningId::from_bytes(b"stdlib-review-p2-meaning"),
        semantic_payload: vec![1, 2, 3],
        presentation: None,
    };
    let decoded = StdObject::decode(&object.encode()).expect("envelope round trip");
    assert_eq!(
        decoded.kind,
        ObjectKind::Custom("custom:proxy".into()),
        "nested custom-namespace names must survive the round trip exactly"
    );
    // The plain custom kind is unaffected either way.
    let plain = StdObject {
        kind: ObjectKind::Custom("proxy".into()),
        meaning_id: MeaningId::from_bytes(b"stdlib-review-p2-meaning"),
        semantic_payload: vec![],
        presentation: None,
    };
    let decoded_plain = StdObject::decode(&plain.encode()).expect("plain round trip");
    assert_eq!(decoded_plain.kind, ObjectKind::Custom("proxy".into()));
}

#[test]
fn duplicate_entry_ids_refuse_at_export() {
    // Canonical export needs an id SET: two entries claiming one id
    // refuse E-EVID-606 instead of silently picking a winner (the
    // pack would otherwise be ambiguous about which payload the id
    // names).
    let entries = std_core_entries(|_, _, _| {});
    let mut duplicated = entries.clone();
    duplicated.push(entries[0].clone());
    match export_std_pack(&duplicated) {
        Err(emath_store::pack::PackFault::DuplicateEntry { id, .. }) => {
            assert_eq!(id, entries[0].id, "the duplicated id is named");
        }
        other => panic!("duplicate ids must refuse E-EVID-606, got {other:?}"),
    }
}

#[test]
fn mount_is_idempotent_over_duplicate_identical_entries() {
    // Mount idempotency: a (non-canonical) third-party container
    // carrying duplicate IDENTICAL entries coalesces — content-addressed
    // identity makes a duplicate a no-op, never a collision and never a
    // double-attached evidence. (The writer refuses duplicates; the
    // reader accepts well-formed framed input, so the mount must stay
    // total and idempotent over it.)
    let mut ids_out = None;
    let entries = std_core_entries(|theory, cell, receipt| {
        ids_out = Some((
            theory.as_str().to_string(),
            cell.as_str().to_string(),
            receipt.as_str().to_string(),
        ));
    });
    let (_, cell_id, _) = ids_out.expect("callback captured the ids");
    let mut duplicated = entries.clone();
    duplicated.extend(entries.iter().cloned());
    let bytes = pack_bytes_with_physical_order(&duplicated);

    let mount = mount_stdlib(&bytes).expect("duplicate-identical entries must coalesce");
    assert_eq!(
        mount.graph.objects().count(),
        2,
        "objects coalesce by content identity"
    );
    let cell = emath_core::ObjectId::from_str(&cell_id).expect("cell id");
    assert_eq!(
        mount.evidence.attachments_of(&cell).len(),
        1,
        "a duplicated receipt attaches exactly once"
    );
}
