//! Shared test library for every emath test package.
//!
//! Failure-first. One [`Probe`] runs many functions and many files, and
//! reports every broken contract in a single panic. Do not write a `#[test]`
//! per assertion. Do not snapshot the current compiler. Demand the intended
//! math; if today's code is short, the probe stays red.

#![forbid(unsafe_code)]

mod corpus;
mod pipeline;
mod probe;
mod table;

pub use corpus::{demand_language_gaps, demand_workspace_corpora};
pub use pipeline::{boot, error_codes, Source};
pub use probe::{Failure, Probe};
pub use table::{
    check_all, check_all_close, expect_ok, workspace_file, workspace_path, Case, Close,
    CloseCase,
};
