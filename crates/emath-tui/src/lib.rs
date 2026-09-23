//! The loop surface crate: the headless research-loop host core (B2)
//! and, later, the `emath loop` line-REPL (B3) and the dashboard over
//! the user's design spec (B4). The host core is lane-agnostic bookkeeping
//! over the VM engine; the native lane (B5) re-implements the same
//! scratch contract over the artifact ABI.

pub mod dream;
pub mod experiment;
pub mod export;
pub mod host;
pub mod repl;
