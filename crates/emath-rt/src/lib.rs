//! Provider-free numeric and storage kernels for generated crates and the
//! interpreter. Language images select these kernels through generic bindings;
//! this crate does not decide feature identity, admission, or applicability.
//!
//! The implementation lives in [`body`] (single source of truth) and is
//! re-exported here. The backend embeds [`SOURCE`] verbatim into every
//! generated crate as `mod emath_rt { ... }`, so generated artifacts stay
//! self-contained with no external dependencies. See `body.rs` for the
//! embedding rules.

#![forbid(unsafe_code)]

mod body;

mod probability;

pub use body::*;
pub use probability::ProbError as DistributionKernelError;

/// Unit-interval uniforms in [0, 1) from an explicit seed and stream path.
/// Same seed and path replay bit-identically. This is the remaining native
/// counter-stream leaf; distribution transforms above it
/// are authored language definitions.
pub fn unit_interval_stream(
    seed: f64,
    draws: f64,
    stream_path: &str,
) -> Result<Vec<f64>, DistributionKernelError> {
    probability::unit_interval_stream(seed, draws, stream_path)
}

/// The verbatim kernel source (`body.rs`), embedded into every generated
/// crate as `mod emath_rt { ... }`. Deterministic per emath-rt version.
pub const SOURCE: &str = concat!(
    include_str!("body/vecmat.rs"),
    include_str!("body/einsum.rs"),
    include_str!("body/bigmod.rs"),
    include_str!("body/numeric.rs"),
    include_str!("body/graphs.rs"),
    include_str!("body/poly.rs"),
    include_str!("body/control.rs"),
    include_str!("body/exact.rs"),
    "\npub mod exact_int {\n",
    include_str!("body/exact_int.rs"),
    "\n}\npub use exact_int::{ExactError, ExactInt, exact_int_hamming, exact_int_poly_eval, exact_int_prod, exact_int_prod_from, exact_int_sum, exact_int_sum_from, exact_int_weighted_prod, exact_ratio};\n",
    "\npub mod special {\n",
    include_str!("../../emath-core/src/special.rs"),
    "\n}\n"
);
