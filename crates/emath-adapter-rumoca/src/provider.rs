//! DAE plan and simulation providers.
//!
//! Causalization and simulation are provider outputs, not universal SIR
//! meaning. Both run through the runtime Outcome/Budget/Continuation
//! contracts: only `Resolved` carries admitted value authority; exhaustion
//! and failure are typed. All numerics are deterministic f64 and the trace
//! canonical form is byte-identical across runs.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use emath_core::{ContentId, SchemaId, fnv1a64_bytes};
use emath_provider_api::runtime::{
    Budget, ContinuationHandle, EvidenceHandle, Outcome, UnresolvedReason,
};

use crate::lower::{DaePlan, LowerError};
use crate::structural::{EqExpr, StructuralModel};

mod artifact;
mod exec;
mod sim;

pub use artifact::*;
pub use exec::*;
pub use sim::*;
