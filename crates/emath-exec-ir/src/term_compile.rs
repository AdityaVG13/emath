//! Generic reference-bytecode records consumed by semantic images.
//!
//! Authored FeatureID reference programs are loaded as data. This module owns
//! only their domain-neutral record types; it contains no built-in cell list or
//! feature-name compiler dispatch.

use std::fmt;

use crate::EmirProgram;

mod types;

pub use types::*;
