//! Executable Mathematics IR (EMIR): typed, target-independent ops.
//!
//! lowers the strict-Float64 subset to a linear op list per output
//! definition. Operations record domain obligations; the generator emits
//! them as assumptions (`assumptions.json`), never silently erasing them.

#![forbid(unsafe_code)]

pub mod image;
pub mod interp;
pub mod language_image;
pub mod language_tables;
pub mod module_index;
pub mod native_kernel;
pub mod progress;
pub mod optimize;
pub mod reference_views;
pub mod exact_int;
pub mod constructor_layer;
pub mod constructor_emir;
mod tree_distill;
pub mod term_compile;


use emath_core::Span;
pub use emath_ir::CellClass;
mod format;
mod op;
mod ops_impl;
mod program;
mod types;

pub use op::*;
pub use program::*;
pub use types::*;

use format::write_nested_programs;

