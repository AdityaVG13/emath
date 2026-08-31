//! Integration test crate for emath-adapter-dew.
//!
//! Thin Pattern-1 conformance slice (`emath-conform-harness-thin-lfpg`):
//! this lib target is the shared test-support owner — the verdict
//! vocabulary below is reused by every integration test binary in the
//! crate, so a green suite stays distinguishable from skipped or
//! deferred checks and XFAIL entries stay bound to discrepancy ids.

/// Verdict of one conformance check.
///
/// `ExpectedFailure` is the only status that carries a
/// `discrepancy_id`: it acknowledges a known, documented divergence
/// instead of skipping it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestResult {
    Pass,
    Fail,
    Skipped,
    ExpectedFailure { discrepancy_id: String },
}

impl TestResult {
    /// Whether the check passed outright.
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self, Self::Pass)
    }

    /// Whether the check is an acknowledged expected failure.
    #[must_use]
    pub const fn acknowledged(&self) -> bool {
        matches!(self, Self::ExpectedFailure { .. })
    }

    /// The bound discrepancy id, if this is an expected failure.
    #[must_use]
    pub fn discrepancy_id(&self) -> Option<&str> {
        match self {
            Self::ExpectedFailure { discrepancy_id } => Some(discrepancy_id),
            _ => None,
        }
    }
}

/// Bit-level view of an f64, distinguishing bit patterns `==` cannot see.
#[must_use]
pub fn f64_bits(value: f64) -> u64 {
    value.to_bits()
}

/// Byte-exact equality.
#[must_use]
pub fn bytes_eq(actual: &[u8], expected: &[u8]) -> bool {
    actual == expected
}

/// Canonical multi-line text: trim each line, drop blanks, rejoin with '\n'.
#[must_use]
pub fn canonical_lines(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Tolerance bucket for a scalar comparison: Exact, Loose, or OutOfRange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToleranceClass {
    Exact,
    Loose,
    OutOfRange,
}

/// Classify `actual` vs `expected`: Exact within `tight`, Loose within `loose`, else OutOfRange.
#[must_use]
pub fn classify_tolerance(actual: f64, expected: f64, tight: f64, loose: f64) -> ToleranceClass {
    let delta = (actual - expected).abs();
    if delta <= tight {
        ToleranceClass::Exact
    } else if delta <= loose {
        ToleranceClass::Loose
    } else {
        ToleranceClass::OutOfRange
    }
}
