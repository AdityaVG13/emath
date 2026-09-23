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

use intent::{IntentVerb, refuse, rewrite_l2, record_hole, constrain_last_hole, lower_intent, attach_find_continuation, finalize_holes, emit_hole_comments};
pub use lower::*;
use render::{free_names, is_builtin, render_from_header, render_function, split_assignment, is_ident};
use text::{split_keyword_tail, split_equation, goal_target, split_top_level, TopPiece, split_declaration_text, is_section_head, is_comment, classify_line, span_of_source, LineKind, header_args, call_position_names, declaration_name, first_content_line, line_offsets, span_bytes, is_item_header, is_content_line, is_unindented, literal_class, first_word, skip_word};
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
