#![forbid(unsafe_code)]

//! Deterministic parametric Rust world artifact generation (Semantic Genesis G3).
//!
//! Emits a self-contained, zero-dependency generated crate evaluating a fixed
//! first-order term under free-symbolic, Boolean, and modular-17 worlds, plus
//! a negative-control world whose `⋈`/`⊛` semantics are swapped.

use emath_term::{Signature, Term};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

mod model;
mod render;
mod template;

pub use model::*;
pub use render::*;
pub use template::*;
