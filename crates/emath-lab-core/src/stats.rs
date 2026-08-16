//!: statistical protocol.
//!
//! Freezes warmup, repetition count, randomization seed, paired
//! comparison, outlier policy and raw-sample retention before runs. All
//! computations are pure functions of the injected samples, so two runs
//! with identical inputs produce byte-identical summaries
//! (`E-HOST-003` structural errors, `E-HOST-006` insufficient evidence,
//! `E-HOST-008` incomparable inputs).

use crate::error::LabError;
use crate::Sampler;

/// Outlier policy for paired samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutlierPolicy {
    /// Keep every post-warmup sample.
    KeepAll,
    /// Trim pairs whose baseline deviates more than `factor * MAD`
    /// from the baseline median (degenerate zero MAD keeps all).
    MadTrim { factor: f64 },
}

/// Frozen statistical protocol.
#[derive(Clone, Debug, PartialEq)]
pub struct StatisticalProtocol {
    /// Repetitions discarded at the start of each sequence.
    pub warmup_repetitions: u64,
    /// Total repetitions per artifact.
    pub repetitions: u64,
    /// Minimum post-warmup samples for evidence.
    pub min_repetitions: u64,
    /// Paired (same seeded inputs for both artifacts).
    pub paired: bool,
    /// Deterministic randomization seed.
    pub seed: u64,
    /// Outlier handling.
    pub outlier: OutlierPolicy,
    /// Whether raw samples must be retained for receipts.
    pub retain_raw: bool,
    /// Whether sample order is shuffled (deterministically) before analysis.
    pub randomize_order: bool,
}

impl StatisticalProtocol {
    /// Validates the protocol configuration (`E-HOST-003`).
    pub fn validate(&self) -> Result<(), LabError> {
        if self.repetitions < self.min_repetitions {
            return Err(LabError::new(
                "E-HOST-003",
                format!(
                    "repetitions {} below protocol minimum {}",
                    self.repetitions, self.min_repetitions
                ),
            ));
        }
        if self.warmup_repetitions >= self.repetitions {
            return Err(LabError::new(
                "E-HOST-003",
                "warmup repetitions must be less than repetitions",
            ));
        }
        if let OutlierPolicy::MadTrim { factor } = self.outlier {
            if factor <= 0.0 || !factor.is_finite() {
                return Err(LabError::new(
                    "E-HOST-003",
                    "MAD trim factor must be positive and finite",
                ));
            }
        }
        Ok(())
    }
}

/// One paired observation (`baseline_ns`, `candidate_ns`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairedObservation {
    /// Baseline duration in nanoseconds.
    pub baseline_ns: u64,
    /// Candidate duration in nanoseconds.
    pub candidate_ns: u64,
}

/// Deterministic paired-comparison result.
#[derive(Clone, Debug, PartialEq)]
pub struct PairedResult {
    /// Post-warmup, post-cull samples used.
    pub samples_used: u64,
    /// Outlier pairs removed.
    pub outliers_removed: u64,
    /// Baseline median (ns).
    pub median_baseline_ns: f64,
    /// Candidate median (ns).
    pub median_candidate_ns: f64,
    /// Median of per-pair candidate/baseline ratios (1.0 = equal,
    /// > 1.0 = candidate slower, < 1.0 = candidate faster).
    pub median_ratio: f64,
    /// p99 of per-pair candidate/baseline ratios.
    pub p99_ratio: f64,
    /// Pairs where the candidate was faster.
    pub wins: u64,
    /// Pairs where the candidate was slower.
    pub losses: u64,
    /// Pairs where both ran in the same number of nanoseconds.
    pub ties: u64,
    /// Whether raw samples were retained as declared.
    pub raw_retained: bool,
    /// Whether the comparison was paired.
    pub paired: bool,
    /// Randomization seed used (when randomization is on).
    pub seed: u64,
}

/// Evaluates the paired comparison under the protocol.
///
/// Sequence: warmup drop, optional deterministic shuffle, MAD cull,
/// per-pair ratio statistics. Zero-duration samples make ratios undefined
/// and are refused (`E-HOST-008`).
#[allow(clippy::cast_precision_loss)] // ns counts -> ratios; below 2^53 the cast is exact
pub fn evaluate_paired(
    protocol: &StatisticalProtocol,
    mut observations: Vec<PairedObservation>,
) -> Result<PairedResult, LabError> {
    protocol.validate()?;
    if !protocol.paired {
        return Err(LabError::new(
            "E-HOST-008",
            "paired analysis requires a paired protocol",
        ));
    }
    if let Some(bad) = observations
        .iter()
        .find(|observation| observation.baseline_ns == 0 || observation.candidate_ns == 0)
    {
        return Err(LabError::new(
            "E-HOST-008",
            format!(
                "zero-duration paired sample (baseline={}, candidate={})",
                bad.baseline_ns, bad.candidate_ns
            ),
        ));
    }
    let warmup = usize::try_from(protocol.warmup_repetitions)
        .map_err(|_| LabError::new("E-HOST-003", "warmup repetitions do not fit usize"))?;
    if observations.len() <= warmup {
        return Err(LabError::new(
            "E-HOST-006",
            format!(
                "no samples remain after warmup ({} observed, {} warmup)",
                observations.len(),
                protocol.warmup_repetitions
            ),
        ));
    }
    observations.drain(..warmup);
    if protocol.randomize_order {
        shuffle(&mut observations, protocol.seed);
    }
    let outliers_removed = cull_outliers(protocol.outlier, &mut observations);
    let samples_used = observations.len();
    let min_repetitions = usize::try_from(protocol.min_repetitions)
        .map_err(|_| LabError::new("E-HOST-003", "min_repetitions does not fit usize"))?;
    if samples_used < min_repetitions {
        return Err(LabError::new(
            "E-HOST-006",
            format!(
                "insufficient evidence after cull: {samples_used} samples, need {}",
                protocol.min_repetitions
            ),
        ));
    }
    let baselines: Vec<u64> = observations
        .iter()
        .map(|sample| sample.baseline_ns)
        .collect();
    let candidates: Vec<u64> = observations
        .iter()
        .map(|sample| sample.candidate_ns)
        .collect();
    let ratios: Vec<f64> = observations
        .iter()
        .map(|sample| sample.candidate_ns as f64 / sample.baseline_ns as f64)
        .collect();
    let wins = observations
        .iter()
        .filter(|sample| sample.candidate_ns < sample.baseline_ns)
        .count() as u64;
    let losses = observations
        .iter()
        .filter(|sample| sample.candidate_ns > sample.baseline_ns)
        .count() as u64;
    let ties = observations
        .iter()
        .filter(|sample| sample.candidate_ns == sample.baseline_ns)
        .count() as u64;
    Ok(PairedResult {
        samples_used: samples_used as u64,
        outliers_removed,
        median_baseline_ns: percentile(&baselines, 0.5),
        median_candidate_ns: percentile(&candidates, 0.5),
        median_ratio: percentile_f64(&ratios, 0.5),
        p99_ratio: percentile_f64(&ratios, 0.99),
        wins,
        losses,
        ties,
        raw_retained: protocol.retain_raw,
        paired: true,
        seed: protocol.seed,
    })
}

/// Deterministic Fisher-Yates shuffle with the sampler seed.
fn shuffle(observations: &mut [PairedObservation], seed: u64) {
    let mut sampler = Sampler::new(seed);
    for index in (1..observations.len()).rev() {
        let pick = next_index(&mut sampler, index + 1);
        observations.swap(index, pick);
    }
}

/// Uniform index in `[0, n)` from the unit sampler. The scaled value is in
/// `[0, n)` by construction, so truncation and sign are safe.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn next_index(sampler: &mut Sampler, n: usize) -> usize {
    let scaled = (sampler.next_unit() + 1.0) / 2.0 * n as f64;
    let index = scaled as usize;
    index.min(n - 1)
}

/// Trim pairs by MAD on the baseline sequence; returns count removed.
#[must_use]
#[allow(clippy::cast_precision_loss)] // ns -> f64 deviations; exact below 2^53
fn cull_outliers(policy: OutlierPolicy, observations: &mut Vec<PairedObservation>) -> u64 {
    let OutlierPolicy::MadTrim { factor } = policy else {
        return 0;
    };
    let baselines: Vec<u64> = observations
        .iter()
        .map(|sample| sample.baseline_ns)
        .collect();
    let median = percentile(&baselines, 0.5);
    let deviations: Vec<f64> = baselines
        .iter()
        .map(|baseline| (*baseline as f64 - median).abs())
        .collect();
    let mad = percentile_f64(&deviations, 0.5);
    if mad == 0.0 {
        return 0; // degenerate spread: nothing to trim against
    }
    let cutoff = mad * factor;
    let before = observations.len();
    observations.retain(|sample| (sample.baseline_ns as f64 - median).abs() <= cutoff);
    (before - observations.len()) as u64
}

/// Arithmetic mean of u64 samples.
#[must_use]
#[allow(clippy::cast_precision_loss)] // sample counts/sums as f64; exact below 2^53
pub fn mean(values: &[u64]) -> f64 {
    let sum: f64 = values.iter().map(|value| *value as f64).sum();
    sum / values.len() as f64
}

/// Linear-interpolated percentile of u64 samples.
///
/// `position` always lies in `[0, len - 1]`, so the truncating/sign casts
/// of the index bounds are exact.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn percentile(values: &[u64], p: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    if sorted.len() == 1 {
        return sorted[0] as f64;
    }
    let position = (sorted.len() - 1) as f64 * p.clamp(0.0, 1.0);
    let lower_index = position.floor() as usize;
    let upper_index = position.ceil() as usize;
    let fraction = position - lower_index as f64;
    let lower = sorted[lower_index] as f64;
    let upper = sorted[upper_index] as f64;
    lower + (upper - lower) * fraction
}

/// Linear-interpolated percentile of f64 ratios.
///
/// `position` always lies in `[0, len - 1]`, so the truncating/sign casts
/// of the index bounds are exact.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn percentile_f64(values: &[f64], p: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable_by(f64::total_cmp);
    if sorted.len() == 1 {
        return sorted[0];
    }
    let position = (sorted.len() - 1) as f64 * p.clamp(0.0, 1.0);
    let lower_index = position.floor() as usize;
    let upper_index = position.ceil() as usize;
    let fraction = position - lower_index as f64;
    let lower = sorted[lower_index];
    let upper = sorted[upper_index];
    lower + (upper - lower) * fraction
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protocol() -> StatisticalProtocol {
        StatisticalProtocol {
            warmup_repetitions: 2,
            repetitions: 10,
            min_repetitions: 4,
            paired: true,
            seed: 7,
            outlier: OutlierPolicy::KeepAll,
            retain_raw: true,
            randomize_order: false,
        }
    }

    #[test]
    fn paired_result_is_reproducible() {
        let observations: Vec<PairedObservation> = (1..=10)
            .map(|i| PairedObservation {
                baseline_ns: 100 * i,
                candidate_ns: 50 * i,
            })
            .collect();
        let protocol = protocol();
        let first = evaluate_paired(&protocol, observations.clone()).unwrap();
        let second = evaluate_paired(&protocol, observations).unwrap();
        assert_eq!(first, second);
        assert!((first.median_ratio - 0.5).abs() < 1e-12);
        assert_eq!(first.wins, 8);
        assert_eq!(first.losses, 0);
        assert_eq!(first.samples_used, 8);
        assert_eq!(first.outliers_removed, 0);
        assert!(first.raw_retained);
    }

    #[test]
    fn warmup_is_discarded() {
        let mut observations = Vec::new();
        observations.push(PairedObservation {
            baseline_ns: 100,
            candidate_ns: 90,
        });
        observations.push(PairedObservation {
            baseline_ns: 100,
            candidate_ns: 90,
        });
        for i in 1..=8 {
            observations.push(PairedObservation {
                baseline_ns: 200 * i,
                candidate_ns: 100 * i,
            });
        }
        let result = evaluate_paired(&protocol(), observations).unwrap();
        assert_eq!(result.samples_used, 8);
        assert!((result.median_ratio - 0.5).abs() < 1e-12);
    }

    #[test]
    fn mad_trim_removes_outliers_deterministically() {
        let mut observations: Vec<PairedObservation> = (1..=10)
            .map(|i| PairedObservation {
                baseline_ns: 100 * i,
                candidate_ns: 50 * i,
            })
            .collect();
        observations.push(PairedObservation {
            baseline_ns: 100_000,
            candidate_ns: 50_000,
        });
        let mut protocol = protocol();
        protocol.outlier = OutlierPolicy::MadTrim { factor: 3.0 };
        let result = evaluate_paired(&protocol, observations).unwrap();
        // 11 observations - 2 warmup = 9 post-warmup; the 100000ns outlier
        // exceeds 3 * MAD and is culled; 8 remain.
        assert_eq!(result.outliers_removed, 1);
        assert_eq!(result.samples_used, 8);
        assert!((result.median_ratio - 0.5).abs() < 1e-12);
    }

    #[test]
    fn randomization_is_seed_deterministic() {
        let observations: Vec<PairedObservation> = (1..=8)
            .map(|i| PairedObservation {
                baseline_ns: 100 * i,
                candidate_ns: 50 * i,
            })
            .collect();
        let mut protocol = protocol();
        protocol.randomize_order = true;
        protocol.min_repetitions = 4;
        let first = evaluate_paired(&protocol, observations.clone()).unwrap();
        let second = evaluate_paired(&protocol, observations).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn structural_and_evidence_errors_are_stable() {
        let bad = StatisticalProtocol {
            warmup_repetitions: 5,
            repetitions: 5,
            ..protocol()
        };
        let error = bad.validate().unwrap_err();
        assert_eq!(error.code, "E-HOST-003");

        let few: Vec<PairedObservation> = (1..=2)
            .map(|i| PairedObservation {
                baseline_ns: 100 * i,
                candidate_ns: 50 * i,
            })
            .collect();
        let error = evaluate_paired(&protocol(), few).unwrap_err();
        assert_eq!(error.code, "E-HOST-006");

        let zero = vec![PairedObservation {
            baseline_ns: 0,
            candidate_ns: 50,
        }];
        let error = evaluate_paired(&protocol(), zero).unwrap_err();
        assert_eq!(error.code, "E-HOST-008");

        let unpaired = StatisticalProtocol {
            paired: false,
            ..protocol()
        };
        let error = evaluate_paired(&unpaired, vec![]).unwrap_err();
        assert_eq!(error.code, "E-HOST-008");
    }

    #[test]
    fn percentiles_are_linear_interpolation() {
        assert!((percentile(&[10, 20], 0.0) - 10.0).abs() < 1e-12);
        assert!((percentile(&[10, 20], 1.0) - 20.0).abs() < 1e-12);
        assert!((percentile(&[10, 20], 0.5) - 15.0).abs() < 1e-12);
        assert!((percentile(&[10, 20], 0.25) - 12.5).abs() < 1e-12);
        assert!((percentile(&[42], 0.99) - 42.0).abs() < 1e-12);
    }
}
