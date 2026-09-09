//! ODE simulation and integration machinery: explicit steppers
//! (Euler / RK4 / Cash-Karp RK45), adaptive dt, event location, and
//! causalized implicit-DAE Newton solving.

mod newton;
pub use newton::{authored_implicit_explicit_step, causal_newton, residual_model_step_program};

use crate::EmirExprRef;
use crate::interp::Value;
use emath_ir::{Declaration, EventDecl, SemanticPackage, TransitionDecl};
use std::collections::{BTreeMap, BTreeSet};
mod api;
mod events;
mod implicit;
mod inner;
mod rk45;
mod types;

pub use api::*;
pub use types::*;

use events::*;
use implicit::*;
use inner::*;
use rk45::*;
