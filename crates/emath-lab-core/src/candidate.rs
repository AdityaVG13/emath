//! Candidate generation loop.
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
