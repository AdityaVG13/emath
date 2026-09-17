//! Constructor exact integers share the carrier generated crates embed.
//!
//! The implementation lives in `emath-rt` so `emath build` and the
//! constructor VM cannot drift.

pub use emath_rt::{
    ExactError, ExactInt, exact_int_hamming, exact_int_poly_eval, exact_int_prod,
    exact_int_prod_from, exact_int_sum, exact_int_sum_from, exact_int_weighted_prod,
};
