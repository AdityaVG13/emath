//! Laboratory core: deterministic measurement, the paired statistical
//! protocol, and finite law checking. Std-only and deterministic;
//! wall-clock timing enters only as injected samples.
//!
//! Key modules: `measure` (deterministic harness), `stats` (paired
//! protocol, MAD outliers), `law_check` (finite-world law obligations),
//! and `identity` (subject/oracle separation for paired comparisons).

#![forbid(unsafe_code)]

pub mod error;
pub mod identity;
pub mod law_check;
pub mod measure;
pub mod stats;

pub use error::LabError;
pub use identity::{EngineIdentity, EngineRole};
pub use measure::{Measurement, MeasurementKind, Summary};
pub use stats::{
    OutlierPolicy, PairedObservation, PairedResult, StatisticalProtocol, evaluate_paired,
};

/// Deterministic pseudo-random samples (seed fixed, runs reproducible).
pub struct Sampler {
    state: u64,
}

impl Sampler {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next f64 in `[-1, 1)` via a splitmix64-style mix; the high 53
    /// bits are f64-exact.
    #[allow(clippy::cast_precision_loss)]
    pub fn next_unit(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        ((z >> 11) as f64 / (1_u64 << 53) as f64) * 2.0 - 1.0
    }
}

/// Meaning authority for a candidate result.
///
/// Lattice (never escalate via ranking): Structural < Tested < Certified < Proved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Authority {
    /// Structural preservation only.
    Structural,
    /// Checked against examples or properties.
    Tested,
    /// Bounded or certified by an admitted checker.
    Certified,
    /// Formally proved in a declared system.
    Proved,
}

impl Authority {
    /// Stable wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Structural => "structural",
            Self::Tested => "tested",
            Self::Certified => "certified",
            Self::Proved => "proved",
        }
    }
}
