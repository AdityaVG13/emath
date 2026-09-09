//! Joint-tuning winner, protection, and resume tests.

use emath_genesis::joint_tuning::{
    CandidateStatus, HostExample, ImplVariant, ProtectedObjective, TUNING_VERSION, TuningBudget,
    TuningError, TuningRequest, candidate_id, check_version, classify, semantic_dna, tune,
    tuning_id,
};
use emath_genesis::synth::OpTable;
use emath_test_harness::Probe;

fn xor_table() -> OpTable {
    OpTable { carrier_size: 2, cells: vec![0, 1, 1, 0] }
}

fn xor_objective() -> ProtectedObjective {
    ProtectedObjective { examples: vec![HostExample { inputs: vec![0, 0], expected: 0 }, HostExample { inputs: vec![0, 1], expected: 1 }, HostExample { inputs: vec![1, 0], expected: 1 }, HostExample { inputs: vec![1, 1], expected: 0 }] }
}

fn xor_request() -> TuningRequest {
    TuningRequest { version: TUNING_VERSION, carrier_size: 2, objective: xor_objective(), budget: TuningBudget::default(), joint_cursor: 0, incumbent: None }
}

#[test]
fn joint_tuning() {
    let mut p = Probe::new("protection beats cost and resume preserves the winner");
    p.case("xor-winner", |p| {
        p.eq("index", OpTable::from_index(2, 6).cells, xor_table().cells);
        let receipt = tune(&xor_request()).expect("winner");
        p.eq("dna", receipt.dna, "2:0,1,1,0".to_string());
        p.eq("impl", receipt.impl_token, "fold-left".to_string());
        p.eq("cost", receipt.cost, 6);
        p.eq("version", receipt.version, TUNING_VERSION);
        p.eq("winner", receipt.winner_id, candidate_id(&xor_table(), ImplVariant::FoldLeft));
        p.demand("qualified", receipt.qualified >= 1, "at least one qualifier");
        p.demand("examined", receipt.examined >= 19, "search examines the space");
    });
    p.case("protection-beats-cost", |p| {
        let receipt = tune(&xor_request()).expect("winner");
        let cheap = OpTable::from_index(2, 0);
        let entry = receipt.ledger.iter().find(|row| row.candidate_id == candidate_id(&cheap, ImplVariant::FoldLeft)).expect("cheapest table in ledger");
        p.eq("failed-at", entry.first_failed_example, 1);
        p.demand("costlier", receipt.cost > 4 + 1, format!("winner cost {} beats cheap disqualified cost", receipt.cost));
        p.eq("dna", receipt.dna, "2:0,1,1,0".to_string());
        p.eq("impl", receipt.impl_token, "fold-left".to_string());
    });
    p.case("impl-variants-real", |p| {
        let nand = OpTable { carrier_size: 2, cells: vec![1, 1, 1, 0] };
        let inputs = [0_u8, 0, 1];
        let left = ImplVariant::FoldLeft.evaluate(&nand, &inputs).expect("non-empty");
        let right = ImplVariant::FoldRight.evaluate(&nand, &inputs).expect("non-empty");
        p.ne("differ", left, right);
        let objective = ProtectedObjective { examples: vec![HostExample { inputs: inputs.to_vec(), expected: left }] };
        p.eq("left-qualifies", classify(&nand, ImplVariant::FoldLeft, &objective), CandidateStatus::Qualified { cost: 4 });
        p.eq("right-refused", classify(&nand, ImplVariant::FoldRight, &objective), CandidateStatus::Disqualified { first_failed_example: 0 });
    });
    p.case("dna-meaning-only", |p| {
        let table = xor_table();
        p.eq("dna", semantic_dna(&table), semantic_dna(&OpTable::from_index(2, 6)));
        p.eq("dna", semantic_dna(&table), "2:0,1,1,0".to_string());
        let left = candidate_id(&table, ImplVariant::FoldLeft);
        let right = candidate_id(&table, ImplVariant::FoldRight);
        let tree = candidate_id(&table, ImplVariant::PairwiseTree);
        p.ne("left-right", left, right);
        p.ne("left-tree", left, tree);
        p.ne("right-tree", right, tree);
    });
    p.case("resume-matches", |p| {
        let unsplit = tune(&xor_request()).expect("unsplit");
        let incumbent = match tune(&TuningRequest { budget: TuningBudget { max_candidates: 8 }, ..xor_request() }) {
            Err(TuningError::BudgetExceeded { limit: 8, incumbent }) => incumbent,
            other => {
                p.fail("window", format!("window of 8 must refuse with incumbent, got {other:?}"));
                return;
            }
        };
        p.eq("no-early-winner", incumbent, None);
        let resumed = tune(&TuningRequest { budget: TuningBudget::default(), joint_cursor: 8, incumbent, ..xor_request() }).expect("resume");
        p.eq("dna", resumed.dna, unsplit.dna);
        p.eq("impl", resumed.impl_token, unsplit.impl_token);
        p.eq("cost", resumed.cost, unsplit.cost);
        p.eq("winner", resumed.winner_id, unsplit.winner_id);
        p.eq("id", resumed.tuning_id, unsplit.tuning_id);
    });
    p.case("incumbent-preserved", |p| {
        let request = TuningRequest { version: TUNING_VERSION, carrier_size: 2, objective: ProtectedObjective { examples: vec![HostExample { inputs: vec![0, 0], expected: 0 }] }, budget: TuningBudget::default(), joint_cursor: 0, incumbent: None };
        let unsplit = tune(&request).expect("unsplit");
        p.eq("dna", unsplit.dna.clone(), "2:0,0,0,0".to_string());
        p.eq("cost", unsplit.cost, 2);
        let incumbent = match tune(&TuningRequest { budget: TuningBudget { max_candidates: 3 }, ..request.clone() }) {
            Err(TuningError::BudgetExceeded { limit: 3, incumbent }) => incumbent,
            other => {
                p.fail("window", format!("window of 3 must refuse with incumbent, got {other:?}"));
                return;
            }
        };
        p.eq("incumbent", incumbent, Some(0));
        let resumed = tune(&TuningRequest { joint_cursor: 3, incumbent, ..request.clone() }).expect("resume with incumbent");
        p.eq("dna", resumed.dna, unsplit.dna);
        p.eq("impl", resumed.impl_token, unsplit.impl_token);
        p.eq("cost", resumed.cost, unsplit.cost);
        p.eq("winner", resumed.winner_id, unsplit.winner_id);
        let naive = tune(&TuningRequest { joint_cursor: 3, incumbent: None, ..request.clone() }).expect("naive resume");
        p.demand("naive-costlier", naive.cost > unsplit.cost, "naive resume misses the cheap winner");
        p.eq("adversarial", tune(&TuningRequest { joint_cursor: 3, incumbent: Some(5), ..request.clone() }), Err(TuningError::InvalidRequest { reason: "incumbent-out-of-window" }));
        let disqualified = TuningRequest { objective: ProtectedObjective { examples: vec![HostExample { inputs: vec![0, 1], expected: 1 }] }, joint_cursor: 3, incumbent: Some(0), ..request };
        p.eq("reverified", tune(&disqualified), Err(TuningError::InvalidRequest { reason: "incumbent-not-qualified" }));
    });
    p.case("malformed-refused", |p| {
        let base = xor_request();
        p.eq("empty", tune(&TuningRequest { carrier_size: 0, ..base.clone() }), Err(TuningError::InvalidRequest { reason: "empty-carrier" }));
        p.eq("range", tune(&TuningRequest { objective: ProtectedObjective { examples: vec![HostExample { inputs: vec![0, 2], expected: 0 }] }, ..base.clone() }), Err(TuningError::InvalidRequest { reason: "example-out-of-range" }));
        p.eq("no-objective", tune(&TuningRequest { objective: ProtectedObjective { examples: Vec::new() }, ..base.clone() }), Err(TuningError::InvalidRequest { reason: "no-protected-objective" }));
        p.eq("version-ok", check_version(TUNING_VERSION), Ok(()));
        p.eq("version-unknown", check_version(TUNING_VERSION + 1), Err(TuningError::UnknownVersion { version: TUNING_VERSION + 1 }));
        p.eq("request-version", tune(&TuningRequest { version: TUNING_VERSION + 1, ..base }), Err(TuningError::UnknownVersion { version: TUNING_VERSION + 1 }));
    });
    p.case("receipts-deterministic", |p| {
        let request = xor_request();
        let first = tune(&request).expect("first").to_json();
        p.eq("stable", first.clone(), tune(&request).expect("second").to_json());
        p.demand("brace", first.starts_with('{'), "receipt is JSON");
        p.contains("schema", &first, "\"schema\":\"emath.joint-tuning\"");
        p.eq("id", tuning_id(&request), tuning_id(&request));
        let shifted_budget = TuningRequest { budget: TuningBudget { max_candidates: 64 }, ..request.clone() };
        p.eq("budget-blind", tuning_id(&request), tuning_id(&shifted_budget));
        let shifted_cursor = TuningRequest { joint_cursor: 1, ..request.clone() };
        p.eq("cursor-blind", tuning_id(&request), tuning_id(&shifted_cursor));
    });
    p.finish();
}
