//! — standard library as
//! executable object packs.

use emath_core::{EvidenceId, MeaningId, ObjectId, PackId};
use emath_store::evidence_plane::EvidenceReceipt;
use emath_store::object_graph::{ObjectGraph, ObjectKind};
use emath_store::pack::PackEntry;
use emath_store::semantic_diff::{ChangeClass, SemanticSnapshot, classify};
use emath_store::stdlib::{StdMountError, StdObject, StdReceipt, export_std_pack, mount_stdlib};
use emath_test_harness::{Probe, Source, boot};
use std::str::FromStr;

const THEORY_SOURCE: &str = "emath function shape_law:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n";
const CELL_SOURCE: &str = "emath function square_ref:\n    inputs:\n        x: Float64\n    definitions:\n        y = (x * x) + 0.0\n";

fn meaning_of(p: &mut Probe, name: &str, source: &str) -> (MeaningId, Vec<u8>) {
    let pkg = Source::from_str(name, source).must_admit(p).package;
    let id = pkg.meaning_id(&[]).expect("meaning id");
    let payload = emath_ir::meaning::canonical_meaning_bytes(&pkg, &[]).unwrap();
    (id, payload)
}

fn std_core_entries(p: &mut Probe) -> (Vec<PackEntry>, ObjectId, ObjectId, EvidenceId) {
    let (theory_meaning, theory_payload) = meaning_of(&mut *p, "theory", THEORY_SOURCE);
    let (cell_meaning, cell_payload) = meaning_of(&mut *p, "cell", CELL_SOURCE);
    let theory = StdObject { kind: ObjectKind::Theory, meaning_id: theory_meaning, semantic_payload: theory_payload, presentation: Some("std.core theory: shape law".into()) };
    let cell = StdObject { kind: ObjectKind::Cell, meaning_id: cell_meaning, semantic_payload: cell_payload, presentation: Some("std.core.cells.square: reference algorithm".into()) };
    let mut scratch = ObjectGraph::default();
    let theory_id = scratch.put(theory.to_draft()).expect("theory");
    let cell_id = scratch.put(cell.to_draft()).expect("cell");
    let receipt = StdReceipt { kind: "algorithm-test".into(), payload: b"square(3) == 9".to_vec(), object_id: cell_id.clone() };
    let receipt_id = EvidenceReceipt::seal("algorithm-test", b"square(3) == 9").evidence_id;
    let entries = vec![
        PackEntry::new(theory_id.as_str(), &theory.encode()),
        PackEntry::new(cell_id.as_str(), &cell.encode()),
        PackEntry::new(receipt_id.as_str(), &receipt.encode()),
    ];
    (entries, theory_id, cell_id, receipt_id)
}

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
fn stdlib_object_packs_unit() {
    boot();
    let mut p = Probe::new("std.core loads as addressed packs with typed forgery refusals");
    p.case("order-independent", |p| {
        let (entries, _, cell_id, receipt_id) = std_core_entries(&mut *p);
        let mut reordered = entries.clone();
        reordered.sort_by_key(|e| (e.id != receipt_id.as_str(), e.id.clone()));
        p.eq("receipt-first", reordered[0].id.clone(), receipt_id.as_str().to_string());
        let reordered_mount = mount_stdlib(&pack_bytes_with_physical_order(&reordered)).unwrap();
        let canonical_mount = mount_stdlib(&export_std_pack(&entries).unwrap()).unwrap();
        let ids = |m: &emath_store::stdlib::StdMount| m.graph.objects().map(|o| o.id.as_str().to_string()).collect::<Vec<_>>();
        p.eq("graphs", ids(&reordered_mount), ids(&canonical_mount));
        let cell = ObjectId::from_str(cell_id.as_str()).unwrap();
        p.eq("evidence", reordered_mount.evidence.attachments_of(&cell), canonical_mount.evidence.attachments_of(&cell));
        p.eq("count", reordered_mount.evidence.attachments_of(&cell).len(), 1);
        p.eq("pack", reordered_mount.pack_id.clone(), canonical_mount.pack_id.clone());
    });
    p.case("mount-meaning", |p| {
        let (entries, _, _, _) = std_core_entries(&mut *p);
        let bytes = export_std_pack(&entries).unwrap();
        let mount = mount_stdlib(&bytes).unwrap();
        p.eq("pack-id", mount.pack_id.clone(), PackId::from_bytes(&bytes));
        p.eq("objects", mount.graph.objects().count(), 2);
        let (theory_meaning, theory_payload) = meaning_of(&mut *p, "theory2", THEORY_SOURCE);
        let (cell_meaning, cell_payload) = meaning_of(&mut *p, "cell2", CELL_SOURCE);
        let theory = mount.graph.objects().find(|o| o.kind == ObjectKind::Theory).unwrap();
        p.eq("theory-meaning", theory.meaning_id.clone(), theory_meaning.clone());
        p.eq("theory-payload", theory.semantic_payload.clone(), theory_payload);
        let cell = mount.graph.objects().find(|o| o.kind == ObjectKind::Cell).unwrap();
        p.eq("cell-meaning", cell.meaning_id.clone(), cell_meaning.clone());
        p.eq("cell-payload", cell.semantic_payload.clone(), cell_payload);
        p.ne("independent", theory_meaning, cell_meaning);
        p.eq("cell-evidence", mount.evidence.attachments_of(&cell.id).len(), 1);
        p.eq("theory-bare", mount.evidence.attachments_of(&theory.id), Vec::<EvidenceId>::new());
    });
    p.case("deterministic-second", |p| {
        let (forward, _, _, _) = std_core_entries(&mut *p);
        let mut reversed = forward.clone();
        reversed.reverse();
        let bytes = export_std_pack(&forward).unwrap();
        p.eq("order", export_std_pack(&reversed).unwrap(), bytes.clone());
        let read = emath_store::pack::PackReader::new(emath_store::pack::PackBudgets::draft()).read(&bytes, None).unwrap();
        let mut expected = forward.clone();
        expected.sort_by(|a, b| a.id.cmp(&b.id));
        p.eq("round-trip", read.clone(), expected);
        p.eq("re-export", export_std_pack(&read).unwrap(), bytes.clone());
        let (first, second) = (mount_stdlib(&bytes).unwrap(), mount_stdlib(&bytes).unwrap());
        p.eq("pack-ids", first.pack_id.clone(), second.pack_id.clone());
        let ids = |m: &emath_store::stdlib::StdMount| { let mut v: Vec<String> = m.graph.objects().map(|o| o.id.as_str().to_string()).collect(); v.sort(); v };
        p.eq("workspaces", ids(&first), ids(&second));
        p.eq("rebytes", export_std_pack(&emath_store::pack::PackReader::new(emath_store::pack::PackBudgets::draft()).read(&bytes, None).unwrap()).unwrap(), bytes);
    });
    p.case("forged", |p| {
        let (cell_meaning, cell_payload) = meaning_of(&mut *p, "cell3", CELL_SOURCE);
        let cell = StdObject { kind: ObjectKind::Cell, meaning_id: cell_meaning, semantic_payload: cell_payload, presentation: None };
        let mut scratch = ObjectGraph::default();
        let cell_id = scratch.put(cell.to_draft()).unwrap();
        let genuine = EvidenceReceipt::seal("algorithm-test", b"square(3) == 9");
        let forged = StdReceipt { kind: "algorithm-test".into(), payload: b"square(4) == 16  /* tampered */".to_vec(), object_id: cell_id.clone() };
        let forged_pack = export_std_pack(&[PackEntry::new(cell_id.as_str(), &cell.encode()), PackEntry::new(genuine.evidence_id.as_str(), &forged.encode())]).unwrap();
        match mount_stdlib(&forged_pack) {
            Err(StdMountError::ForgedEvidence { code }) => { p.eq("evidence-code", code, "E-EVID-503".to_string()); },
            other => { p.fail("evidence-code", format!("must refuse E-EVID-503, got {other:?}")); },
        }
        let mut tampered = genuine.clone();
        tampered.payload = b"square(999) == 999  /* tampered */".to_vec();
        match emath_store::EvidencePlane::default().attach(&scratch, &cell_id, tampered) {
            Err(emath_store::evidence_plane::EvidencePlaneError::ForgedHash(code, _)) => { p.eq("plane-code", code, "E-EVID-503".to_string()); },
            other => { p.fail("plane-code", format!("must refuse E-EVID-503, got {other:?}")); },
        }
        let (mut_meaning, mut_payload) = meaning_of(&mut *p, "mutated", "emath function square_ref:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x * x\n");
        let forged_object = StdObject { kind: ObjectKind::Cell, meaning_id: mut_meaning, semantic_payload: mut_payload, presentation: None };
        match mount_stdlib(&export_std_pack(&[PackEntry::new(cell_id.as_str(), &forged_object.encode())]).unwrap()) {
            Err(StdMountError::ForgedObject { code, entry_id, recomputed }) => {
                p.eq("object-code", code, "E-STD-002".to_string());
                p.eq("entry", entry_id, cell_id.as_str().to_string());
                p.ne("recomputed", recomputed, cell_id.as_str().to_string());
            }
            other => { p.fail("object-code", format!("must refuse E-STD-002, got {other:?}")); },
        }
    });
    p.case("laws-misc", |p| {
        let base = "emath function square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x\n";
        let presentation = format!("# docs only\n\n{base}\n");
        let law = "emath function square:\n    inputs:\n        x: Float64\n    definitions:\n        y = x * x * x\n";
        let (base_m, _) = meaning_of(&mut *p, "base", base);
        let (pres_m, _) = meaning_of(&mut *p, "pres", &presentation);
        let (law_m, _) = meaning_of(&mut *p, "law", law);
        p.eq("pres-stable", base_m.clone(), pres_m.clone());
        p.ne("law-moves", base_m.clone(), law_m.clone());
        p.eq("pres-class", classify(&SemanticSnapshot::new(emath_core::SourceId::from_bytes(base.as_bytes()), base_m.clone(), "stdlib-pack", &[]), &SemanticSnapshot::new(emath_core::SourceId::from_bytes(presentation.as_bytes()), pres_m, "stdlib-pack", &[])), ChangeClass::Presentation);
        p.eq("law-class", classify(&SemanticSnapshot::new(emath_core::SourceId::from_bytes(base.as_bytes()), base_m.clone(), "stdlib-pack", &[]), &SemanticSnapshot::new(emath_core::SourceId::from_bytes(law.as_bytes()), law_m, "stdlib-pack", &[])), ChangeClass::Meaning);
        let report = emath_exec_ir::growth::growth_gate(&[("stdlib_as_core_enum.emath", include_str!("../../../tests/invalid/stdlib_as_core_enum.emath"))], &["std.tensor.softmax"]);
        p.demand("gate-fails", !report.passed(), "seeded branch must fail");
        p.eq("violations", report.violations.len(), 1);
        p.eq("token", report.violations[0].token.clone(), "std.tensor.softmax".to_string());
        let nested = StdObject { kind: ObjectKind::Custom("custom:proxy".into()), meaning_id: MeaningId::from_bytes(b"stdlib-review-p2-meaning"), semantic_payload: vec![1, 2, 3], presentation: None };
        p.eq("nested", StdObject::decode(&nested.encode()).unwrap().kind.clone(), ObjectKind::Custom("custom:proxy".into()));
        let (dup_entries, _, _, _) = std_core_entries(&mut *p);
        let mut duplicated = dup_entries.clone();
        duplicated.push(dup_entries[0].clone());
        match export_std_pack(&duplicated) {
            Err(emath_store::pack::PackFault::DuplicateEntry { id, .. }) => { p.eq("dup-id", id, dup_entries[0].id.clone()); },
            other => { p.fail("dup-id", format!("must refuse E-EVID-606, got {other:?}")); },
        }
        let mut doubled = dup_entries.clone();
        doubled.extend(dup_entries.iter().cloned());
        let mount = mount_stdlib(&pack_bytes_with_physical_order(&doubled)).unwrap();
        p.eq("coalesce", mount.graph.objects().count(), 2);
    });
    p.finish();
}
