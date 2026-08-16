//! Goal schema (full goal kinds, custom-goal envelope) and capability
//! surface for the intent-compiler lane.
//!
//! The goal-elaboration kernel used by the Phase 1 session moved down to
//! `emath-ir::goal` (`RequestSpec`, `build_goal`) and into `emath-sema`
//! (`elaborate_requests`, its only consumer). This crate hosts the goal
//! schema (`schema`) that validates itself and carries the versioned
//! canonical encoding used by plan identity and future request lanes.

#![forbid(unsafe_code)]

pub mod schema;
pub use schema::{
    budget_token, custom_token, exactness_token, fallback_token, target_token, BudgetConstraint,
    GoalKindSpec, GoalSchema, GoalSchemaProblem,
};
