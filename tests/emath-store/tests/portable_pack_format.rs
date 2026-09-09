//! contracts: draft .emlib reader/writer,
//! corruption budgets, canonical export.

use emath_store::pack::{PackBudgets, PackEntry, PackFault, PackReader, PackWriter};
use emath_test_harness::Probe;

const MAGIC_LEN: usize = 9;

fn entries() -> Vec<PackEntry> {
    vec![
        PackEntry::new("emath:meaning:v1:aaaa", b"payload-two"),
        PackEntry::new("emath:meaning:v1:bbbb", b"payload-one"),
        PackEntry::new("emath:meaning:v1:cccc", b"payload-three"),
    ]
}

fn code(f: &PackFault) -> &str {
    match f {
        PackFault::BadMagic { code } => code,
        PackFault::Truncated { code } => code,
        PackFault::Oversized { code } => code,
        PackFault::ThinWithoutParent { code } => code,
        PackFault::DuplicateEntry { .. } => "E-EVID-606",
        other => panic!("unexpected fault {other:?}"),
    }
}

#[test]
fn portable_pack_format() {
    let mut p = Probe::new("packs round-trip canonically and corruption refuses by name");
    let budgets = PackBudgets::draft();
    p.case("round-trip", |p| {
        let bytes = PackWriter::new(budgets.clone()).write(&entries(), None).unwrap();
        p.eq("entries", PackReader::new(budgets.clone()).read(&bytes, None).unwrap(), entries());
        let shuffled = vec![
            PackEntry::new("emath:meaning:v1:cccc", b"payload-three"),
            PackEntry::new("emath:meaning:v1:aaaa", b"payload-two"),
            PackEntry::new("emath:meaning:v1:bbbb", b"payload-one"),
        ];
        p.eq("canonical", PackWriter::new(budgets.clone()).write(&shuffled, None).unwrap(), bytes.clone());
        let read_back = PackReader::new(budgets.clone()).read(&bytes, None).unwrap();
        let ids: Vec<&str> = read_back.iter().map(|e| e.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        p.eq("sorted", ids, sorted);
        p.eq("deterministic", PackWriter::new(budgets.clone()).write(&entries(), None).unwrap(), bytes);
    });
    p.case("corrupt", |p| {
        let bytes = PackWriter::new(budgets.clone()).write(&entries(), None).unwrap();
        let mut renamed = bytes.clone();
        renamed[0] = b'X';
        p.eq("magic", code(&PackReader::new(budgets.clone()).read(&renamed, None).unwrap_err()), "E-EVID-602");
        let mut late = bytes.clone();
        late[MAGIC_LEN - 1] = b'X';
        p.eq("magic-late", code(&PackReader::new(budgets.clone()).read(&late, None).unwrap_err()), "E-EVID-602");
        p.eq("truncated", code(&PackReader::new(budgets.clone()).read(&bytes[..bytes.len() - 3], None).unwrap_err()), "E-EVID-603");
        let tiny = PackBudgets { max_total_bytes: 4096, max_entries: 16, max_payload_bytes: 8 };
        p.eq("oversized", code(&PackWriter::new(tiny).write(&[PackEntry::new("emath:meaning:v1:aaaa", b"way-too-long-payload")], None).unwrap_err()), "E-EVID-604");
        let tiny_n = PackBudgets { max_total_bytes: 1 << 20, max_entries: 2, max_payload_bytes: 1 << 20 };
        p.eq("count", code(&PackWriter::new(tiny_n).write(&entries(), None).unwrap_err()), "E-EVID-604");
    });
    p.case("thin-verbatim", |p| {
        let parent = PackWriter::new(budgets.clone()).write(&[PackEntry::new("emath:meaning:v1:base", b"base-payload")], None).unwrap();
        let thin = PackWriter::new(budgets.clone()).write(&[PackEntry::new("emath:meaning:v1:delta", b"delta-payload")], Some("emath:meaning:v1:base")).unwrap();
        p.eq("no-parent", code(&PackReader::new(budgets.clone()).read(&thin, None).unwrap_err()), "E-EVID-605");
        let merged = PackReader::new(budgets.clone()).read(&thin, Some(&parent)).unwrap();
        let ids: Vec<&str> = merged.iter().map(|e| e.id.as_str()).collect();
        p.demand("has-base", ids.contains(&"emath:meaning:v1:base"), "parent merged");
        p.demand("has-delta", ids.contains(&"emath:meaning:v1:delta"), "delta merged");
        let id = "emath:meaning:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let bytes = PackWriter::new(budgets.clone()).write(&[PackEntry::new(id, b"payload")], None).unwrap();
        p.eq("verbatim", PackReader::new(budgets.clone()).read(&bytes, None).unwrap()[0].id.as_str(), id);
    });
    p.finish();
}
