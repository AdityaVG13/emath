//! Lab error contract: stable `E-HOST-*` codes.
//!
//! Codes: `003` invalid manifest/protocol, `004` not frozen or baseline
//! == candidate, `005` gate failure, `006` insufficient evidence, `007`
//! metric regression, `008` incomparable, `010` drift, `012` protocol,
//! `013` canary routing, `014` drift band, `015` engine policy, `016`
//! self-comparison refusal. `001`/`002` owned by `emath-rust-ir`.

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
