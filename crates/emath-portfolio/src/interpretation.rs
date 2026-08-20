//! G7 interpretation portfolios: integer ranking, Pareto archive,
//! selection with an explicit collapse gate, disqualification ledger,
//! and byte-identical receipt replay.
//!
//! Authority never escalates: ranking and selection copy
//! [`WorldCandidate::labeled_authority`], which must already be `<=`
//! evidence authority.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use emath_world_ir::fnv1a64;

use crate::record::WorldCandidate;
use crate::Authority;

/// Durable receipt schema id.
pub const RECEIPT_SCHEMA: &str = "emath.interpretation-portfolio-receipt";
/// Receipt layout version. Bump on any change to [`PortfolioReceipt::encode`].
pub const RECEIPT_VERSION: u32 = 1;

/// Documented lexicographic ranking key (total order; integer metrics only):
///
/// 1. `evidence_authority` descending (`Proved > Certified > Tested > Structural`)
/// 2. each declared metric axis in declaration order:
///    `Maximize` → larger `i64` first; `Minimize` → smaller `i64` first
/// 3. `world_fingerprint` ascending (bit-exact tie-break)
/// 4. `provider_id` ascending
/// 5. `artifact_hash` ascending
pub const RANKING_KEY_SPEC: &str =
    "authority.desc,axes.declared,fingerprint.asc,provider.asc,artifact.asc";

/// Metric polarity on a declared Pareto / ranking axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricPolarity {
    /// Larger integer is better.
    Maximize,
    /// Smaller integer is better.
    Minimize,
}

impl MetricPolarity {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Maximize => "max",
            Self::Minimize => "min",
        }
    }
}

/// One declared metric axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricAxis {
    /// Metric key in [`WorldCandidate::metrics`].
    pub name: String,
    /// Optimization direction.
    pub polarity: MetricPolarity,
}

impl MetricAxis {
    /// Declares a named axis.
    #[must_use]
    pub fn new(name: impl Into<String>, polarity: MetricPolarity) -> Self {
        Self {
            name: name.into(),
            polarity,
        }
    }

    fn canonical(&self) -> String {
        format!("{}:{}", self.name, self.polarity.as_str())
    }
}

/// When `single-best` may collapse several non-dominated worlds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollapsePolicy {
    /// Legal only when the archive has exactly one member.
    RequireUnique,
    /// Explicit permission to pick the ranking-key winner among the archive.
    RankKey,
}

impl CollapsePolicy {
    const fn as_str(self) -> &'static str {
        match self {
            Self::RequireUnique => "require-unique",
            Self::RankKey => "rank-key",
        }
    }
}

/// G7 selection policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterpretationPolicy {
    /// Keep every non-dominated candidate. Never collapses.
    Portfolio,
    /// Collapse to one winner only when legal under `collapse`.
    SingleBest {
        /// Collapse rule. [`CollapsePolicy::RequireUnique`] is the exit gate.
        collapse: CollapsePolicy,
    },
    /// User-locked single world. Provenance is `user-locked`; authority
    /// is never raised. `selected_at` is not part of this policy.
    UserLocked {
        /// Identity of the project lock document.
        lock_id: u64,
        /// Portfolio receipt the user picked from.
        origin_receipt_id: u64,
        /// Selection method wire name (`cli-set`, `agent`, `file-edit`).
        method: String,
    },
}

impl InterpretationPolicy {
    /// Wire name used in receipts and CLI summaries.
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Portfolio => "portfolio".to_string(),
            Self::SingleBest { collapse } => format!("single-best:{}", collapse.as_str()),
            Self::UserLocked {
                lock_id,
                origin_receipt_id,
                method,
            } => format!(
                "user-locked:lock={lock_id:016x}:from={origin_receipt_id:016x}:method={method}"
            ),
        }
    }
}

/// Why a candidate left the live set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisqualificationReason {
    /// Strictly dominated on the declared axes. `by` is the lowest-fingerprint
    /// witness dominator.
    Dominated {
        /// Witness dominator fingerprint.
        by: u64,
    },
    /// Failed an applicability guard (including a missing declared metric).
    FailedGuard {
        /// Machine-readable code.
        code: String,
        /// Human-readable detail.
        detail: String,
    },
    /// Typed refusal that still accounts for the candidate on the ledger.
    Refused {
        /// Machine-readable code.
        code: String,
        /// Human-readable detail.
        detail: String,
    },
}

impl DisqualificationReason {
    /// Canonical ledger encoding (also used in lock-set diagnostics).
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Dominated { by } => format!("dominated:by={by:016x}"),
            Self::FailedGuard { code, detail } => format!("failed-guard:{code}:{detail}"),
            Self::Refused { code, detail } => format!("refused:{code}:{detail}"),
        }
    }
}

/// One ledger row. Every removed candidate has exactly one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    /// Candidate fingerprint.
    pub fingerprint: u64,
    /// Removal reason.
    pub reason: DisqualificationReason,
}

/// Non-dominated set plus recorded dominated members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParetoArchive {
    /// Non-dominated candidates, ranking-key order.
    pub nondominated: Vec<WorldCandidate>,
    /// Dominated candidates with a deterministic witness fingerprint.
    pub dominated: Vec<(WorldCandidate, u64)>,
}

/// Enough of a run to replay selection byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptInput {
    /// Selection policy.
    pub policy: InterpretationPolicy,
    /// Declared metric axes, in ranking / Pareto order.
    pub axes: Vec<MetricAxis>,
    /// Input candidates, sorted by fingerprint in the receipt.
    pub candidates: Vec<WorldCandidate>,
}

/// Deterministic portfolio receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortfolioReceipt {
    /// Replay input.
    pub input: ReceiptInput,
    /// Fingerprints in ranking-key order (viable candidates only).
    pub ranked: Vec<u64>,
    /// Policy winners, ranking-key order.
    pub selected: Vec<u64>,
    /// Non-dominated candidates not selected, ranking-key order.
    pub archived: Vec<u64>,
    /// Every removed candidate, fingerprint ascending.
    pub ledger: Vec<LedgerEntry>,
    /// FNV-1a64 of the canonical body (excludes this field).
    pub receipt_id: u64,
}

impl PortfolioReceipt {
    /// Canonical encoding, including `receipt_id`.
    #[must_use]
    pub fn encode(&self) -> String {
        format!("{}receipt_id={:016x}\n", self.body(), self.receipt_id)
    }

    fn body(&self) -> String {
        let axes = self
            .input
            .axes
            .iter()
            .map(MetricAxis::canonical)
            .collect::<Vec<_>>()
            .join(",");
        let candidates = self
            .input
            .candidates
            .iter()
            .map(WorldCandidate::canonical)
            .collect::<Vec<_>>()
            .join("\n");
        let ranked = join_fps(&self.ranked);
        let selected = join_fps(&self.selected);
        let archived = join_fps(&self.archived);
        let ledger = self
            .ledger
            .iter()
            .map(|entry| format!("{:016x}:{}", entry.fingerprint, entry.reason.canonical()))
            .collect::<Vec<_>>()
            .join(";");
        format!(
            "{RECEIPT_SCHEMA}:{RECEIPT_VERSION}\nkey={RANKING_KEY_SPEC}\npolicy={}\naxes={axes}\ncandidates={}\n{candidates}\nranked={ranked}\nselected={selected}\narchived={archived}\nledger={ledger}\n",
            self.input.policy.canonical(),
            self.input.candidates.len(),
        )
    }
}

/// Typed G7 refusals. Authority never escalates through a successful receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortfolioError {
    /// `single-best` with more than one non-dominated world and no collapse.
    AmbiguousSingleBest {
        /// Non-dominated fingerprints, ranking-key order.
        nondominated: Vec<u64>,
    },
    /// No viable candidate remained after guards.
    NoViableCandidate,
    /// A candidate presented a label above its evidence authority.
    AuthorityEscalation {
        /// Offending fingerprint.
        fingerprint: u64,
        /// Authority supported by evidence.
        evidence: Authority,
        /// Attempted label.
        claimed: Authority,
    },
    /// Two candidates share a world fingerprint.
    DuplicateFingerprint {
        /// Repeated fingerprint.
        fingerprint: u64,
    },
}

impl fmt::Display for PortfolioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AmbiguousSingleBest { nondominated } => write!(
                formatter,
                "single-best refused: {} non-dominated worlds remain ({})",
                nondominated.len(),
                join_fps(nondominated)
            ),
            Self::NoViableCandidate => {
                write!(formatter, "single-best refused: no viable candidate")
            }
            Self::AuthorityEscalation {
                fingerprint,
                evidence,
                claimed,
            } => write!(
                formatter,
                "authority escalation refused: fp={fingerprint:016x} evidence={} claimed={}",
                evidence.as_str(),
                claimed.as_str()
            ),
            Self::DuplicateFingerprint { fingerprint } => {
                write!(formatter, "duplicate world fingerprint {fingerprint:016x}")
            }
        }
    }
}

impl std::error::Error for PortfolioError {}

/// Ranks viable candidates by [`RANKING_KEY_SPEC`]. Input order is ignored.
#[must_use]
pub fn rank_candidates<'a>(
    candidates: &'a [WorldCandidate],
    axes: &[MetricAxis],
) -> Vec<&'a WorldCandidate> {
    let mut ranked: Vec<&WorldCandidate> = candidates.iter().collect();
    ranked.sort_by(|left, right| compare_ranking_key(left, right, axes));
    ranked
}

/// Builds the Pareto archive over `axes`. Dominated members are recorded.
#[must_use]
pub fn archive(candidates: &[WorldCandidate], axes: &[MetricAxis]) -> ParetoArchive {
    let mut dominated = Vec::new();
    let mut nondominated = Vec::new();
    for candidate in candidates {
        if let Some(by) = witness_dominator(candidate, candidates, axes) {
            dominated.push((candidate.clone(), by));
        } else {
            nondominated.push(candidate.clone());
        }
    }
    nondominated.sort_by(|left, right| compare_ranking_key(left, right, axes));
    dominated.sort_by(|(left, left_by), (right, right_by)| {
        left.world_fingerprint
            .cmp(&right.world_fingerprint)
            .then(left_by.cmp(right_by))
    });
    ParetoArchive {
        nondominated,
        dominated,
    }
}

/// Runs ranking, archive, and selection. The receipt is replayable.
pub fn evaluate(
    candidates: Vec<WorldCandidate>,
    axes: Vec<MetricAxis>,
    policy: InterpretationPolicy,
) -> Result<PortfolioReceipt, PortfolioError> {
    check_unique_fingerprints(&candidates)?;
    for candidate in &candidates {
        if candidate.labeled_authority > candidate.evidence_authority {
            return Err(PortfolioError::AuthorityEscalation {
                fingerprint: candidate.world_fingerprint,
                evidence: candidate.evidence_authority,
                claimed: candidate.labeled_authority,
            });
        }
    }

    let mut ledger = Vec::new();
    let mut viable = Vec::new();
    for candidate in &candidates {
        if let Some(failure) = &candidate.guard_failure {
            ledger.push(LedgerEntry {
                fingerprint: candidate.world_fingerprint,
                reason: DisqualificationReason::FailedGuard {
                    code: failure.code.clone(),
                    detail: failure.detail.clone(),
                },
            });
            continue;
        }
        if let Some(name) = missing_axis(candidate, &axes) {
            ledger.push(LedgerEntry {
                fingerprint: candidate.world_fingerprint,
                reason: DisqualificationReason::FailedGuard {
                    code: "missing-metric".to_string(),
                    detail: name,
                },
            });
            continue;
        }
        viable.push(candidate.clone());
    }

    let ranked_refs = rank_candidates(&viable, &axes);
    let ranked: Vec<u64> = ranked_refs
        .iter()
        .map(|candidate| candidate.world_fingerprint)
        .collect();
    let pareto = archive(&viable, &axes);
    for (candidate, by) in &pareto.dominated {
        ledger.push(LedgerEntry {
            fingerprint: candidate.world_fingerprint,
            reason: DisqualificationReason::Dominated { by: *by },
        });
    }
    ledger.sort_by_key(|entry| entry.fingerprint);

    let (selected, archived) = apply_policy(&pareto, &policy)?;
    let receipt = finish_receipt(candidates, axes, policy, ranked, selected, archived, ledger);
    check_authority_invariant(&receipt)?;
    Ok(receipt)
}

/// Replays selection from a receipt input section. Byte-identical on success.
pub fn replay(input: &ReceiptInput) -> Result<PortfolioReceipt, PortfolioError> {
    evaluate(
        input.candidates.clone(),
        input.axes.clone(),
        input.policy.clone(),
    )
}

fn apply_policy(
    pareto: &ParetoArchive,
    policy: &InterpretationPolicy,
) -> Result<(Vec<u64>, Vec<u64>), PortfolioError> {
    let nondominated: Vec<u64> = pareto
        .nondominated
        .iter()
        .map(|candidate| candidate.world_fingerprint)
        .collect();
    match policy {
        InterpretationPolicy::Portfolio => Ok((nondominated, Vec::new())),
        InterpretationPolicy::SingleBest { collapse } => {
            if nondominated.is_empty() {
                return Err(PortfolioError::NoViableCandidate);
            }
            if nondominated.len() == 1 {
                return Ok((nondominated, Vec::new()));
            }
            match collapse {
                CollapsePolicy::RequireUnique => {
                    Err(PortfolioError::AmbiguousSingleBest { nondominated })
                }
                CollapsePolicy::RankKey => {
                    let selected = vec![nondominated[0]];
                    let archived = nondominated.into_iter().skip(1).collect();
                    Ok((selected, archived))
                }
            }
        }
        InterpretationPolicy::UserLocked { .. } => {
            if nondominated.len() == 1 {
                Ok((nondominated, Vec::new()))
            } else if nondominated.is_empty() {
                Err(PortfolioError::NoViableCandidate)
            } else {
                Err(PortfolioError::AmbiguousSingleBest { nondominated })
            }
        }
    }
}

fn finish_receipt(
    mut candidates: Vec<WorldCandidate>,
    axes: Vec<MetricAxis>,
    policy: InterpretationPolicy,
    ranked: Vec<u64>,
    selected: Vec<u64>,
    archived: Vec<u64>,
    ledger: Vec<LedgerEntry>,
) -> PortfolioReceipt {
    candidates.sort_by_key(|candidate| candidate.world_fingerprint);
    let mut receipt = PortfolioReceipt {
        input: ReceiptInput {
            policy,
            axes,
            candidates,
        },
        ranked,
        selected,
        archived,
        ledger,
        receipt_id: 0,
    };
    receipt.receipt_id = fnv1a64(receipt.body().as_bytes());
    receipt
}

fn check_authority_invariant(receipt: &PortfolioReceipt) -> Result<(), PortfolioError> {
    let by_fp: BTreeMap<u64, &WorldCandidate> = receipt
        .input
        .candidates
        .iter()
        .map(|candidate| (candidate.world_fingerprint, candidate))
        .collect();
    for fingerprint in receipt.selected.iter().chain(receipt.archived.iter()) {
        let Some(candidate) = by_fp.get(fingerprint) else {
            continue;
        };
        if candidate.labeled_authority > candidate.evidence_authority {
            return Err(PortfolioError::AuthorityEscalation {
                fingerprint: *fingerprint,
                evidence: candidate.evidence_authority,
                claimed: candidate.labeled_authority,
            });
        }
    }
    Ok(())
}

fn check_unique_fingerprints(candidates: &[WorldCandidate]) -> Result<(), PortfolioError> {
    let mut seen = BTreeSet::new();
    for candidate in candidates {
        if !seen.insert(candidate.world_fingerprint) {
            return Err(PortfolioError::DuplicateFingerprint {
                fingerprint: candidate.world_fingerprint,
            });
        }
    }
    Ok(())
}

fn missing_axis(candidate: &WorldCandidate, axes: &[MetricAxis]) -> Option<String> {
    axes.iter()
        .find(|axis| !candidate.metrics.contains_key(&axis.name))
        .map(|axis| axis.name.clone())
}

fn witness_dominator(
    candidate: &WorldCandidate,
    pool: &[WorldCandidate],
    axes: &[MetricAxis],
) -> Option<u64> {
    let mut witness: Option<u64> = None;
    for other in pool {
        if other.world_fingerprint == candidate.world_fingerprint {
            continue;
        }
        if dominates(other, candidate, axes) {
            witness = Some(match witness {
                Some(current) => current.min(other.world_fingerprint),
                None => other.world_fingerprint,
            });
        }
    }
    witness
}

/// `left` is at least as good as `right` on every declared axis and strictly
/// better on at least one. Missing metrics are treated as incomparable
/// (callers strip them first).
fn dominates(left: &WorldCandidate, right: &WorldCandidate, axes: &[MetricAxis]) -> bool {
    if axes.is_empty() {
        return false;
    }
    let mut not_worse = true;
    let mut strict = false;
    for axis in axes {
        let Some(&left_value) = left.metrics.get(&axis.name) else {
            return false;
        };
        let Some(&right_value) = right.metrics.get(&axis.name) else {
            return false;
        };
        match axis.polarity {
            MetricPolarity::Maximize => {
                if left_value < right_value {
                    not_worse = false;
                }
                if left_value > right_value {
                    strict = true;
                }
            }
            MetricPolarity::Minimize => {
                if left_value > right_value {
                    not_worse = false;
                }
                if left_value < right_value {
                    strict = true;
                }
            }
        }
    }
    not_worse && strict
}

fn compare_ranking_key(
    left: &WorldCandidate,
    right: &WorldCandidate,
    axes: &[MetricAxis],
) -> Ordering {
    left.evidence_authority
        .lattice_rank()
        .cmp(&right.evidence_authority.lattice_rank())
        .reverse()
        .then_with(|| compare_axes(left, right, axes))
        .then(left.world_fingerprint.cmp(&right.world_fingerprint))
        .then_with(|| left.provider_id.cmp(&right.provider_id))
        .then(left.artifact_hash.cmp(&right.artifact_hash))
}

fn compare_axes(left: &WorldCandidate, right: &WorldCandidate, axes: &[MetricAxis]) -> Ordering {
    for axis in axes {
        let left_value = left.metrics.get(&axis.name).copied().unwrap_or(0);
        let right_value = right.metrics.get(&axis.name).copied().unwrap_or(0);
        let ordering = match axis.polarity {
            MetricPolarity::Maximize => right_value.cmp(&left_value),
            MetricPolarity::Minimize => left_value.cmp(&right_value),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

fn join_fps(fingerprints: &[u64]) -> String {
    fingerprints
        .iter()
        .map(|fingerprint| format!("{fingerprint:016x}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::GuardFailure;
    use std::collections::BTreeMap;

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
}
