//! Generic fit-goal data and execution seams (04 §5.3). Domain-free: this module knows parameters, observables,
//! residual methods, optimizer methods, weights, and content-addressed
//! provenance — never a concrete model. The PK two-compartment model is
//! a runnable `.emath` fixture (`language/examples/science/
//! pk-two-compartment-fit.emath`); execution here goes through
//! capability/method/provider seams:
//!
//! - residuals through [`ResidualMethod`] + [`FitModel`];
//! - optimization through [`OptimizerMethod`];
//! - structural identifiability through [`IdentifiabilityProvider`].
//!
//! Without a structural-identifiability provider the disposition is an
//! honest typed [`UnresolvedReason`]: no authority is claimed and
//! escalation refuses ([`AuthorityEscalation::Refused`]) instead of
//! silently claiming a fit. Fitting is estimation with uncertainty,
//! provenance, and identifiability — never bare optimization.

use std::collections::BTreeMap;

use emath_ir::goal::GoalPayload;
use emath_ir::provenance::{DistributionKind, Measured, Provenance};
use emath_term::SymbolId;

mod model;
mod solve;
mod measured;
mod linalg;

pub use model::*;
pub use solve::*;
pub use measured::*;
pub use linalg::*;

use model::*;
use solve::*;
use measured::*;
use linalg::*;
