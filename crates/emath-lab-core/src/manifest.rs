//! Frozen experiment manifest.
//!
//! Freezes campaign identity before measurement: baseline, candidate,
//! generator, partitions (stages A–E), environment, metrics, protection
//! envelope, seed, budget. Self-validates (`E-HOST-003`/`E-HOST-004`),
//! with canonical encoding (`lab:...`) and deterministic canonical JSON.

use crate::error::LabError;
use crate::json;
use emath_core::{ContentId, fnv1a64_bytes};

mod model;
mod parse;
mod fields;

pub use model::*;
pub use json::*;

use fields::{problem, kill_condition_token, artifact_json, json_count, kill_condition_json, expect_object, owned_string_field, bool_field, artifact_from_json, field, u64_field, array_field, object_field, string_field, parse_partition_kind, expect_u64, parse_direction, number_field, optional_number_field, kill_condition_from_json, parse_kill_action, parse_fallback_action, expect_string};
