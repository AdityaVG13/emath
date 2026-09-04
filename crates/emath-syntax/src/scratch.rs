//! Official scratch expansion for progressive exactness L0–L2.
//!
//! Bare expressions, guided relationships, intent verbs, and named-declaration
//! shorthand lower to the same declaration IR as contracted components.
//! Inspect the expansion with `emath expand`.

use crate::exactness::ExactnessStatus;
use emath_core::{Diagnostics, FileId, Pedagogy, Span};

mod intent;
mod lower;
mod render;
mod text;
mod types;

pub(super) use intent::*;
pub use lower::*;
pub(super) use render::*;
pub(super) use text::*;
pub use types::*;

const SYNTH_DECL: &str = "Scratch";
const SYNTH_RESULT: &str = "result";

const SECTION_HEADS: &[&str] = &[
    "about",
    "algebraic",
    "compile",
    "constraints",
    "constructors",
    "definitions",
    "equation",
    "equations",
    "events",
    "evidence",
    "exports",
    "goals",
    "host",
    "inputs",
    "invariant",
    "outputs",
    "state",
    "tests",
    "transitions",
];

const SOLVE_CANDIDATES: &str = "Real, Complex, modular, symbolic, numeric";

const BUILTINS: &[&str] = &[
    "abs",
    "and",
    "at",
    "atan2",
    "Bool",
    "ceil",
    "Complex",
    "cos",
    "derivative",
    "else",
    "ensure",
    "example",
    "exists",
    "exp",
    "false",
    "Float64",
    "floor",
    "for",
    "forall",
    "Hole",
    "if",
    "in",
    "Int",
    "integral",
    "is_finite",
    "let",
    "ln",
    "log",
    "match",
    "max",
    "min",
    "Nat",
    "not",
    "on",
    "or",
    "over",
    "pi",
    "plot",
    "pow",
    "product",
    "Real",
    "require",
    "return",
    "self",
    "sin",
    "solve",
    "sqrt",
    "sum",
    "tan",
    "tanh",
    "then",
    "this",
    "to",
    "true",
    "while",
    "with",
    "wrt",
    "m",
    "km",
    "s",
    "kg",
    "g",
];
