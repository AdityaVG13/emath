//! SG-13 production path: evaluate every finite-analogue kind, print
//! deterministic receipts, assert byte-identity across two runs, record
//! a budget refusal, and detect a seeded wrong quadrature by recomputation.

use emath_genesis::{
    AnalogueDomain, AnalogueError, AnalogueRequest, AnalogueVerdict, BinderBudget, BinderKind,
    BinderTerm, ANALOGUE_VERSION,
};
use emath_term::{SymbolId, Term, VariableId};
use emath_world_ir::fnv1a64;

pub fn demo() -> u8 {
    println!("== demo finite-analogues ==");
    match run_demo() {
        Ok(()) => {
            println!("finite-analogues demo: ok");
            0
        }
        Err(error) => {
            eprintln!("finite-analogues demo FAILED: {error}");
            1
        }
    }
}

fn run_demo() -> Result<(), String> {
    emath_genesis::analogue::check_version(ANALOGUE_VERSION)
        .map_err(|error| format!("analogue version handshake refused: {error:?}"))?;

    let identity = |kind: BinderKind, domain: AnalogueDomain| AnalogueRequest {
        kind,
        domain,
        budget: BinderBudget::default(),
        bound: VariableId("x".to_string()),
        body: BinderTerm::Leaf(Term::Variable(VariableId("x".to_string()))),
    };

    let cases = [
        (
            "sum",
            identity(
                BinderKind::Sum,
                AnalogueDomain::IntegerRange { lower: 1, upper: 5 },
            ),
        ),
        (
            "product",
            identity(
                BinderKind::Product,
                AnalogueDomain::IntegerRange { lower: 1, upper: 4 },
            ),
        ),
        (
            "integral",
            identity(
                BinderKind::Integral,
                AnalogueDomain::Interval {
                    lower: 0.0,
                    upper: 1.0,
                    n: 4,
                },
            ),
        ),
        (
            "derivative",
            AnalogueRequest {
                kind: BinderKind::Derivative,
                domain: AnalogueDomain::Difference {
                    point: 3.0,
                    h: 0.25,
                },
                budget: BinderBudget::default(),
                bound: VariableId("x".to_string()),
                body: BinderTerm::Leaf(Term::Apply {
                    operator: SymbolId("*".to_string()),
                    arguments: vec![
                        Term::Variable(VariableId("x".to_string())),
                        Term::Variable(VariableId("x".to_string())),
                    ],
                }),
            },
        ),
        (
            "limit",
            identity(
                BinderKind::Limit,
                AnalogueDomain::Approach {
                    point: 0.0,
                    samples: 4,
                },
            ),
        ),
    ];

    let mut rows: Vec<String> = Vec::new();
    for (label, request) in &cases {
        let first = request
            .evaluate()
            .map_err(|error| format!("{label}: analogue refused: {error:?}"))?;
        let second = request
            .evaluate()
            .map_err(|error| format!("{label}: second run refused: {error:?}"))?;
        let json = first.to_json();
        if json != second.to_json() {
            return Err(format!(
                "{label}: receipts must be byte-identical across runs"
            ));
        }
        println!("finite-analogues|{label}|{json}");
        rows.push(format!(
            "{label}|id={:016x}|verdict={}|spent={}|value={}",
            first.request_id,
            first.verdict.canonical(),
            first.budget_spent,
            first
                .value_bits
                .map(|bits| format!("{bits:016x}"))
                .unwrap_or_else(|| "null".to_string())
        ));
        if *label == "limit" && first.verdict != AnalogueVerdict::NoClaim {
            return Err("limit sampling must carry the no-claim verdict".to_string());
        }
    }

    let oversized = AnalogueRequest {
        budget: BinderBudget { max_terms: 8 },
        ..identity(
            BinderKind::Sum,
            AnalogueDomain::IntegerRange {
                lower: 1,
                upper: 1000,
            },
        )
    };
    match oversized.evaluate() {
        Err(AnalogueError::BudgetExceeded { limit: 8 }) => {
            rows.push("refuse|sum-1000|budget-exceeded|limit=8".to_string());
            println!("finite-analogues|refuse|budget-exceeded|limit=8");
        }
        other => return Err(format!("budget overrun must refuse, got {other:?}")),
    }

    // Seeded negative control: a planted wrong quadrature value must be
    // detected by recomputing the same integral request.
    let quadrature = identity(
        BinderKind::Integral,
        AnalogueDomain::Interval {
            lower: 0.0,
            upper: 1.0,
            n: 4,
        },
    );
    let honest = quadrature
        .evaluate()
        .map_err(|error| format!("quadrature control refused: {error:?}"))?;
    let honest_bits = honest
        .value_bits
        .ok_or_else(|| "quadrature must produce a computed value".to_string())?;
    let seed = 13_u64;
    let planted = honest_bits ^ seed;
    if planted == honest_bits {
        return Err("seeded plant must differ from the honest quadrature".to_string());
    }
    let recomputed = quadrature
        .evaluate()
        .map_err(|error| format!("quadrature recompute refused: {error:?}"))?;
    if recomputed.value_bits != Some(honest_bits) {
        return Err("recompute must reproduce the honest quadrature bits".to_string());
    }
    if recomputed.value_bits == Some(planted) {
        return Err("recompute must reject the seeded wrong quadrature".to_string());
    }
    rows.push(format!(
        "negative|quadrature|honest={honest_bits:016x}|planted={planted:016x}|seed={seed}"
    ));
    println!(
        "finite-analogues|negative|quadrature|honest={honest_bits:016x}|planted={planted:016x}|detected"
    );

    let body = rows.join("\n");
    let receipt_id = fnv1a64(body.as_bytes());
    println!(
        "finite-analogues: rows={} receipt={receipt_id:016x}",
        rows.len()
    );
    Ok(())
}
