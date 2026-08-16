//!: candidate generation loop.
//!
//! Human/AI/evolutionary proposers hand candidates through the
//! compiler, evidence gates and lab into a Pareto archive. The proposer
//! never promotes: promotion is exclusively the policy engine's job
//! (a candidate whose gate is closed is refused with `E-HOST-005`).

use crate::error::LabError;
use crate::manifest::ArtifactRef;

/// A measured candidate on one metric vector (lower is better on every
/// metric; direction normalization happens before the archive).
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// Sealed artifact.
    pub artifact: ArtifactRef,
    /// `metric id -> value`, lower is better.
    pub metrics: Vec<(String, f64)>,
}

/// True when `left` dominates `right`: no worse on every shared metric
/// and strictly better on at least one. Candidates without shared
/// metrics never dominate each other.
#[must_use]
pub fn dominates(left: &Candidate, right: &Candidate) -> bool {
    let left_by_id: Vec<(&String, f64)> = left
        .metrics
        .iter()
        .map(|(id, value)| (id, *value))
        .collect();
    let right_by_id: Vec<(&String, f64)> = right
        .metrics
        .iter()
        .map(|(id, value)| (id, *value))
        .collect();
    let mut strictly_better = false;
    for (id, left_value) in &left_by_id {
        let Some((_, right_value)) = right_by_id.iter().find(|(right_id, _)| right_id == id) else {
            return false;
        };
        if left_value > right_value {
            return false;
        }
        if left_value < right_value {
            strictly_better = true;
        }
    }
    if left_by_id.is_empty() {
        return false;
    }
    strictly_better
}

/// Pareto archive of non-dominated candidates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParetoArchive {
    front: Vec<Candidate>,
}

impl ParetoArchive {
    /// Adds a gated, measured candidate; returns whether it entered the
    /// (possibly shrunk) front.
    pub fn add(&mut self, candidate: Candidate) -> bool {
        if self
            .front
            .iter()
            .any(|existing| dominates(existing, &candidate))
        {
            return false;
        }
        self.front
            .retain(|existing| !dominates(&candidate, existing));
        self.front.push(candidate);
        true
    }

    /// Non-dominated candidates, in insertion order.
    #[must_use]
    pub fn front(&self) -> &[Candidate] {
        &self.front
    }

    /// Whether the archive holds any candidate.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.front.is_empty()
    }
}

/// One step of the candidate loop.
#[derive(Clone, Debug, Default)]
pub struct CandidateLoop {
    archive: ParetoArchive,
}

impl CandidateLoop {
    /// A fresh loop with an empty archive.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Gate-then-archive step: the proposed candidate is refused with
    /// `E-HOST-005` unless its evidence gate passed. Returns whether the
    /// candidate entered the Pareto front.
    pub fn propose(&mut self, candidate: Candidate, gate_passed: bool) -> Result<bool, LabError> {
        if !gate_passed {
            return Err(LabError::new(
                "E-HOST-005",
                format!(
                    "candidate {} refused by the evidence gate",
                    candidate.artifact.content_id.0
                ),
            ));
        }
        Ok(self.archive.add(candidate))
    }

    /// The current Pareto front.
    #[must_use]
    pub fn archive(&self) -> &ParetoArchive {
        &self.archive
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, latency: f64, size: f64) -> Candidate {
        Candidate {
            artifact: ArtifactRef {
                package: "score".into(),
                content_id: emath_core::ContentId(id.into()),
                profile: "release".into(),
            },
            metrics: vec![("latency".into(), latency), ("size".into(), size)],
        }
    }

    #[test]
    fn domination_is_partial_order() {
        let fast_small = candidate("a", 10.0, 100.0);
        let slow_big = candidate("b", 20.0, 200.0);
        assert!(dominates(&fast_small, &slow_big));
        assert!(!dominates(&slow_big, &fast_small));
        // (10,100) dominates (10,200): equal latency, strictly smaller
        // size. (10,200) does not dominate back (100 <= 200 is false).
        let fast_big = candidate("c", 10.0, 200.0);
        assert!(dominates(&fast_small, &fast_big));
        assert!(!dominates(&fast_big, &fast_small));
    }

    #[test]
    fn archive_keeps_only_the_pareto_front() {
        let mut archive = ParetoArchive::default();
        assert!(archive.add(candidate("a", 10.0, 100.0)));
        assert!(archive.add(candidate("b", 5.0, 120.0)));
        assert!(!archive.add(candidate("c", 12.0, 150.0)));
        assert_eq!(archive.front().len(), 2);
        // A new front candidate evicts dominated ones.
        assert!(archive.add(candidate("d", 4.0, 90.0)));
        assert_eq!(archive.front().len(), 1);
        assert_eq!(
            archive.front()[0].artifact.content_id,
            emath_core::ContentId("d".into())
        );
    }

    #[test]
    fn proposer_cannot_bypass_the_gate() {
        let mut loop_ = CandidateLoop::new();
        let error = loop_.propose(candidate("x", 1.0, 1.0), false).unwrap_err();
        assert_eq!(error.code, "E-HOST-005");
        assert!(loop_.archive().is_empty());
        assert!(loop_.propose(candidate("x", 1.0, 1.0), true).unwrap());
    }
}
