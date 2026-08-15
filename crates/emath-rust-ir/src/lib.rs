//! Structured Rust IR: a target AST with deterministic rendering, identifier
//! hygiene and byte-range anchors for source maps. No string-concatenated
//! generation outside this renderer.

#![forbid(unsafe_code)]

pub mod ast;
pub mod render;
