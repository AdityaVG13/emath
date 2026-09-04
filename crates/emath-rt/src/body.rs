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
