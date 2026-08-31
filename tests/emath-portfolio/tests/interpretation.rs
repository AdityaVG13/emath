//! interpretation tests migrated from the in-crate `#[cfg(test)]` module.

use emath_portfolio::interpretation::*;
use emath_portfolio::{Authority, WorldCandidate};
use emath_portfolio::record::GuardFailure;
use std::collections::{BTreeMap, BTreeSet};

fn metrics(cost: i64, utility: i64) -> BTreeMap<String, i64> {
    let mut map = BTreeMap::new();
    map.insert("cost".to_string(), cost);
    map.insert("utility".to_string(), utility);
    map
}

fn axes() -> Vec<MetricAxis> {
    vec![
        MetricAxis::new("cost", MetricPolarity::Minimize),
        MetricAxis::new("utility", MetricPolarity::Maximize),
    ]
}

fn world(fp: u64, authority: Authority, cost: i64, utility: i64) -> WorldCandidate {
    WorldCandidate::new(fp, format!("p{fp}"), authority, metrics(cost, utility), fp)
}

/// Hand-computed 3-candidate case:
/// W1 (cost=1, utility=2) and W2 (cost=3, utility=5) are non-dominated;
/// W3 (cost=4, utility=2) is dominated by W1 (worse cost, same utility).
fn frontier_three() -> Vec<WorldCandidate> {
    vec![
        world(3, Authority::Structural, 4, 2),
        world(1, Authority::Tested, 1, 2),
        world(2, Authority::Certified, 3, 5),
    ]
}

#[test]
fn ranking_is_deterministic_and_tie_breaks_on_fingerprint() {
    let axis = vec![MetricAxis::new("cost", MetricPolarity::Minimize)];
    let tied = vec![
        WorldCandidate::new(9, "p", Authority::Structural, metrics(1, 0), 90),
        WorldCandidate::new(4, "p", Authority::Structural, metrics(1, 0), 40),
    ];
    let first = rank_candidates(&tied, &axis)
        .into_iter()
        .map(|candidate| candidate.world_fingerprint)
        .collect::<Vec<_>>();
    let reversed = vec![tied[1].clone(), tied[0].clone()];
    let second = rank_candidates(&reversed, &axis)
        .into_iter()
        .map(|candidate| candidate.world_fingerprint)
        .collect::<Vec<_>>();
    assert_eq!(first, vec![4, 9]);
    assert_eq!(first, second);
}

#[test]
fn ranking_uses_authority_then_axes_not_input_order() {
    let ranked = rank_candidates(&frontier_three(), &axes())
        .into_iter()
        .map(|candidate| candidate.world_fingerprint)
        .collect::<Vec<_>>();
    // Certified (W2) before Tested (W1) before Structural (W3).
    assert_eq!(ranked, vec![2, 1, 3]);
}

#[test]
fn pareto_archive_matches_hand_computed_three() {
    let pareto = archive(&frontier_three(), &axes());
    let live: Vec<u64> = pareto
        .nondominated
        .iter()
        .map(|candidate| candidate.world_fingerprint)
        .collect();
    assert_eq!(live, vec![2, 1], "ranking-key order on the frontier");
    assert_eq!(pareto.dominated.len(), 1);
    assert_eq!(pareto.dominated[0].0.world_fingerprint, 3);
    assert_eq!(pareto.dominated[0].1, 1, "lowest-fingerprint witness is W1");
}

#[test]
fn single_best_refuses_when_several_nondominated_remain() {
    let error = evaluate(
        frontier_three(),
        axes(),
        InterpretationPolicy::SingleBest {
            collapse: CollapsePolicy::RequireUnique,
        },
    )
    .expect_err("exit gate");
    match error {
        PortfolioError::AmbiguousSingleBest { nondominated } => {
            assert_eq!(nondominated, vec![2, 1]);
        }
        other => panic!("expected AmbiguousSingleBest, got {other:?}"),
    }
}

#[test]
fn ledger_accounts_for_every_input_candidate() {
    let mut input = frontier_three();
    input.push(WorldCandidate {
        guard_failure: Some(GuardFailure {
            code: "hard-constraint:violated".to_string(),
            detail: "carrier empty".to_string(),
        }),
        ..world(7, Authority::Structural, 0, 9)
    });
    let receipt = evaluate(input.clone(), axes(), InterpretationPolicy::Portfolio)
        .expect("portfolio selection");
    let mut accounted = BTreeSet::new();
    accounted.extend(receipt.selected.iter().copied());
    accounted.extend(receipt.archived.iter().copied());
    accounted.extend(receipt.ledger.iter().map(|entry| entry.fingerprint));
    let expected: BTreeSet<u64> = input.iter().map(|c| c.world_fingerprint).collect();
    assert_eq!(accounted, expected);
    assert_eq!(
        receipt.selected.len() + receipt.archived.len() + receipt.ledger.len(),
        input.len()
    );
    assert!(receipt.archived.is_empty());
    assert_eq!(receipt.selected, vec![2, 1]);
    assert_eq!(receipt.ledger.len(), 2);
}

#[test]
fn replay_is_byte_identical() {
    let receipt =
        evaluate(frontier_three(), axes(), InterpretationPolicy::Portfolio).expect("portfolio");
    let again = replay(&receipt.input).expect("replay");
    assert_eq!(receipt.encode(), again.encode());
    assert_eq!(receipt.encode().as_bytes(), again.encode().as_bytes());
}

#[test]
fn authority_never_escalates_and_seeded_claim_is_refused() {
    let receipt =
        evaluate(frontier_three(), axes(), InterpretationPolicy::Portfolio).expect("portfolio");
    for fingerprint in receipt.selected.iter().chain(receipt.archived.iter()) {
        let candidate = receipt
            .input
            .candidates
            .iter()
            .find(|item| item.world_fingerprint == *fingerprint)
            .expect("input row");
        assert!(candidate.labeled_authority <= candidate.evidence_authority);
        assert_eq!(candidate.labeled_authority, candidate.evidence_authority);
    }

    let structural = world(11, Authority::Structural, 1, 1);
    let seeded = WorldCandidate {
        labeled_authority: Authority::Proved,
        ..structural
    };
    let error = evaluate(vec![seeded], axes(), InterpretationPolicy::Portfolio)
        .expect_err("seeded escalation");
    match error {
        PortfolioError::AuthorityEscalation {
            fingerprint,
            evidence,
            claimed,
        } => {
            assert_eq!(fingerprint, 11);
            assert_eq!(evidence, Authority::Structural);
            assert_eq!(claimed, Authority::Proved);
        }
        other => panic!("expected AuthorityEscalation, got {other:?}"),
    }

    let claim = world(12, Authority::Tested, 1, 1)
        .with_claimed_label(Authority::Certified)
        .expect_err("builder gate");
    assert!(matches!(
        claim,
        PortfolioError::AuthorityEscalation {
            evidence: Authority::Tested,
            claimed: Authority::Certified,
            ..
        }
    ));
}

#[test]
fn explicit_rank_key_collapse_selects_one_and_archives_the_rest() {
    let receipt = evaluate(
        frontier_three(),
        axes(),
        InterpretationPolicy::SingleBest {
            collapse: CollapsePolicy::RankKey,
        },
    )
    .expect("explicit collapse");
    assert_eq!(receipt.selected, vec![2]);
    assert_eq!(receipt.archived, vec![1]);
    assert_eq!(receipt.ledger.len(), 1);
    assert_eq!(
        receipt.selected.len() + receipt.archived.len() + receipt.ledger.len(),
        3
    );
}
