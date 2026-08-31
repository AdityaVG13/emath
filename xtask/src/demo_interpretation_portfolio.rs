//! G7 production-path demo: rank, archive, and select mixed-authority
//! interpretation worlds under both policies, print a receipt twice, and
//! show the single-best and authority-escalation refusals.

use std::collections::BTreeMap;

use emath_cli::portfolio::{
    archive, evaluate, rank_candidates, replay, Authority, CollapsePolicy, InterpretationPolicy,
    MetricAxis, MetricPolarity, PortfolioError, WorldCandidate,
};

pub(crate) fn run() -> u8 {
    println!("== demo interpretation-portfolio ==");
    match run_demo() {
        Ok(()) => {
            println!("interpretation-portfolio demo: ok");
            0
        }
        Err(error) => {
            eprintln!("interpretation-portfolio demo FAILED: {error}");
            1
        }
    }
}

fn run_demo() -> Result<(), String> {
    let axes = vec![
        MetricAxis::new("cost", MetricPolarity::Minimize),
        MetricAxis::new("utility", MetricPolarity::Maximize),
    ];
    let candidates = vec![
        world(1, "free-term", Authority::Tested, 1, 2, 0x11),
        world(2, "certified-numeric", Authority::Certified, 3, 5, 0x22),
        world(3, "structural-approx", Authority::Structural, 4, 2, 0x33),
        world(4, "proved-slow", Authority::Proved, 8, 4, 0x44),
    ];

    let ranked = rank_candidates(&candidates, &axes);
    println!(
        "rank: {}",
        ranked
            .iter()
            .map(|candidate| format!(
                "{:016x}:{}",
                candidate.world_fingerprint,
                candidate.evidence_authority.as_str()
            ))
            .collect::<Vec<_>>()
            .join(" ")
    );

    let pareto = archive(&candidates, &axes);
    println!(
        "archive: nondominated={} dominated={}",
        pareto.nondominated.len(),
        pareto.dominated.len()
    );
    for candidate in &pareto.nondominated {
        println!("  live {:016x}", candidate.world_fingerprint);
    }
    for (candidate, by) in &pareto.dominated {
        println!(
            "  dominated {:016x} by={by:016x}",
            candidate.world_fingerprint
        );
    }

    let portfolio = evaluate(
        candidates.clone(),
        axes.clone(),
        InterpretationPolicy::Portfolio,
    )
    .map_err(|error| error.to_string())?;
    let encoded = portfolio.encode();
    let again = replay(&portfolio.input).map_err(|error| error.to_string())?;
    if encoded.as_bytes() != again.encode().as_bytes() {
        return Err("replay is not byte-identical".to_string());
    }
    println!("receipt-1:\n{encoded}");
    println!("receipt-2:\n{}", again.encode());
    println!(
        "replay-byte-identical receipt_id={:016x}",
        portfolio.receipt_id
    );

    let collapsed = evaluate(
        candidates.clone(),
        axes.clone(),
        InterpretationPolicy::SingleBest {
            collapse: CollapsePolicy::RankKey,
        },
    )
    .map_err(|error| error.to_string())?;
    println!(
        "single-best rank-key selected={} archived={}",
        collapsed
            .selected
            .iter()
            .map(|fp| format!("{fp:016x}"))
            .collect::<Vec<_>>()
            .join(","),
        collapsed
            .archived
            .iter()
            .map(|fp| format!("{fp:016x}"))
            .collect::<Vec<_>>()
            .join(",")
    );

    match evaluate(
        candidates,
        axes.clone(),
        InterpretationPolicy::SingleBest {
            collapse: CollapsePolicy::RequireUnique,
        },
    ) {
        Err(PortfolioError::AmbiguousSingleBest { nondominated }) => {
            println!(
                "single-best-refusal: {} non-dominated ({})",
                nondominated.len(),
                nondominated
                    .iter()
                    .map(|fp| format!("{fp:016x}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        other => {
            return Err(format!("expected AmbiguousSingleBest, got {other:?}"));
        }
    }

    let seeded = WorldCandidate {
        labeled_authority: Authority::Proved,
        ..world(11, "escalator", Authority::Structural, 1, 9, 0xaa)
    };
    match evaluate(vec![seeded], axes, InterpretationPolicy::Portfolio) {
        Err(PortfolioError::AuthorityEscalation {
            fingerprint,
            evidence,
            claimed,
        }) => {
            println!(
                "authority-escalation-refusal: fp={fingerprint:016x} evidence={} claimed={}",
                evidence.as_str(),
                claimed.as_str()
            );
        }
        other => {
            return Err(format!("expected AuthorityEscalation, got {other:?}"));
        }
    }
    Ok(())
}

fn world(
    fingerprint: u64,
    provider: &str,
    authority: Authority,
    cost: i64,
    utility: i64,
    artifact: u64,
) -> WorldCandidate {
    let mut metrics = BTreeMap::new();
    metrics.insert("cost".to_string(), cost);
    metrics.insert("utility".to_string(), utility);
    WorldCandidate::new(fingerprint, provider, authority, metrics, artifact)
}
