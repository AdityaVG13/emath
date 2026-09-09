//! World-morphism quotient, homomorphism, and dedupe tests.

use emath_genesis::morphism::{
    MAX_ISO_SEARCH_SIZE, MORPHISM_VERSION, MorphismError, WorldMorphism, check_version, dedupe,
    find_isomorphism, mine_invariants, morphism_id, quotient, verify,
};
use emath_genesis::synth::OpTable;
use emath_test_harness::Probe;

fn xor_table() -> OpTable {
    OpTable { carrier_size: 2, cells: vec![0, 1, 1, 0] }
}

fn xnor_table() -> OpTable {
    OpTable { carrier_size: 2, cells: vec![1, 0, 0, 1] }
}

fn constant_table() -> OpTable {
    OpTable { carrier_size: 2, cells: vec![0, 0, 0, 0] }
}

fn and_table() -> OpTable {
    OpTable { carrier_size: 2, cells: vec![0, 0, 0, 1] }
}

#[test]
fn world_morphism() {
    let mut p = Probe::new("quotients, homomorphisms, and dedupe respect world structure");
    p.case("quotients", |p| {
        let xor = quotient(&xor_table()).expect("xor quotient");
        p.eq("classes", xor.classes, vec![vec![0], vec![1]]);
        p.eq("size", xor.table.carrier_size, 2);
        p.eq("cells", xor.table.cells, vec![0, 1, 1, 0]);
        p.eq("projection", xor.projection.map, vec![0, 1]);
        let constant = quotient(&constant_table()).expect("constant quotient");
        p.eq("classes", constant.classes, vec![vec![0, 1]]);
        p.eq("size", constant.table.carrier_size, 1);
        p.eq("cells", constant.table.cells, vec![0]);
        p.eq("projection", constant.projection.map, vec![0, 0]);
    });
    p.case("homomorphism", |p| {
        let xor = xor_table();
        let identity = WorldMorphism { source_size: 2, target_size: 2, map: vec![0, 1] };
        p.eq("identity", verify(&identity, &xor, &xor), Ok(()));
        let wrong = WorldMorphism { source_size: 2, target_size: 2, map: vec![1, 0] };
        p.eq("wrong-map", verify(&wrong, &xor, &xor), Err(MorphismError::NotAHomomorphism { pair: [0, 0] }));
    });
    p.case("isomorphism", |p| {
        let witness = find_isomorphism(&xor_table(), &xnor_table()).expect("budget").expect("iso");
        p.eq("map", witness.map.clone(), vec![1, 0]);
        p.eq("verifies", verify(&witness, &xor_table(), &xnor_table()), Ok(()));
        p.eq("none", find_isomorphism(&xor_table(), &constant_table()).expect("budget"), None);
    });
    p.case("invariants", |p| {
        let report = mine_invariants(&[xor_table(), constant_table()]).expect("mine");
        p.eq("worlds", report.world_count, 2);
        p.eq("shared", report.shared, vec!["commutative".to_string(), "associative".to_string()]);
        let by_law = |token: &str| report.laws.iter().find(|v| v.law == token).cloned().expect(token);
        p.eq("comm", by_law("commutative").holds, vec![true, true]);
        p.eq("assoc", by_law("associative").holds, vec![true, true]);
        p.eq("left-id", by_law("left-identity").holds, vec![true, false]);
        p.eq("right-id", by_law("right-identity").holds, vec![true, false]);
        p.eq("identity", by_law("identity").holds, vec![true, false]);
        p.demand("not-shared", !by_law("identity").shared, "existential identity is not shared");
    });
    p.case("dedupe", |p| {
        let receipt = dedupe(&[xor_table(), xnor_table(), constant_table()]).expect("dedupe");
        p.eq("groups", receipt.groups.len(), 2);
        p.eq("first", receipt.groups[0].representative.cells.clone(), vec![0, 0, 0, 0]);
        p.demand("first-kept", receipt.groups[0].dropped.is_empty(), "constant representative drops nothing");
        p.eq("second", receipt.groups[1].representative.cells.clone(), vec![0, 1, 1, 0]);
        p.eq("dropped", receipt.groups[1].dropped.len(), 1);
        p.eq("dropped-cells", receipt.groups[1].dropped[0].table.cells.clone(), vec![1, 0, 0, 1]);
        p.eq("witness", receipt.groups[1].dropped[0].witness.map.clone(), vec![1, 0]);
    });
    p.case("malformed-refused", |p| {
        let xor = xor_table();
        let short = WorldMorphism { source_size: 2, target_size: 2, map: vec![0] };
        p.eq("length", verify(&short, &xor, &xor), Err(MorphismError::InvalidMorphism { reason: "map-length" }));
        let oob = WorldMorphism { source_size: 2, target_size: 2, map: vec![0, 2] };
        p.eq("range", verify(&oob, &xor, &xor), Err(MorphismError::InvalidMorphism { reason: "image-out-of-range" }));
        let mismatched = WorldMorphism { source_size: 1, target_size: 1, map: vec![0] };
        p.eq("size", verify(&mismatched, &xor, &xor), Err(MorphismError::SizeMismatch));
        p.eq("ctor", WorldMorphism::new(2, 2, vec![0]), Err(MorphismError::InvalidMorphism { reason: "map-length" }));
        p.eq("version-ok", check_version(MORPHISM_VERSION), Ok(()));
        p.eq("version-unknown", check_version(MORPHISM_VERSION + 1), Err(MorphismError::UnknownVersion { version: MORPHISM_VERSION + 1 }));
        let oversized = usize::from(MAX_ISO_SEARCH_SIZE + 1);
        let cells = vec![0; oversized.saturating_mul(oversized)];
        p.eq("budget", find_isomorphism(&OpTable { carrier_size: MAX_ISO_SEARCH_SIZE + 1, cells: cells.clone() }, &OpTable { carrier_size: MAX_ISO_SEARCH_SIZE + 1, cells }), Err(MorphismError::BudgetExceeded { limit: u64::from(MAX_ISO_SEARCH_SIZE) }));
    });
    p.case("receipts-deterministic", |p| {
        let first_q = quotient(&constant_table()).expect("q1").to_json();
        p.eq("quotient", first_q.clone(), quotient(&constant_table()).expect("q2").to_json());
        p.contains("schema", &first_q, "\"schema\":\"emath.world-morphism\"");
        let tables = [xor_table(), and_table(), constant_table()];
        p.eq("invariants", mine_invariants(&tables).expect("i1").to_json(), mine_invariants(&tables).expect("i2").to_json());
        p.eq("dedupe", dedupe(&[xnor_table(), xor_table(), constant_table()]).expect("d1").to_json(), dedupe(&[xnor_table(), xor_table(), constant_table()]).expect("d2").to_json());
        let morphism = WorldMorphism { source_size: 2, target_size: 2, map: vec![0, 1] };
        p.eq("id", morphism_id(&morphism), morphism_id(&morphism));
    });
    p.finish();
}
