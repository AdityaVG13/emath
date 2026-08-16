//! Source files, line maps and human-readable diagnostic rendering.
//!
//! The source-file types moved down to `emath-core` (Tier 0: they depend
//! only on core identity/diagnostic types). This crate remains the stable
//! public import surface: every `emath_source::SourceFile` /
//! `emath_source::SourceStore` path resolves to the same type as
//! `emath_core`, so downstream type annotations keep compiling unchanged.

#![forbid(unsafe_code)]

pub use emath_core::{SourceFile, SourceStore};
