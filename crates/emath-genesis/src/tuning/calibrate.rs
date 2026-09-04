//! Execution deltas, joint candidates, confidence calibration.

use super::*;

/// An implementation delta: lowering, precision, provider, target, schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionDelta {
    /// Lowering/algorithm label, e.g. `polynomial`.
    pub lowering: String,
    /// Precision label, e.g. `f64`, `f32_bounded`.
    pub precision: String,
    /// Provider label, e.g. `native`, `dew`.
    pub provider: String,
    /// Target label, e.g. `cpu.simd`, `gpu.wgsl`.
    pub target: String,
    /// Schedule label, e.g. `shadow-first`.
    pub schedule: String,
}

impl ExecutionDelta {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "exec-delta:{}/{}:{}:{}:{}",
            self.lowering, self.precision, self.provider, self.target, self.schedule
        )
    }
}

/// A joint tuning candidate: world delta plus execution delta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JointCandidate {
    /// Stable label.
    pub label: String,
    /// World changes.
    pub world: WorldDelta,
    /// Implementation changes.
    pub execution: ExecutionDelta,
    /// Whether the candidate passed the held-out challenge.
    pub held_out_verified: bool,
    /// Evidence units admitting the candidate semantics.
    pub evidence_units: u32,
    /// Candidate content identity (FNV-1a64 over canonical form).
    pub identity: u64,
}

impl JointCandidate {
    /// Builds a candidate with deterministic identity.
    #[must_use]
    pub fn new(
        label: impl Into<String>,
        world: WorldDelta,
        execution: ExecutionDelta,
        held_out_verified: bool,
        evidence_units: u32,
    ) -> Self {
        let label = label.into();
        let candidate = Self {
            label,
            world,
            execution,
            held_out_verified,
            evidence_units,
            identity: 0,
        };
        let identity = emath_world_ir::fnv1a64(candidate.canonical().as_bytes());
        Self {
            identity,
            ..candidate
        }
    }

    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "candidate:{}:{}:{}:held-out={}:evidence={}",
            self.label,
            self.world.canonical(),
            self.execution.canonical(),
            self.held_out_verified,
            self.evidence_units
        )
    }
}

/// Construction vs held-out coverage used to recalibrate meaning confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageSample {
    /// Construction coverage in permille.
    pub construction_permille: u64,
    /// Held-out coverage in permille.
    pub held_out_permille: u64,
    /// Fitted table cell count (description complexity).
    pub table_cells: u64,
    /// Construction example count.
    pub construction_examples: u64,
}

/// Recalibrated meaning confidence after held-out challenge and complexity penalty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalibratedConfidence {
    /// Confidence in permille after subtracting the complexity penalty.
    pub permille: u64,
    /// Penalty for unused table capacity (memorization headroom).
    pub complexity_penalty_permille: u64,
    /// Whether the candidate is admitted as a general meaning.
    pub admitted: bool,
    /// Machine-readable reason: `construction:no-coverage`,
    /// `held-out:memorization`, `complexity-penalty`, or `passed`.
    pub reason: String,
}

/// Minimum held-out coverage (permille) to count as general rather than memorized.
pub const MIN_HELD_OUT_PERMILLE: u64 = 800;

/// Recalibrates meaning confidence against held-out outcomes: memorizing
/// candidates are refused or penalized for unused table capacity.
#[must_use]
pub fn calibrate_confidence(sample: CoverageSample) -> CalibratedConfidence {
    let complexity_penalty_permille = if sample.construction_examples == 0 {
        1000
    } else if sample.table_cells <= sample.construction_examples {
        0
    } else {
        ((sample.table_cells - sample.construction_examples) * 1000) / sample.table_cells
    };
    let held_out_after_penalty = sample
        .held_out_permille
        .saturating_sub(complexity_penalty_permille);
    let (admitted, reason) = if sample.construction_permille == 0 {
        (false, "construction:no-coverage")
    } else if sample.held_out_permille < MIN_HELD_OUT_PERMILLE {
        (false, "held-out:memorization")
    } else if held_out_after_penalty < MIN_HELD_OUT_PERMILLE {
        (false, "complexity-penalty")
    } else {
        (true, "passed")
    };
    CalibratedConfidence {
        permille: held_out_after_penalty.min(sample.construction_permille),
        complexity_penalty_permille,
        admitted,
        reason: reason.to_string(),
    }
}
