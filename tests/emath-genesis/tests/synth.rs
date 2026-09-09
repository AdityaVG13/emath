//! Finite-world synthesis tests.

use emath_genesis::synth::{
    MAX_CARRIER_SIZE, OpTable, SYNTH_VERSION, SynthBudget, SynthError, SynthExample, SynthLaw,
    SynthRequest, check_table, check_version, synth_id,
};
use emath_test_harness::Probe;

fn request(n: u8, laws: Vec<SynthLaw>) -> SynthRequest {
    SynthRequest { carrier_size: n, laws, examples: Vec::new(), budget: SynthBudget::default(), resume_cursor: 0 }
}

fn comm_id() -> Vec<SynthLaw> {
    vec![SynthLaw::Commutative, SynthLaw::Identity { element: None }]
}

#[test]
fn finite_synth() {
    let mut p = Probe::new("synthesis finds the deterministic winner and refuses the rest");
    p.case("happy-path", |p| {
        let receipt = request(2, comm_id()).synthesize().expect("winner");
        p.eq("cells", receipt.table.cells, vec![0, 1, 1, 0]);
        p.eq("size", receipt.carrier_size, 2);
        p.eq("examined", receipt.tables_examined, 7);
        p.eq("cursor", receipt.resume_cursor, 7);
        p.eq("version", receipt.version, SYNTH_VERSION);
    });
    p.case("impossible-refused", |p| {
        let request = SynthRequest { carrier_size: 2, laws: vec![SynthLaw::Commutative], examples: vec![SynthExample { left: 0, right: 1, result: 0 }, SynthExample { left: 1, right: 0, result: 1 }], budget: SynthBudget { max_tables: 16 }, resume_cursor: 0 };
        p.eq("unsat", request.synthesize(), Err(SynthError::Unsatisfiable { tables_examined: 16 }));
    });
    p.case("resume-matches", |p| {
        let oversized = SynthRequest { budget: SynthBudget { max_tables: 10 }, ..request(3, comm_id()) };
        p.eq("budget", oversized.synthesize(), Err(SynthError::BudgetExceeded { limit: 10 }));
        let laws = comm_id();
        let unsplit = request(2, laws.clone()).synthesize().expect("unsplit");
        let first_window = SynthRequest { budget: SynthBudget { max_tables: 3 }, ..request(2, laws.clone()) };
        p.eq("window", first_window.synthesize(), Err(SynthError::BudgetExceeded { limit: 3 }));
        let continued = SynthRequest { budget: SynthBudget { max_tables: 16 }, resume_cursor: 3, ..request(2, laws) }.synthesize().expect("resume");
        p.eq("table", continued.table.clone(), unsplit.table);
        p.eq("request-id", continued.request_id, unsplit.request_id);
        p.eq("cells", continued.table.cells, vec![0, 1, 1, 0]);
    });
    p.case("malformed-refused", |p| {
        p.eq("empty", request(0, vec![SynthLaw::Commutative]).synthesize(), Err(SynthError::InvalidRequest { reason: "empty-carrier" }));
        p.eq("large", request(MAX_CARRIER_SIZE + 1, vec![SynthLaw::Commutative]).synthesize(), Err(SynthError::InvalidRequest { reason: "carrier-too-large" }));
        p.eq("example-range", SynthRequest { examples: vec![SynthExample { left: 0, right: 2, result: 0 }], ..request(2, vec![SynthLaw::Commutative]) }.synthesize(), Err(SynthError::InvalidRequest { reason: "example-out-of-range" }));
        p.eq("identity-range", request(2, vec![SynthLaw::Identity { element: Some(5) }]).synthesize(), Err(SynthError::InvalidRequest { reason: "identity-out-of-range" }));
        p.eq("version-ok", check_version(SYNTH_VERSION), Ok(()));
        p.eq("version-unknown", check_version(SYNTH_VERSION + 1), Err(SynthError::UnknownVersion { version: SYNTH_VERSION + 1 }));
    });
    p.case("receipts-deterministic", |p| {
        let request = request(2, comm_id());
        let first = request.synthesize().expect("first").to_json();
        p.eq("stable", first.clone(), request.synthesize().expect("second").to_json());
        p.demand("brace", first.starts_with('{'), "receipt is JSON");
        p.contains("schema", &first, "\"schema\":\"emath.finite-world\"");
        p.eq("id", synth_id(&request), synth_id(&request));
        let shifted_budget = SynthRequest { budget: SynthBudget { max_tables: 64 }, ..request.clone() };
        p.eq("budget-blind", synth_id(&request), synth_id(&shifted_budget));
        let shifted_cursor = SynthRequest { resume_cursor: 1, ..request.clone() };
        p.eq("cursor-blind", synth_id(&request), synth_id(&shifted_cursor));
    });
    p.case("winners-commute", |p| {
        let mut cursor = 0_u64;
        let mut found = 0_u32;
        loop {
            match (SynthRequest { resume_cursor: cursor, budget: SynthBudget { max_tables: 16 }, ..request(2, vec![SynthLaw::Commutative]) }).synthesize() {
                Ok(receipt) => {
                    for a in 0..2_u8 {
                        for b in 0..2_u8 {
                            p.eq(format!("commute-{cursor}-{a}-{b}"), receipt.table.apply(a, b), receipt.table.apply(b, a));
                        }
                    }
                    found += 1;
                    cursor = receipt.resume_cursor;
                }
                Err(SynthError::Unsatisfiable { .. }) => break,
                other => {
                    p.fail("exhaust", format!("unexpected outcome {other:?}"));
                    break;
                }
            }
        }
        p.eq("count", found, 8);
    });
    p.case("nand-triple", |p| {
        let planted = OpTable { carrier_size: 2, cells: vec![1, 1, 1, 0] };
        let violation = check_table(&planted, &[SynthLaw::Associative]).expect_err("NAND must violate associativity");
        p.eq("law", violation.law, "associative".to_string());
        p.eq("triple", violation.counterexample, [0, 0, 1]);
        let left = planted.apply(planted.apply(0, 0), 1);
        let right = planted.apply(0, planted.apply(0, 1));
        p.ne("associates", left, right);
        p.eq("commutes", check_table(&planted, &[SynthLaw::Commutative]), Ok(()));
    });
    p.finish();
}
