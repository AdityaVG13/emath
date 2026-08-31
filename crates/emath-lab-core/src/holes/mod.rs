#![forbid(unsafe_code)]

//! Meaning holes and finite synthesis.
//!
//! An underconstrained construct is a meaning hole: deterministic hole
//! graph (stable ids, states, dependencies, budget) plus finite
//! synthesis of operator tables — deterministic `carrier^(n²)`
//! enumeration validated by the independent law checker. Continuations
//! yield an immutable next graph + receipt; failed proposals never
//! mutate the authoritative graph.

pub mod graph;
pub mod synth;

pub use graph::{HoleGraph, HoleState, MeaningHole, MeaningHoleKind};
pub use synth::{
    Continuation, SolveReceipt, SynthesisError, SynthesisLaw, SynthesisRun, check_laws,
    impossible_identity_laws, satisfiable_or_table_laws, solve_op_hole, synthesize_tables,
};
