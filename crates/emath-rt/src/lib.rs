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
mod control;
mod dynamics;
mod graph;
mod linalg;
mod optimization;
mod pde;
mod polynomial;
mod probability;
mod sequence;

pub use body::*;
pub use category::{CategoryError, category_check, diagram_commutative};
pub use control::{
    poles_stable as checked_sign_table, state_space_dc_gain as checked_linear_projection,
    transfer_eval as checked_polynomial_ratio,
};
pub use graph::{
    GraphError as DenseCarrierError, bellman_ford as relaxation_shortest_path,
    bfs_order as breadth_order, degree_out as row_nonzero_counts,
    dijkstra as nonnegative_shortest_path, laplacian as degree_minus_carrier,
    reachability as reachable_mask, sparse_from_triplets as coordinate_stream_to_dense,
    sparse_triplets as dense_to_coordinate_stream, symmetrize as transpose_average,
};
pub use linalg::{
    LinalgError as DenseLinearError, cg_solve as convergent_dense_solve,
    jacobi_eigen as symmetric_decomposition, svd_factors_packed as rectangular_factors,
    svd_singular_values as rectangular_spectrum,
};
pub use optimization::{
    lp_minimize as constrained_linear_minimize, pareto_front as nondominated_mask,
};
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
    let family = distribution_family(kind)?;
    probability::prob_sample_in_stream(family, parameters, seed, draws, stream_path)
}

/// Evaluate a validated density selected by its capsule-supplied kernel code.
pub fn distribution_density(
    kind: u8,
    parameters: &[f64],
    point: f64,
) -> Result<f64, DistributionKernelError> {
    probability::prob_density(distribution_family(kind)?, parameters, point)
}

fn distribution_family(kind: u8) -> Result<probability::Family, DistributionKernelError> {
    match kind {
        0 => Ok(probability::Family::Normal),
        1 => Ok(probability::Family::Uniform),
        2 => Ok(probability::Family::Bernoulli),
        _ => Err(DistributionKernelError::InvalidParameter),
    }
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
    "\npub mod special {\n",
    include_str!("../../emath-core/src/special.rs"),
    "\n}\n"
);
