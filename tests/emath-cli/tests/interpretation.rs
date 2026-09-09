//! interpretation tests migrated from the in-crate `#[cfg(test)]` module.
use emath_cli::portfolio::interpretation::*;
use emath_cli::portfolio::record::GuardFailure;
use emath_cli::portfolio::{Authority, WorldCandidate};
use emath_test_harness::{Case, Probe, check_all, expect_ok};
use std::collections::{BTreeMap, BTreeSet};
fn metrics(cost: i64, utility: i64) -> BTreeMap<String, i64> { BTreeMap::from([("cost".into(), cost), ("utility".into(), utility)]) }
fn axes() -> Vec<MetricAxis> { vec![MetricAxis::new("cost", MetricPolarity::Minimize), MetricAxis::new("utility", MetricPolarity::Maximize)] }
fn world(fp: u64, authority: Authority, cost: i64, utility: i64) -> WorldCandidate { WorldCandidate::new(fp, format!("p{fp}"), authority, metrics(cost, utility), fp) }
fn frontier_three() -> Vec<WorldCandidate> { vec![world(3, Authority::Structural, 4, 2), world(1, Authority::Tested, 1, 2), world(2, Authority::Certified, 3, 5)] }
#[test]
fn probe() {
    let mut p = Probe::new("interpretation ranking, pareto, ledger, and authority gates");
    expect_ok(check_all(&[Case::new("tie-fp", vec![world(9, Authority::Structural, 1, 0), world(4, Authority::Structural, 1, 0)], vec![4, 9])], |cs| { let axis = vec![MetricAxis::new("cost", MetricPolarity::Minimize)]; rank_candidates(cs, &axis).into_iter().map(|c| c.world_fingerprint).collect::<Vec<_>>() })); expect_ok(check_all(&[Case::new("authority-first", frontier_three(), vec![2, 1, 3])], |cs| rank_candidates(cs, &axes()).into_iter().map(|c| c.world_fingerprint).collect::<Vec<_>>()));
    p.case("pareto", |p| { let a = archive(&frontier_three(), &axes()); p.eq("live", a.nondominated.iter().map(|c| c.world_fingerprint).collect::<Vec<_>>(), vec![2, 1]); p.eq("dom-n", a.dominated.len(), 1); p.eq("dom-fp", a.dominated[0].0.world_fingerprint, 3); p.eq("witness", a.dominated[0].1, 1); });
    p.case("single-best", |p| { match evaluate(frontier_three(), axes(), InterpretationPolicy::SingleBest { collapse: CollapsePolicy::RequireUnique }) { Err(PortfolioError::AmbiguousSingleBest { nondominated }) => { p.eq("nondom", nondominated, vec![2, 1]); }, other => { p.fail("gate", format!("expected AmbiguousSingleBest, got {other:?}")); } } });
    p.case("ledger", |p| { let mut input = frontier_three(); input.push(WorldCandidate { guard_failure: Some(GuardFailure { code: "hard-constraint:violated".into(), detail: "carrier empty".into() }), ..world(7, Authority::Structural, 0, 9) }); let r = evaluate(input.clone(), axes(), InterpretationPolicy::Portfolio).expect("portfolio"); let mut acc = BTreeSet::new(); acc.extend(r.selected.iter().copied()); acc.extend(r.archived.iter().copied()); acc.extend(r.ledger.iter().map(|e| e.fingerprint)); p.eq("accounts", acc, input.iter().map(|c| c.world_fingerprint).collect()); p.eq("selected", r.selected, vec![2, 1]); p.eq("ledger-n", r.ledger.len(), 2); });
    p.case("replay", |p| { let r = evaluate(frontier_three(), axes(), InterpretationPolicy::Portfolio).expect("p"); p.eq("bytes", replay(&r.input).expect("replay").encode(), r.encode()); });
    p.case("authority", |p| {
        let r = evaluate(frontier_three(), axes(), InterpretationPolicy::Portfolio).expect("p");
        p.demand("never-escalates", r.selected.iter().chain(r.archived.iter()).all(|fp| r.input.candidates.iter().find(|c| c.world_fingerprint == *fp).is_some_and(|c| c.labeled_authority <= c.evidence_authority)), "labels never exceed evidence");
        match evaluate(vec![WorldCandidate { labeled_authority: Authority::Proved, ..world(11, Authority::Structural, 1, 1) }], axes(), InterpretationPolicy::Portfolio) { Err(PortfolioError::AuthorityEscalation { fingerprint: 11, .. }) => { p.demand("seeded", true, "ok"); }, other => { p.fail("seeded", format!("expected escalation, got {other:?}")); } }
        p.demand("builder-gate", world(12, Authority::Tested, 1, 1).with_claimed_label(Authority::Certified).is_err(), "builder refuses escalation");
    });
    p.case("rankkey", |p| { let r = evaluate(frontier_three(), axes(), InterpretationPolicy::SingleBest { collapse: CollapsePolicy::RankKey }).expect("collapse"); p.eq("sel", r.selected, vec![2]); p.eq("arch", r.archived, vec![1]); p.eq("ledger", r.ledger.len(), 1); });
    p.finish();
}
