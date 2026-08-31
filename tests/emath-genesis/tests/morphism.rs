//! morphism tests migrated from the in-crate `#[cfg(test)]`
//! module: every symbol they exercise is public crate surface.

use emath_genesis::morphism::{
    check_version, dedupe, find_isomorphism, mine_invariants, morphism_id, quotient, verify,
    MorphismError, WorldMorphism, MAX_ISO_SEARCH_SIZE, MORPHISM_VERSION,
};
use emath_genesis::synth::OpTable;

fn xor_table() -> OpTable {
    OpTable {
        carrier_size: 2,
        cells: vec![0, 1, 1, 0],
    }
}

fn xnor_table() -> OpTable {
    OpTable {
        carrier_size: 2,
        cells: vec![1, 0, 0, 1],
    }
}

fn constant_table() -> OpTable {
    OpTable {
        carrier_size: 2,
        cells: vec![0, 0, 0, 0],
    }
}

fn and_table() -> OpTable {
    OpTable {
        carrier_size: 2,
        cells: vec![0, 0, 0, 1],
    }
}

#[test]
fn happy_path_xor_and_constant_quotients() {
    let xor = quotient(&xor_table()).expect("xor quotient");
    assert_eq!(xor.classes, vec![vec![0], vec![1]]);
    assert_eq!(xor.table.carrier_size, 2);
    assert_eq!(xor.table.cells, vec![0, 1, 1, 0]);
    assert_eq!(xor.projection.map, vec![0, 1]);

    let constant = quotient(&constant_table()).expect("constant quotient");
    assert_eq!(constant.classes, vec![vec![0, 1]]);
    assert_eq!(constant.table.carrier_size, 1);
    assert_eq!(constant.table.cells, vec![0]);
    assert_eq!(constant.projection.map, vec![0, 0]);
}

#[test]
fn homomorphism_identity_ok_wrong_map_refused() {
    let xor = xor_table();
    let identity = WorldMorphism {
        source_size: 2,
        target_size: 2,
        map: vec![0, 1],
    };
    assert_eq!(verify(&identity, &xor, &xor), Ok(()));

    let wrong = WorldMorphism {
        source_size: 2,
        target_size: 2,
        map: vec![1, 0],
    };
    assert_eq!(
        verify(&wrong, &xor, &xor),
        Err(MorphismError::NotAHomomorphism { pair: [0, 0] })
    );
}

#[test]
fn isomorphism_relabeling_detected_xor_vs_constant_none() {
    let witness = find_isomorphism(&xor_table(), &xnor_table())
        .expect("budget")
        .expect("iso");
    assert_eq!(witness.map, vec![1, 0]);
    assert_eq!(verify(&witness, &xor_table(), &xnor_table()), Ok(()));
    assert_eq!(
        find_isomorphism(&xor_table(), &constant_table()).expect("budget"),
        None
    );
}

#[test]
fn invariant_mining_shared_commutative_not_identity() {
    // XOR has a two-sided identity (0). The constant-0 table is
    // commutative (AND-like collapse) but has no identity, so
    // existential identity is not shared. AND on {0,1} would share
    // identity existentially (element 1).
    let report = mine_invariants(&[xor_table(), constant_table()]).expect("mine");
    assert_eq!(report.world_count, 2);
    assert_eq!(
        report.shared,
        vec!["commutative".to_string(), "associative".to_string()]
    );
    let by_law = |token: &str| {
        report
            .laws
            .iter()
            .find(|verdict| verdict.law == token)
            .cloned()
            .expect(token)
    };
    assert_eq!(by_law("commutative").holds, vec![true, true]);
    assert_eq!(by_law("associative").holds, vec![true, true]);
    assert_eq!(by_law("left-identity").holds, vec![true, false]);
    assert_eq!(by_law("right-identity").holds, vec![true, false]);
    assert_eq!(by_law("identity").holds, vec![true, false]);
    assert!(!by_law("identity").shared);
}

#[test]
fn dedupe_groups_isomorphic_pair() {
    let receipt = dedupe(&[xor_table(), xnor_table(), constant_table()]).expect("dedupe");
    assert_eq!(receipt.groups.len(), 2);
    assert_eq!(receipt.groups[0].representative.cells, vec![0, 0, 0, 0]);
    assert!(receipt.groups[0].dropped.is_empty());
    assert_eq!(receipt.groups[1].representative.cells, vec![0, 1, 1, 0]);
    assert_eq!(receipt.groups[1].dropped.len(), 1);
    assert_eq!(receipt.groups[1].dropped[0].table.cells, vec![1, 0, 0, 1]);
    assert_eq!(receipt.groups[1].dropped[0].witness.map, vec![1, 0]);
}

#[test]
fn malformed_maps_and_versions_refuse() {
    let xor = xor_table();
    let short = WorldMorphism {
        source_size: 2,
        target_size: 2,
        map: vec![0],
    };
    assert_eq!(
        verify(&short, &xor, &xor),
        Err(MorphismError::InvalidMorphism {
            reason: "map-length"
        })
    );
    let oob = WorldMorphism {
        source_size: 2,
        target_size: 2,
        map: vec![0, 2],
    };
    assert_eq!(
        verify(&oob, &xor, &xor),
        Err(MorphismError::InvalidMorphism {
            reason: "image-out-of-range"
        })
    );
    let mismatched = WorldMorphism {
        source_size: 1,
        target_size: 1,
        map: vec![0],
    };
    assert_eq!(
        verify(&mismatched, &xor, &xor),
        Err(MorphismError::SizeMismatch)
    );
    assert_eq!(
        WorldMorphism::new(2, 2, vec![0]),
        Err(MorphismError::InvalidMorphism {
            reason: "map-length"
        })
    );
    assert_eq!(check_version(MORPHISM_VERSION), Ok(()));
    assert_eq!(
        check_version(MORPHISM_VERSION + 1),
        Err(MorphismError::UnknownVersion {
            version: MORPHISM_VERSION + 1
        })
    );
    let oversized = usize::from(MAX_ISO_SEARCH_SIZE + 1);
    let cells = vec![0; oversized.saturating_mul(oversized)];
    assert_eq!(
        find_isomorphism(
            &OpTable {
                carrier_size: MAX_ISO_SEARCH_SIZE + 1,
                cells: cells.clone(),
            },
            &OpTable {
                carrier_size: MAX_ISO_SEARCH_SIZE + 1,
                cells,
            }
        ),
        Err(MorphismError::BudgetExceeded {
            limit: u64::from(MAX_ISO_SEARCH_SIZE)
        })
    );
}

#[test]
fn receipts_are_byte_identical_across_runs() {
    let first_q = quotient(&constant_table()).expect("q1").to_json();
    let second_q = quotient(&constant_table()).expect("q2").to_json();
    assert_eq!(first_q, second_q);
    assert!(first_q.contains("\"schema\":\"emath.world-morphism\""));

    let first_i = mine_invariants(&[xor_table(), and_table(), constant_table()])
        .expect("i1")
        .to_json();
    let second_i = mine_invariants(&[xor_table(), and_table(), constant_table()])
        .expect("i2")
        .to_json();
    assert_eq!(first_i, second_i);

    let first_d = dedupe(&[xnor_table(), xor_table(), constant_table()])
        .expect("d1")
        .to_json();
    let second_d = dedupe(&[xnor_table(), xor_table(), constant_table()])
        .expect("d2")
        .to_json();
    assert_eq!(first_d, second_d);

    let morphism = WorldMorphism {
        source_size: 2,
        target_size: 2,
        map: vec![0, 1],
    };
    assert_eq!(morphism_id(&morphism), morphism_id(&morphism));
}
