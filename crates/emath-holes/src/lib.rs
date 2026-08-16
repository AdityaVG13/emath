#![forbid(unsafe_code)]

//! Meaning holes and finite synthesis.
//!
//! An underconstrained construct is a meaning hole. This crate delivers
//! a deterministic hole graph (stable ids, kinds, states, dependencies,
//! budget bookkeeping) and finite synthesis of operator tables:
//!
//! - deterministic finite-carrier enumeration over `carrier^(n²)`;
//! - law validation by the independent finite-law checker
//!   (emath-law-check), so only tables satisfying every declared law are
//!   synthesized;
//! - continuations: solving a hole produces a new immutable problem
//!   state (the next graph) plus a receipt; failed proposals never
//!   mutate the authoritative graph.
//!
//! The synthesis exit is covered: operator tables satisfying declared finite
//! laws are synthesized, and a seeded impossible law set (two distinct
//! identities for the same operator) is rejected exhaustively.

pub mod graph;
pub mod synth;

pub use graph::{HoleGraph, HoleState, MeaningHole, MeaningHoleKind};
pub use synth::{
    solve_op_hole, synthesize_tables, Continuation, SolveReceipt, SynthesisError, SynthesisLaw,
    SynthesisRun,
};
