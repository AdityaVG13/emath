//! Lab error contract.
//!
//! Stable codes under the `E-HOST-*` prefix (host/lab area):
//! - `E-HOST-003` invalid experiment manifest or protocol configuration;
//! - `E-HOST-004` experiment not frozen or baseline equals candidate;
//! - `E-HOST-005` correctness/quality gate failure;
//! - `E-HOST-006` insufficient evidence (protocol minimum not met);
//! - `E-HOST-007` metric regression (latency/throughput/memory/energy);
//! - `E-HOST-008` incomparable experiment (mismatched protocol or inputs);
//! - `E-HOST-009` raw samples not retained as declared;
//! - `E-HOST-010` drift detected (input/quality/latency/memory/health);
//! - `E-HOST-011` decision receipt cannot be recomputed independently;
//! - `E-HOST-012` invalid statistical protocol configuration;
//! - `E-HOST-013` invalid canary routing configuration;
//! - `E-HOST-014` invalid drift band tolerance;
//! - `E-HOST-015` invalid engine policy;
//! - `E-HOST-016` refuse self-comparison: subject and oracle of a
//!   comparison must be distinct engine identities.
//!
//! `E-HOST-001`/`E-HOST-002` are owned by the host-binding layer
//! (`emath-rust-ir`).

use std::fmt;

/// Typed lab failure with a stable code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabError {
    /// Stable code (`E-HOST-003`..`E-HOST-011`).
    pub code: &'static str,
    /// Message.
    pub message: String,
}

impl LabError {
    /// Constructs a lab error with a stable code.
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for LabError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for LabError {}
