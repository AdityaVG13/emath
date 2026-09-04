//! SG-18 production path: joint-tune a protected XOR objective, print
//! the winner, record the protection-beats-cost negative control, a
//! budget refusal plus resumed continuation, and a deterministic receipt.

use emath_genesis::{
    HostExample, ImplVariant, OpTable, ProtectedObjective, TUNING_VERSION, TuningBudget,
    TuningError, TuningRequest, candidate_id, tune,
};
use emath_world_ir::fnv1a64;

pub fn demo() -> u8 {
    println!("== demo joint-tuning ==");
    match run_demo() {
        Ok(()) => {
            println!("joint-tuning demo: ok");
            0
        }
        Err(error) => {
            eprintln!("joint-tuning demo FAILED: {error}");
            1
        }
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

fn run_demo() -> Result<(), String> {
    emath_genesis::joint_tuning::check_version(TUNING_VERSION)
        .map_err(|error| format!("tuning version handshake refused: {error:?}"))?;

    let request = xor_request();
    let first = tune(&request).map_err(|error| format!("xor tune refused: {error:?}"))?;
    let second = tune(&request).map_err(|error| format!("xor second run refused: {error:?}"))?;
    let json = first.to_json();
    if json != second.to_json() {
        return Err("tuning receipts must be byte-identical across runs".to_string());
    }
    if first.dna != "2:0,1,1,0" || first.impl_token != "fold-left" || first.cost != 6 {
        return Err(format!(
            "expected xor/fold-left/cost=6, got dna={} impl={} cost={}",
            first.dna, first.impl_token, first.cost
        ));
    }
    println!(
        "joint-tuning|winner|dna={}|impl={}|cost={}|id={:016x}",
        first.dna, first.impl_token, first.cost, first.winner_id
    );

    let mut rows: Vec<String> = Vec::new();
    rows.push(format!(
        "winner|dna={}|impl={}|cost={}|examined={}|qualified={}|id={:016x}",
        first.dna, first.impl_token, first.cost, first.examined, first.qualified, first.winner_id
    ));

    let cheap = OpTable::from_index(2, 0);
    let cheap_id = candidate_id(&cheap, ImplVariant::FoldLeft);
    let entry = first
        .ledger
        .iter()
        .find(|row| row.candidate_id == cheap_id)
        .ok_or_else(|| "cheapest candidate missing from disqualification ledger".to_string())?;
    if entry.first_failed_example != 1 {
        return Err(format!(
            "expected cheap fail at example 1, got {}",
            entry.first_failed_example
        ));
    }
    let cheap_cost = 5_u64;
    if first.cost <= cheap_cost {
        return Err(format!(
            "protection must beat cost: winner {} <= cheap {cheap_cost}",
            first.cost
        ));
    }
    rows.push(format!(
        "negative|cheapest-disqualified|id={cheap_id:016x}|example={}|winner-cost={}",
        entry.first_failed_example, first.cost
    ));
    println!(
        "joint-tuning|negative|cheapest-disqualified|id={cheap_id:016x}|example={}|winner-cost={}",
        entry.first_failed_example, first.cost
    );

    let split = TuningRequest {
        budget: TuningBudget { max_candidates: 8 },
        ..request.clone()
    };
    let carried = match tune(&split) {
        Err(TuningError::BudgetExceeded {
            limit: 8,
            incumbent,
        }) => {
            rows.push(format!(
                "budget|budget-exceeded|limit=8|incumbent={incumbent:?}"
            ));
            println!("joint-tuning|budget|budget-exceeded|limit=8|incumbent={incumbent:?}");
            incumbent
        }
        other => return Err(format!("window of 8 must refuse, got {other:?}")),
    };

    let resumed = tune(&TuningRequest {
        budget: TuningBudget::default(),
        joint_cursor: 8,
        incumbent: carried,
        ..request
    })
    .map_err(|error| format!("resume refused: {error:?}"))?;
    if resumed.dna != first.dna
        || resumed.impl_token != first.impl_token
        || resumed.cost != first.cost
        || resumed.winner_id != first.winner_id
    {
        return Err("resumed search must match the unsplit winner".to_string());
    }
    rows.push(format!(
        "resume|cursor=8|dna={}|impl={}|cost={}|examined={}",
        resumed.dna, resumed.impl_token, resumed.cost, resumed.examined
    ));
    println!(
        "joint-tuning|resume|cursor=8|examined={}|dna={}|cost={}",
        resumed.examined, resumed.dna, resumed.cost
    );

    let body = rows.join("\n");
    let receipt_id = fnv1a64(body.as_bytes());
    println!(
        "joint-tuning: rows={} receipt={receipt_id:016x}",
        rows.len()
    );
    Ok(())
}
