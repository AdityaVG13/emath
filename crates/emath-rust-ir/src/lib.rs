//! Structured Rust IR: a target AST with deterministic rendering, identifier
//! hygiene and byte-range anchors for source maps. No string-concatenated
//! generation outside this renderer.

#![forbid(unsafe_code)]

pub mod ast;
pub mod host;
pub mod profiles;
pub mod render;

pub use host::{
    HostBindError, HostBinding, HostMethod, HostTraitSpec, append_to_module, check_version,
    fallback_binding, generate_binding,
};
pub use profiles::{CrateProfile, ProfileProblem, parse_profile};
pub use render::{
    Anchor, FileSet, RenderResult, coverage_gaps, render_file_set, render_file_set_partitioned,
    render_generics, render_module,
};
