//!: single-ownership guarantees
//! after the magnet consolidation.

use emath_term::SymbolId;
use emath_test_harness::Probe;
use emath_world_ir::{FittedTable, WorldId, fnv1a64};
use std::collections::BTreeMap;

#[test]
fn fnv_fitted_ownership() {
    let mut p = Probe::new("fnv1a64 is the core primitive and FittedTable round-trips");
    p.case("hash", |p| {
        p.eq("empty", fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        p.eq("a", fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        p.eq("foobar", fnv1a64(b"foobar"), 0x85944171f73967e8);
        let input = b"world://demo/morphisms/identity";
        p.eq("parity", fnv1a64(input), emath_core::fnv1a64_bytes(input));
        p.eq("world-id", WorldId(fnv1a64(input)).0, fnv1a64(input));
    });
    p.case("table", |p| {
        let mut cells = BTreeMap::new();
        cells.insert(vec!["a".to_string(), "b".to_string()], "ab".to_string());
        cells.insert(vec!["b".to_string(), "a".to_string()], "ba".to_string());
        let table = FittedTable::from_cells(SymbolId("op.mul".to_string()), 2, cells);
        p.eq("hit", table.get(&["a".to_string(), "b".to_string()]), Some("ab"));
        p.eq("arity", table.get(&["a".to_string()]), None);
        p.eq(
            "order",
            table.cells().map(|(ins, out)| format!("{}:{}", ins.join(","), out)).collect::<Vec<_>>(),
            vec!["a,b:ab".to_string(), "b,a:ba".to_string()],
        );
        p.contains("prefix", &table.canonical(), "table:op.mul:arity=2:");
        p.eq("stable", table.canonical(), table.canonical());
    });
    p.finish();
}
