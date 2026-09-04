#![forbid(unsafe_code)]

//! Semantic and joint tuning.
//!
//! Semantic tuning varies World IR components while protecting laws and
//! held-out examples; joint tuning varies the world and implementation
//! together. Promotion needs semantic admission, an evidence threshold, a
//! resource envelope, fallback availability, and a deterministic receipt.

pub mod campaign;
pub mod frontier;

use emath_term::{Signature, SymbolId};
use emath_world_ir::{Fixity, MeaningHoleId, OperatorSemantics, SymbolDef, WorldId, WorldIr};

mod apply;
mod calibrate;
mod codec;
mod model;

pub use apply::*;
pub use calibrate::*;
pub use codec::*;
pub use model::*;
