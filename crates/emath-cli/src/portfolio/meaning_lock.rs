//! Project-local meaning lock: persist a chosen world fingerprint so
//! later runs are single-world and user-locked.
//!
//! Locks are local-side (per-user, per-project). They are not baked into
//! shared source. The locked identity is the same world fingerprint used
//! by G7 [`crate::portfolio::WorldCandidate::world_fingerprint`] (`WorldIr::identity`).

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use emath_world_ir::fnv1a64;

use crate::portfolio::interpretation::{
    InterpretationPolicy, LedgerEntry, MetricAxis, PortfolioError, PortfolioReceipt, evaluate,
};
use crate::portfolio::record::WorldCandidate;

mod json;
mod lock;
mod model;

pub use json::*;
pub use lock::*;
pub use model::*;
