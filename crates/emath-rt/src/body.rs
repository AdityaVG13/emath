//! Pre-compiled math kernels, embedded verbatim into generated crates as
//! `mod emath_rt { ... }`. The kernel body lives in `body/` parts, included
//! here in order so this module (and the embedded `SOURCE` in `lib.rs`,
//! which concatenates the same parts) stays byte-identical to the original
//! single-file layout. Keep every part std-only (no external crates, no
//! `crate::` paths, no crate attributes) and deterministic: same inputs,
//! same IEEE-754 operation order, bit-for-bit same output.

include!("body/vecmat.rs");
include!("body/einsum.rs");
include!("body/bigmod.rs");
include!("body/numeric.rs");
include!("body/graphs.rs");
include!("body/poly.rs");
include!("body/control.rs");
include!("body/exact.rs");
pub mod exact_int {
    include!("body/exact_int.rs");
}
pub use exact_int::{
    ExactError, ExactInt, exact_int_hamming, exact_int_poly_eval, exact_int_prod,
    exact_int_prod_from, exact_int_sum, exact_int_sum_from, exact_int_weighted_prod, exact_ratio,
};
pub mod code {
    include!("body/code.rs");
}
pub mod code_tree {
    include!("body/code_tree.rs");
}
