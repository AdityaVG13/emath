//! Deterministic JSON for host documents: the std-only streaming writer
//! (re-exported from `emath-core`) and its parse-back reader. The reader
//! accepts the writer's grammar and refuses anything else with a typed
//! [`JsonError`].

#![forbid(unsafe_code)]

mod json;
mod jsonval;

pub use json::*;
pub use jsonval::*;

/// A document that does not match the writer's JSON grammar or the shape a
/// caller asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError(pub String);

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "malformed JSON: {}", self.0)
    }
}

impl std::error::Error for JsonError {}
