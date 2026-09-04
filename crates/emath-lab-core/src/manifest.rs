//! Frozen experiment manifest.
//!
//! Freezes campaign identity before measurement: baseline, candidate,
//! generator, partitions (stages A–E), environment, metrics, protection
//! envelope, seed, budget. Self-validates (`E-HOST-003`/`E-HOST-004`),
//! with canonical encoding (`lab:...`) and deterministic canonical JSON.

use crate::error::LabError;
use crate::json::{self, JsonValue};
use crate::stats::StatisticalProtocol;
use emath_core::{ContentId, fnv1a64_bytes};

mod model;
mod parse;
mod fields;

pub use model::*;
pub use parse::*;
pub use json::*;

use model::*;
use parse::*;
use fields::*;
