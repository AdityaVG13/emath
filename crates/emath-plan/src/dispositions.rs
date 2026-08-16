//!: total compilation dispositions.
//!
//! Every planning outcome maps to exactly one artifact disposition, driven
//! by the goal's fallback policy. Nothing silently falls back.

use emath_ir::FallbackPolicy;

/// Total compilation disposition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArtifactDisposition {
    /// Native artifact, no provider representation conversion.
    Native,
    /// Hybrid artifact (native plus provider path with conversions).
    Hybrid,
    /// Parametric artifact (missing provider lifted to a trait).
    Parametric,
    /// Exploration artifact (candidate exploration preserved).
    Exploration,
    /// Continuation artifact (planner budget exhausted, resumable).
    Continuation,
    /// Diagnostic artifact (no eligible plan, reasons only).
    Diagnostic,
}

impl ArtifactDisposition {
    /// Stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Hybrid => "hybrid",
            Self::Parametric => "parametric",
            Self::Exploration => "exploration",
            Self::Continuation => "continuation",
            Self::Diagnostic => "diagnostic",
        }
    }
}

/// Disposition when no eligible plan exists, per fallback policy.
#[must_use]
pub fn disposition_without_plan(fallback: &FallbackPolicy) -> ArtifactDisposition {
    match fallback {
        FallbackPolicy::NativeOnly | FallbackPolicy::Diagnostic => ArtifactDisposition::Diagnostic,
        FallbackPolicy::Parametric => ArtifactDisposition::Parametric,
        FallbackPolicy::Continuation => ArtifactDisposition::Continuation,
        FallbackPolicy::ExplicitLadder => ArtifactDisposition::Exploration,
    }
}

/// Disposition when the planning budget is exhausted, per fallback policy.
#[must_use]
pub fn disposition_exhausted(fallback: &FallbackPolicy) -> ArtifactDisposition {
    match fallback {
        FallbackPolicy::Continuation => ArtifactDisposition::Continuation,
        FallbackPolicy::Parametric | FallbackPolicy::ExplicitLadder => {
            ArtifactDisposition::Exploration
        }
        FallbackPolicy::NativeOnly | FallbackPolicy::Diagnostic => ArtifactDisposition::Diagnostic,
    }
}

/// Disposition for a selected plan; `Native` when no representation
/// conversions were planned, `Hybrid` otherwise.
#[must_use]
pub fn disposition_for_plan(has_conversions: bool) -> ArtifactDisposition {
    if has_conversions {
        ArtifactDisposition::Hybrid
    } else {
        ArtifactDisposition::Native
    }
}
