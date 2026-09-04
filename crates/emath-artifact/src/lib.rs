//! Artifact emission: deterministic JSON writers for the four durable
//! schemas (`emath.artifact`, `emath.source-map`,
//! `emath.resolution-plan`, `emath.evidence-bundle`), staging and
//! atomic publish with content-identity verification, and an independent checker that
//! never calls generator internals.

#![forbid(unsafe_code)]

use emath_core::{ContentId, SchemaId, bootstrap_content_id, content_id_of_str, fnv1a64_bytes};
use emath_ir::{
    ClaimVerdict, EvidenceClaim, EvidenceLevel, PlanNodeDef, PlanOperation, ResolutionPlan,
    TargetProfile,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

mod authority;
mod emit;
mod identity;
mod json;
mod jsonval;
mod manifest_io;
mod model;
mod staging;

pub use authority::*;
pub use emit::*;
pub use identity::*;
pub use json::*;
pub use jsonval::*;
pub use manifest_io::*;
pub use model::*;
pub use staging::*;

use emit::*;
use identity::*;
use json::*;
use jsonval::*;
use manifest_io::*;
use model::*;
use staging::*;
