//! Newton-step codegen support.
//!
//! The Newton and Gaussian mathematics migrated into authored
//! reference cells; generated residual steps render the shared static
//! step program from
//! `emath_exec_ir::runner::residual_model_step_program` (see
//! `newton_impl`). Nothing in this module emits solver math anymore.

