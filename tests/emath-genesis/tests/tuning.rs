//! tuning tests migrated from the in-crate `#[cfg(test)]`
//! module: every symbol they exercise is public crate surface.

use emath_genesis::tuning::{
    candidate_id, check_version, classify, semantic_dna, tune, tuning_id, CandidateStatus,
    HostExample, ImplVariant, ProtectedObjective, TuningBudget, TuningError,
    TuningRequest, TUNING_VERSION,
};
use emath_genesis::synth::OpTable;

fn xor_table() -> OpTable {
    OpTable {
        carrier_size: 2,
        cells: vec![0, 1, 1, 0],
    }
}

fn xor_objective() -> ProtectedObjective {
    ProtectedObjective {
        examples: vec![
            HostExample {
                inputs: vec![0, 0],
                expected: 0,
            },
            HostExample {
                inputs: vec![0, 1],
                expected: 1,
            },
            HostExample {
                inputs: vec![1, 0],
                expected: 1,
            },
            HostExample {
                inputs: vec![1, 1],
                expected: 0,
            },
        ],
    }
}

fn xor_request() -> TuningRequest {
    TuningRequest {
        version: TUNING_VERSION,
        carrier_size: 2,
        objective: xor_objective(),
        budget: TuningBudget::default(),
        joint_cursor: 0,
        incumbent: None,
    }
}

#[test]
fn happy_path_xor_fold_left_is_the_deterministic_winner() {
    assert_eq!(OpTable::from_index(2, 6).cells, xor_table().cells);
    let receipt = tune(&xor_request()).expect("winner");
    assert_eq!(receipt.dna, "2:0,1,1,0");
    assert_eq!(receipt.impl_token, "fold-left");
    assert_eq!(receipt.cost, 6);
    assert_eq!(receipt.version, TUNING_VERSION);
    assert_eq!(receipt.winner_id, candidate_id(&xor_table(), ImplVariant::FoldLeft));
    assert!(receipt.qualified >= 1);
    assert!(receipt.examined >= 19);
}

#[test]
fn protection_beats_cost_seeded_negative_control() {
    let receipt = tune(&xor_request()).expect("winner");
    let cheap = OpTable::from_index(2, 0);
    let cheap_id = candidate_id(&cheap, ImplVariant::FoldLeft);
    let entry = receipt
        .ledger
        .iter()
        .find(|row| row.candidate_id == cheap_id)
        .expect("cheapest constant table must be in the ledger");
    assert_eq!(entry.first_failed_example, 1);
    let cheap_cost = 4 + 1;
    assert!(
        receipt.cost > cheap_cost,
        "winner cost {} must beat cheap disqualified cost {cheap_cost}",
        receipt.cost
    );
    assert_eq!(receipt.dna, "2:0,1,1,0");
    assert_eq!(receipt.impl_token, "fold-left");
}

#[test]
fn impl_variants_are_real_on_a_non_associative_table() {
    let nand = OpTable {
        carrier_size: 2,
        cells: vec![1, 1, 1, 0],
    };
    let inputs = [0_u8, 0, 1];
    let left = ImplVariant::FoldLeft
        .evaluate(&nand, &inputs)
        .expect("non-empty");
    let right = ImplVariant::FoldRight
        .evaluate(&nand, &inputs)
        .expect("non-empty");
    assert_ne!(left, right);
    let objective = ProtectedObjective {
        examples: vec![HostExample {
            inputs: inputs.to_vec(),
            expected: left,
        }],
    };
    assert_eq!(
        classify(&nand, ImplVariant::FoldLeft, &objective),
        CandidateStatus::Qualified { cost: 4 }
    );
    assert_eq!(
        classify(&nand, ImplVariant::FoldRight, &objective),
        CandidateStatus::Disqualified {
            first_failed_example: 0
        }
    );
}

#[test]
fn semantic_dna_is_meaning_only() {
    let table = xor_table();
    let dna = semantic_dna(&table);
    assert_eq!(dna, semantic_dna(&OpTable::from_index(2, 6)));
    let left = candidate_id(&table, ImplVariant::FoldLeft);
    let right = candidate_id(&table, ImplVariant::FoldRight);
    let tree = candidate_id(&table, ImplVariant::PairwiseTree);
    assert_eq!(semantic_dna(&table), "2:0,1,1,0");
    assert_ne!(left, right);
    assert_ne!(left, tree);
    assert_ne!(right, tree);
}

#[test]
fn budget_then_resume_matches_unsplit_winner() {
    let unsplit = tune(&xor_request()).expect("unsplit");
    let first = TuningRequest {
        budget: TuningBudget { max_candidates: 8 },
        ..xor_request()
    };
    let incumbent = match tune(&first) {
        Err(TuningError::BudgetExceeded { limit: 8, incumbent }) => incumbent,
        other => panic!("window of 8 must refuse with incumbent, got {other:?}"),
    };
    // No qualified candidate exists in the first 8 joint indices of
    // the XOR objective, so the incumbent is empty here.
    assert_eq!(incumbent, None);
    let resumed = tune(&TuningRequest {
        budget: TuningBudget::default(),
        joint_cursor: 8,
        incumbent,
        ..xor_request()
    })
    .expect("resume");
    assert_eq!(resumed.dna, unsplit.dna);
    assert_eq!(resumed.impl_token, unsplit.impl_token);
    assert_eq!(resumed.cost, unsplit.cost);
    assert_eq!(resumed.winner_id, unsplit.winner_id);
    assert_eq!(resumed.tuning_id, unsplit.tuning_id);
}

#[test]
fn incumbent_preserves_a_cheap_winner_found_before_the_split() {
    // Objective satisfied by the constant-0 table (joint index 0,
    // complexity 1, cheapest possible). Splitting right after table 0
    // must not lose it to a costlier later candidate.
    let request = TuningRequest {
        version: TUNING_VERSION,
        carrier_size: 2,
        objective: ProtectedObjective {
            examples: vec![HostExample {
                inputs: vec![0, 0],
                expected: 0,
            }],
        },
        budget: TuningBudget::default(),
        joint_cursor: 0,
        incumbent: None,
    };
    let unsplit = tune(&request).expect("unsplit");
    assert_eq!(unsplit.dna, "2:0,0,0,0");
    assert_eq!(unsplit.cost, 2);

    let window = TuningRequest {
        budget: TuningBudget { max_candidates: 3 },
        ..request.clone()
    };
    let incumbent = match tune(&window) {
        Err(TuningError::BudgetExceeded { limit: 3, incumbent }) => incumbent,
        other => panic!("window of 3 must refuse with incumbent, got {other:?}"),
    };
    assert_eq!(incumbent, Some(0), "constant-0 fold-left is the incumbent");

    let resumed = tune(&TuningRequest {
        joint_cursor: 3,
        incumbent,
        ..request.clone()
    })
    .expect("resume with incumbent");
    assert_eq!(resumed.dna, unsplit.dna);
    assert_eq!(resumed.impl_token, unsplit.impl_token);
    assert_eq!(resumed.cost, unsplit.cost);
    assert_eq!(resumed.winner_id, unsplit.winner_id);

    // Dropping the incumbent silently loses the pre-split winner:
    // the naive resume picks a strictly costlier later candidate.
    let naive = tune(&TuningRequest {
        joint_cursor: 3,
        incumbent: None,
        ..request.clone()
    })
    .expect("naive resume");
    assert!(
        naive.cost > unsplit.cost,
        "naive resume must miss the cheap pre-split winner ({} vs {})",
        naive.cost,
        unsplit.cost
    );

    // Adversarial incumbents are re-verified, never trusted.
    assert_eq!(
        tune(&TuningRequest {
            joint_cursor: 3,
            incumbent: Some(5),
            ..request.clone()
        }),
        Err(TuningError::InvalidRequest {
            reason: "incumbent-out-of-window"
        })
    );
    let disqualified_incumbent = TuningRequest {
        objective: ProtectedObjective {
            examples: vec![HostExample {
                inputs: vec![0, 1],
                expected: 1,
            }],
        },
        joint_cursor: 3,
        incumbent: Some(0),
        ..request
    };
    assert_eq!(
        tune(&disqualified_incumbent),
        Err(TuningError::InvalidRequest {
            reason: "incumbent-not-qualified"
        })
    );
}

#[test]
fn malformed_requests_refuse() {
    let base = xor_request();
    assert_eq!(
        tune(&TuningRequest {
            carrier_size: 0,
            ..base.clone()
        }),
        Err(TuningError::InvalidRequest {
            reason: "empty-carrier"
        })
    );
    assert_eq!(
        tune(&TuningRequest {
            objective: ProtectedObjective {
                examples: vec![HostExample {
                    inputs: vec![0, 2],
                    expected: 0,
                }],
            },
            ..base.clone()
        }),
        Err(TuningError::InvalidRequest {
            reason: "example-out-of-range"
        })
    );
    assert_eq!(
        tune(&TuningRequest {
            objective: ProtectedObjective {
                examples: Vec::new(),
            },
            ..base.clone()
        }),
        Err(TuningError::InvalidRequest {
            reason: "no-protected-objective"
        })
    );
    assert_eq!(check_version(TUNING_VERSION), Ok(()));
    assert_eq!(
        check_version(TUNING_VERSION + 1),
        Err(TuningError::UnknownVersion {
            version: TUNING_VERSION + 1
        })
    );
    assert_eq!(
        tune(&TuningRequest {
            version: TUNING_VERSION + 1,
            ..base
        }),
        Err(TuningError::UnknownVersion {
            version: TUNING_VERSION + 1
        })
    );
}

#[test]
fn receipts_are_byte_identical_across_runs() {
    let request = xor_request();
    let first = tune(&request).expect("first").to_json();
    let second = tune(&request).expect("second").to_json();
    assert_eq!(first, second);
    assert!(first.starts_with('{'));
    assert!(first.contains("\"schema\":\"emath.joint-tuning\""));
    assert_eq!(tuning_id(&request), tuning_id(&request));
    let shifted_budget = TuningRequest {
        budget: TuningBudget { max_candidates: 64 },
        ..request.clone()
    };
    assert_eq!(tuning_id(&request), tuning_id(&shifted_budget));
    let shifted_cursor = TuningRequest {
        joint_cursor: 1,
        ..request.clone()
    };
    assert_eq!(tuning_id(&request), tuning_id(&shifted_cursor));
}
