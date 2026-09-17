//! ODE simulation and integration machinery: explicit steppers
//! (Euler / RK4 / Cash-Karp RK45), adaptive dt, event location, and
//! causalized implicit-DAE Newton solving.
//!
//! Public entries refuse. The bodies stay on disk (RULE 1) and are not
//! a constructor operation.

/// Native simulate/ODE is not a constructor. CLI and library share this text.
pub const CONSTRUCTOR_SIMULATE_GONE: &str =
    "E-KIND-GONE: `emath simulate` is not a constructor command. `emath model` is not a core kind. Write an ordinary `emath function` (import a stepper such as `numerics.rk4`) and run it with `emath run`.";

fn gone<T>() -> Result<T, String> {
    Err(CONSTRUCTOR_SIMULATE_GONE.into())
}

mod newton;
pub use newton::{authored_implicit_explicit_step, residual_model_step_program};

use crate::interp::Value;
use emath_ir::{Declaration, SemanticPackage};
use std::collections::BTreeMap;
mod api;
mod types;

pub use api::*;
pub use types::*;
