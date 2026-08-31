//! synth tests migrated from the in-crate `#[cfg(test)]`
//! module: every symbol they exercise is public crate surface.

use emath_genesis::synth::{
    check_table, check_version, synth_id, OpTable, SynthBudget, SynthError, SynthExample,
    SynthLaw, SynthRequest, MAX_CARRIER_SIZE, SYNTH_VERSION,
};

fn request(n: u8, laws: Vec<SynthLaw>) -> SynthRequest {
    SynthRequest {
        carrier_size: n,
        laws,
        examples: Vec::new(),
        budget: SynthBudget::default(),
        resume_cursor: 0,
    }
}

fn comm_id() -> Vec<SynthLaw> {
    vec![SynthLaw::Commutative, SynthLaw::Identity { element: None }]
}

#[test]
fn happy_path_size_two_commutative_identity() {
    let receipt = request(2, comm_id()).synthesize().expect("winner");
    assert_eq!(receipt.table.cells, vec![0, 1, 1, 0]);
    assert_eq!(receipt.carrier_size, 2);
    assert_eq!(receipt.tables_examined, 7);
    assert_eq!(receipt.resume_cursor, 7);
    assert_eq!(receipt.version, SYNTH_VERSION);
}

#[test]
fn impossible_law_refuses_unsatisfiable() {
    let request = SynthRequest {
        carrier_size: 2,
        laws: vec![SynthLaw::Commutative],
        examples: vec![
            SynthExample {
                left: 0,
                right: 1,
                result: 0,
            },
            SynthExample {
                left: 1,
                right: 0,
                result: 1,
            },
        ],
        budget: SynthBudget { max_tables: 16 },
        resume_cursor: 0,
    };
    assert_eq!(
        request.synthesize(),
        Err(SynthError::Unsatisfiable {
            tables_examined: 16
        })
    );
}

#[test]
fn budget_exceeded_then_split_equals_unsplit() {
    let oversized = SynthRequest {
        budget: SynthBudget { max_tables: 10 },
        ..request(3, comm_id())
    };
    assert_eq!(
        oversized.synthesize(),
        Err(SynthError::BudgetExceeded { limit: 10 })
    );

    let laws = comm_id();
    let unsplit = request(2, laws.clone()).synthesize().expect("unsplit");

    let first_window = SynthRequest {
        budget: SynthBudget { max_tables: 3 },
        ..request(2, laws.clone())
    };
    assert_eq!(
        first_window.synthesize(),
        Err(SynthError::BudgetExceeded { limit: 3 })
    );
    let continued = SynthRequest {
        budget: SynthBudget { max_tables: 16 },
        resume_cursor: 3,
        ..request(2, laws)
    }
    .synthesize()
    .expect("resume");
    assert_eq!(continued.table, unsplit.table);
    assert_eq!(continued.request_id, unsplit.request_id);
    assert_eq!(continued.table.cells, vec![0, 1, 1, 0]);
}

#[test]
fn malformed_and_adversarial_requests_refuse() {
    assert_eq!(
        request(0, vec![SynthLaw::Commutative]).synthesize(),
        Err(SynthError::InvalidRequest {
            reason: "empty-carrier"
        })
    );
    assert_eq!(
        request(MAX_CARRIER_SIZE + 1, vec![SynthLaw::Commutative]).synthesize(),
        Err(SynthError::InvalidRequest {
            reason: "carrier-too-large"
        })
    );
    assert_eq!(
        SynthRequest {
            examples: vec![SynthExample {
                left: 0,
                right: 2,
                result: 0,
            }],
            ..request(2, vec![SynthLaw::Commutative])
        }
        .synthesize(),
        Err(SynthError::InvalidRequest {
            reason: "example-out-of-range"
        })
    );
    assert_eq!(
        request(2, vec![SynthLaw::Identity { element: Some(5) }]).synthesize(),
        Err(SynthError::InvalidRequest {
            reason: "identity-out-of-range"
        })
    );
    assert_eq!(check_version(SYNTH_VERSION), Ok(()));
    assert_eq!(
        check_version(SYNTH_VERSION + 1),
        Err(SynthError::UnknownVersion {
            version: SYNTH_VERSION + 1
        })
    );
}

#[test]
fn receipts_are_byte_identical_across_runs() {
    let request = request(2, comm_id());
    let first = request.synthesize().expect("first").to_json();
    let second = request.synthesize().expect("second").to_json();
    assert_eq!(first, second);
    assert!(first.starts_with('{'));
    assert!(first.contains("\"schema\":\"emath.finite-world\""));
    assert_eq!(synth_id(&request), synth_id(&request));
    let shifted_budget = SynthRequest {
        budget: SynthBudget { max_tables: 64 },
        ..request.clone()
    };
    assert_eq!(synth_id(&request), synth_id(&shifted_budget));
    let shifted_cursor = SynthRequest {
        resume_cursor: 1,
        ..request.clone()
    };
    assert_eq!(synth_id(&request), synth_id(&shifted_cursor));
}

#[test]
fn every_commutative_winner_actually_commutes() {
    let mut cursor = 0_u64;
    let mut found = 0_u32;
    loop {
        let outcome = SynthRequest {
            resume_cursor: cursor,
            budget: SynthBudget { max_tables: 16 },
            ..request(2, vec![SynthLaw::Commutative])
        }
        .synthesize();
        match outcome {
            Ok(receipt) => {
                for a in 0..2_u8 {
                    for b in 0..2_u8 {
                        assert_eq!(
                            receipt.table.apply(a, b),
                            receipt.table.apply(b, a),
                            "winner at cursor {cursor} must commute"
                        );
                    }
                }
                found += 1;
                cursor = receipt.resume_cursor;
            }
            Err(SynthError::Unsatisfiable { .. }) => break,
            other => panic!("unexpected outcome {other:?}"),
        }
    }
    assert_eq!(found, 8, "size-2 has eight commutative tables");
}

#[test]
fn seeded_non_associative_table_is_rejected_with_triple() {
    // NAND on {0,1}: op = 1 except op(1,1)=0. Not associative.
    let planted = OpTable {
        carrier_size: 2,
        cells: vec![1, 1, 1, 0],
    };
    let violation = check_table(&planted, &[SynthLaw::Associative])
        .expect_err("NAND must violate associativity");
    assert_eq!(violation.law, "associative");
    assert_eq!(violation.counterexample, [0, 0, 1]);
    let left = planted.apply(planted.apply(0, 0), 1);
    let right = planted.apply(0, planted.apply(0, 1));
    assert_ne!(left, right);
    assert_eq!(check_table(&planted, &[SynthLaw::Commutative]), Ok(()));
}
