//! Tree module — the syntax tree is owned by emath-core so that
//! semantic admission (emath-sema) can depend on core/ir alone. This
//! module is a stable forwarding re-export; downstream imports such as
//! `use emath_syntax::tree::{...}` keep working verbatim.

pub use emath_core::tree::*;
