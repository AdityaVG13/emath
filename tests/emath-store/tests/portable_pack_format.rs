//! emath-epic-emlib-nz1n.3 contracts: draft .emlib reader/writer,
//! corruption budgets, canonical export.
//!
//! The portable-pack draft: magic `EMATHLIB\0`, length-framed segments,
//! budgets that refuse corruption before it can masquerade as data, and
//! a canonical export (entries sorted by id). Thin packs carry a parent
//! reference; reading one WITHOUT its parent closure is a typed
//! refusal. Entry ids are carried verbatim — never re-derived from
//! payload bytes (re-deriving would mint a DIFFERENT identity).
//!
//! Draft status: no compression/decompression yet (the decompression
//! budget is a declared follow-up), invisible to `emath run` until
//! share/mount; must not stabilize before capstones.

use emath_store::pack::{PackBudgets, PackEntry, PackFault, PackReader, PackWriter};

const MAGIC_LEN: usize = 9;

fn entries() -> Vec<PackEntry> {
    vec![
        PackEntry::new("emath:meaning:v1:aaaa", b"payload-two"),
        PackEntry::new("emath:meaning:v1:bbbb", b"payload-one"),
        PackEntry::new("emath:meaning:v1:cccc", b"payload-three"),
    ]
}

fn budgets() -> PackBudgets {
    PackBudgets::draft()
}

/// Round-trip: write entries, read them back identically — object
/// payload survives the pack unchanged.
#[test]
fn round_trip_object_payload() {
    let bytes = PackWriter::new(budgets())
        .write(&entries(), None)
        .expect("full pack must write");
    let read = emath_store::pack::PackReader::new(budgets())
        .read(&bytes, None)
        .expect("full pack must read");
    assert_eq!(read, entries(), "round trip must preserve entries");
}

/// Canonical export: entry order in the bytes is by id, so two
/// insertion orders of the same entries produce IDENTICAL bytes, and
/// ids come back sorted.
#[test]
fn canonical_export_sorts_by_id() {
    let sorted_first = PackWriter::new(budgets()).write(&entries(), None).unwrap();
    let shuffled = vec![
        PackEntry::new("emath:meaning:v1:cccc", b"payload-three"),
        PackEntry::new("emath:meaning:v1:aaaa", b"payload-two"),
        PackEntry::new("emath:meaning:v1:bbbb", b"payload-one"),
    ];
    let shuffled_bytes = PackWriter::new(budgets()).write(&shuffled, None).unwrap();
    assert_eq!(
        sorted_first, shuffled_bytes,
        "canonical export must be insertion-order independent"
    );

    let read = emath_store::pack::PackReader::new(budgets())
        .read(&sorted_first, None)
        .unwrap();
    let ids: Vec<&str> = read.iter().map(|entry| entry.id.as_str()).collect();
    let mut expected = ids.clone();
    expected.sort_unstable();
    assert_eq!(ids, expected, "read order must be canonical (sorted by id)");
}

/// Corruption matrix: bad magic refuses — a renamed or corrupted magic
/// is never read as a pack (the check must cover the WHOLE magic, not a
/// prefix).
#[test]
fn bad_magic_refuses() {
    let bytes = PackWriter::new(budgets()).write(&entries(), None).unwrap();
    let mut renamed = bytes.clone();
    renamed[0] = b'X';
    match emath_store::pack::PackReader::new(budgets()).read(&renamed, None) {
        Err(PackFault::BadMagic { code }) => assert_eq!(code, "E-EVID-602"),
        other => panic!("bad magic must refuse E-EVID-602, got {other:?}"),
    }
    // Late-byte corruption inside the magic: a prefix-only check would
    // read this corrupted pack as data.
    let mut late = bytes.clone();
    late[MAGIC_LEN - 1] = b'X';
    match emath_store::pack::PackReader::new(budgets()).read(&late, None) {
        Err(PackFault::BadMagic { code }) => assert_eq!(code, "E-EVID-602"),
        other => panic!("late magic corruption must refuse E-EVID-602, got {other:?}"),
    }
}

/// Corruption matrix: a pack truncated mid-entry refuses — length
/// prefixes are never read past the buffer.
#[test]
fn truncated_refuses() {
    let bytes = PackWriter::new(budgets()).write(&entries(), None).unwrap();
    let truncated = &bytes[..bytes.len() - 3];
    match emath_store::pack::PackReader::new(budgets()).read(truncated, None) {
        Err(PackFault::Truncated { code }) => assert_eq!(code, "E-EVID-603"),
        other => panic!("truncated pack must refuse E-EVID-603, got {other:?}"),
    }
}

/// Budgets: an entry payload above the per-entry budget refuses at
/// write time (the pack never emits what the reader would have to
/// refuse).
#[test]
fn oversized_payload_refuses() {
    let tiny = PackBudgets {
        max_total_bytes: 4096,
        max_entries: 16,
        max_payload_bytes: 8,
    };
    let oversized = vec![PackEntry::new("emath:meaning:v1:aaaa", b"way-too-long-payload")];
    match PackWriter::new(tiny).write(&oversized, None) {
        Err(PackFault::Oversized { code }) => assert_eq!(code, "E-EVID-604"),
        other => panic!("oversized payload must refuse E-EVID-604, got {other:?}"),
    }
}

/// Budgets: an entry-count (ref-count) budget refuses at write time.
#[test]
fn entry_count_budget_refuses() {
    let tiny = PackBudgets {
        max_total_bytes: 1 << 20,
        max_entries: 2,
        max_payload_bytes: 1 << 20,
    };
    match PackWriter::new(tiny).write(&entries(), None) {
        Err(PackFault::Oversized { code }) => assert_eq!(code, "E-EVID-604"),
        other => panic!("over-budget entry count must refuse E-EVID-604, got {other:?}"),
    }
}

/// Thin packs: a pack written with a parent reference refuses to read
/// without the parent closure — the typed refusal, never a partial
/// silent read.
#[test]
fn thin_pack_without_parent_closure_refuses() {
    let parent = PackWriter::new(budgets())
        .write(&[PackEntry::new("emath:meaning:v1:base", b"base-payload")], None)
        .unwrap();
    let thin = PackWriter::new(budgets())
        .write(
            &[PackEntry::new("emath:meaning:v1:delta", b"delta-payload")],
            Some("emath:meaning:v1:base"),
        )
        .expect("thin pack must write with a parent reference");
    match emath_store::pack::PackReader::new(budgets()).read(&thin, None) {
        Err(PackFault::ThinWithoutParent { code }) => assert_eq!(code, "E-EVID-605"),
        other => panic!("thin without parent must refuse E-EVID-605, got {other:?}"),
    }

    // With the parent closure provided, the thin pack reads as the
    // merged view: parent entries plus the thin overlay.
    let merged = emath_store::pack::PackReader::new(budgets())
        .read(&thin, Some(&parent))
        .expect("thin pack must read with its parent closure");
    let ids: Vec<&str> = merged.iter().map(|entry| entry.id.as_str()).collect();
    assert!(ids.contains(&"emath:meaning:v1:base"));
    assert!(ids.contains(&"emath:meaning:v1:delta"));
}

/// Determinism: the same entries under the same budgets write to
/// identical bytes (a pack is content, not a session).
#[test]
fn pack_bytes_are_deterministic() {
    let first = PackWriter::new(budgets()).write(&entries(), None).unwrap();
    let second = PackWriter::new(budgets()).write(&entries(), None).unwrap();
    assert_eq!(first, second);
}

/// Entry ids are carried VERBATIM — the reader never re-derives an id
/// from the payload (re-derivation mints a different identity; the
/// nz1n.5 lesson).
#[test]
fn entry_ids_are_verbatim_not_rederived() {
    let id = "emath:meaning:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let bytes = PackWriter::new(budgets())
        .write(&[PackEntry::new(id, b"payload")], None)
        .unwrap();
    let read = emath_store::pack::PackReader::new(budgets())
        .read(&bytes, None)
        .unwrap();
    assert_eq!(read[0].id, id, "the id must round-trip byte-identical");
}
