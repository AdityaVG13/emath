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

mod category;
mod dynamics;
mod linalg;
mod pde;
mod polynomial;
mod probability;
mod sequence;

pub use body::*;
pub use category::{CategoryError, category_check, diagram_commutative};
pub use linalg::LinalgError as DenseLinearError;
pub use polynomial::{PolyError, poly_eval as checked_poly_eval, poly_mul as checked_poly_mul};
pub use probability::ProbError as DistributionKernelError;

/// Sample a validated distribution selected by its capsule-supplied kernel code.
pub fn sample_distribution_in_stream(
    kind: u8,
    parameters: &[f64],
    seed: f64,
    draws: f64,
    stream_path: &str,
) -> Result<Vec<f64>, DistributionKernelError> {
    probability::prob_sample_in_stream(kind, parameters, seed, draws, stream_path)
}

/// Unit-interval uniforms in [0, 1) from an explicit seed and stream path.
pub fn unit_interval_stream(
    seed: f64,
    draws: f64,
    stream_path: &str,
) -> Result<Vec<f64>, DistributionKernelError> {
    probability::unit_interval_stream(seed, draws, stream_path)
}

/// Evaluate a validated density selected by its capsule-supplied kernel code.
pub fn distribution_density(
    kind: u8,
    parameters: &[f64],
    point: f64,
) -> Result<f64, DistributionKernelError> {
    probability::prob_density(kind, parameters, point)
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
    "\npub mod special {\n",
    include_str!("../../emath-core/src/special.rs"),
    "\n}\n"
);
