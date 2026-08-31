//! emath-mct-type-magnets-fnv-fitted-o7a6: single-ownership guarantees
//! after the magnet consolidation.
//!
//! Hash parity: `emath_world_ir::fnv1a64` IS Tier-0 core's
//! `fnv1a64_bytes` (thin re-export, zero duplicated primitive) — proven
//! against the known FNV-1a 64-bit test vectors, so every existing call
//! site computes byte-identical digests before/after the move.
//! FittedTable round-trip: the relocated shared leaf keeps its
//! construction, lookup, and canonical-form contract from its new
//! world-ir home.

use emath_term::SymbolId;
use emath_world_ir::{FittedTable, WorldId, fnv1a64};
use std::collections::BTreeMap;

/// Hash parity against known FNV-1a 64 vectors (empty string = offset
/// basis; "a" and "foobar" are the canonical reference digests) and
/// against core's primitive directly.
#[test]
fn fnv1a64_is_the_core_primitive_known_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325, "offset basis");
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
    // Parity with the single owner, byte for byte, on non-trivial input.
    let input = b"world://demo/morphisms/identity";
    assert_eq!(fnv1a64(input), emath_core::fnv1a64_bytes(input));
    // The digest still keys WorldId identity the same way.
    let id = WorldId(fnv1a64(input));
    assert_eq!(id.0, fnv1a64(input));
}

/// FittedTable round-trip from its new home: build, look up (arity
/// checked), iterate deterministically, canonicalize stably.
#[test]
fn fitted_table_round_trips_from_world_ir_home() {
    let mut cells = BTreeMap::new();
    cells.insert(vec!["a".to_string(), "b".to_string()], "ab".to_string());
    cells.insert(vec!["b".to_string(), "a".to_string()], "ba".to_string());
    let table = FittedTable::from_cells(SymbolId("op.mul".to_string()), 2, cells);
    // Lookup respects arity: right row hits, wrong arity misses.
    assert_eq!(
        table.get(&["a".to_string(), "b".to_string()]),
        Some("ab")
    );
    assert_eq!(table.get(&["a".to_string()]), None, "arity mismatch = miss");
    // Rows iterate in deterministic lexicographic order.
    let rows: Vec<String> = table
        .cells()
        .map(|(inputs, output)| format!("{}:{}", inputs.join(","), output))
        .collect();
    assert_eq!(rows, vec!["a,b:ab".to_string(), "b,a:ba".to_string()]);
    // Canonical form is deterministic and names operator + arity.
    let canonical = table.canonical();
    assert!(canonical.starts_with("table:op.mul:arity=2:"), "{canonical}");
    assert_eq!(canonical, table.canonical(), "canonicalization is stable");
}
