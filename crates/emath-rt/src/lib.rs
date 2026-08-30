//! Pre-compiled math kernels for emath-generated crates and the emath
//! interpreter.
//!
//! The implementation lives in [`body`] (single source of truth) and is
//! re-exported here. The backend embeds [`SOURCE`] verbatim into every
//! generated crate as `mod emath_rt { ... }`, so generated artifacts stay
//! self-contained with no external dependencies. See `body.rs` for the
//! embedding rules.

#![forbid(unsafe_code)]

mod body;

pub mod category;
pub mod control;
pub mod dynamics;
pub mod graph;
pub mod linalg;
pub mod optimization;
pub mod pde;
pub mod polynomial;
pub mod rat;
pub mod probability;
pub mod sequence;
pub mod stochastic;

pub use body::*;

/// The verbatim kernel source (`body.rs`), embedded into every generated
/// crate as `mod emath_rt { ... }`. Deterministic per emath-rt version.
pub const SOURCE: &str = concat!(
    include_str!("body.rs"),
    "\npub mod special {\n",
    include_str!("../../emath-core/src/special.rs"),
    "\n}\n"
);
