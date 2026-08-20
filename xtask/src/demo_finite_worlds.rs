//! SG-14 production path: synthesize a size-2 commutative monoid, print
//! deterministic rows, record an impossible-law refusal, a budget refusal
//! plus resumed continuation, and detect a planted non-associative table.

use emath_genesis::{
    check_table, OpTable, SynthBudget, SynthError, SynthExample, SynthLaw, SynthRequest,
    SYNTH_VERSION,
};
use emath_world_ir::fnv1a64;

pub fn demo() -> u8 {
    println!("== demo finite-worlds ==");
    match run_demo() {
        Ok(()) => {
            println!("finite-worlds demo: ok");
            0
        }
        Err(error) => {
            eprintln!("finite-worlds demo FAILED: {error}");
            1
        }
    }
}

fn run_demo() -> Result<(), String> {
    emath_genesis::synth::check_version(SYNTH_VERSION)
        .map_err(|error| format!("synth version handshake refused: {error:?}"))?;

    let monoid_laws = vec![
        SynthLaw::Commutative,
        SynthLaw::Associative,
        SynthLaw::Identity { element: None },
    ];
    let monoid = SynthRequest {
        carrier_size: 2,
        laws: monoid_laws.clone(),
        examples: Vec::new(),
        budget: SynthBudget::default(),
        resume_cursor: 0,
    };
    let first = monoid
        .synthesize()
        .map_err(|error| format!("monoid refused: {error:?}"))?;
    let second = monoid
        .synthesize()
        .map_err(|error| format!("monoid second run refused: {error:?}"))?;
    let json = first.to_json();
    if json != second.to_json() {
        return Err("monoid receipts must be byte-identical across runs".to_string());
    }
    let cells = first
        .table
        .cells
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    println!("finite-worlds|monoid|{json}");

    let mut rows: Vec<String> = Vec::new();
    rows.push(format!(
        "monoid|table={cells}|examined={}|cursor={}|id={:016x}",
        first.tables_examined, first.resume_cursor, first.request_id
    ));

    let impossible = SynthRequest {
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
    match impossible.synthesize() {
        Err(SynthError::Unsatisfiable { tables_examined }) if tables_examined > 0 => {
            rows.push(format!(
                "impossible|unsatisfiable|examined={tables_examined}"
            ));
            println!("finite-worlds|impossible|unsatisfiable|examined={tables_examined}");
        }
        other => return Err(format!("contradictory examples must refuse, got {other:?}")),
    }

    let oversized = SynthRequest {
        carrier_size: 3,
        laws: monoid_laws,
        examples: Vec::new(),
        budget: SynthBudget { max_tables: 10 },
        resume_cursor: 0,
    };
    match oversized.synthesize() {
        Err(SynthError::BudgetExceeded { limit: 10 }) => {
            rows.push("budget|size-3|budget-exceeded|limit=10".to_string());
            println!("finite-worlds|budget|budget-exceeded|limit=10");
        }
        other => return Err(format!("size-3 window of 10 must refuse, got {other:?}")),
    }

    let split = SynthRequest {
        budget: SynthBudget { max_tables: 3 },
        ..monoid.clone()
    };
    match split.synthesize() {
        Err(SynthError::BudgetExceeded { limit: 3 }) => {}
        other => return Err(format!("split window must refuse, got {other:?}")),
    }
    let resumed = SynthRequest {
        budget: SynthBudget { max_tables: 16 },
        resume_cursor: 3,
        ..monoid
    }
    .synthesize()
    .map_err(|error| format!("resume refused: {error:?}"))?;
    if resumed.table != first.table {
        return Err("resumed search must match the unsplit winner".to_string());
    }
    rows.push(format!(
        "resume|cursor=3|table={cells}|examined={}",
        resumed.tables_examined
    ));
    println!(
        "finite-worlds|resume|cursor=3|examined={}|table={cells}",
        resumed.tables_examined
    );

    let planted = OpTable {
        carrier_size: 2,
        cells: vec![1, 1, 1, 0],
    };
    let violation = check_table(&planted, &[SynthLaw::Associative])
        .err()
        .ok_or_else(|| "planted NAND must violate associativity".to_string())?;
    if violation.counterexample != [0, 0, 1] {
        return Err(format!(
            "expected NAND counterexample [0,0,1], got {:?}",
            violation.counterexample
        ));
    }
    rows.push(format!(
        "negative|associative|counterexample={},{},{}|detected",
        violation.counterexample[0], violation.counterexample[1], violation.counterexample[2]
    ));
    println!(
        "finite-worlds|negative|associative|counterexample={},{},{}|detected",
        violation.counterexample[0], violation.counterexample[1], violation.counterexample[2]
    );

    let body = rows.join("\n");
    let receipt_id = fnv1a64(body.as_bytes());
    println!(
        "finite-worlds: rows={} receipt={receipt_id:016x}",
        rows.len()
    );
    Ok(())
}
