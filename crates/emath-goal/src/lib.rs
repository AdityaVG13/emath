//! Goal schema and capability surface for the intent-compiler lane.
//!
//! The goal-elaboration kernel lives in `emath-ir::goal` and
//! `emath-sema::elaborate_requests`; this crate hosts the self-
//! validating [`schema`] with the versioned canonical encoding used by
//! plan identity.

#![forbid(unsafe_code)]

pub mod schema;
pub use schema::{
    BudgetConstraint, GoalKindSpec, GoalSchema, GoalSchemaProblem, budget_token, custom_token,
    exactness_token, fallback_token, target_token,
};
