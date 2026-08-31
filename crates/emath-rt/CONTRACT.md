# CONTRACT.md

## Purpose and layer

`emath-rt` is the pre-compiled math kernel library: vector/matrix/tensor
arithmetic, stencil convolution, modular/finite-field arithmetic, and the
higher-order drivers (fold, Simpson quadrature, numerical limits). It is
shared two ways:

1. The interpreter (`emath-exec-ir`) calls these functions directly for
   op evaluation.
2. The Rust backend (`emath-rust-backend`) embeds [`SOURCE`] (the verbatim
   `body.rs` text) into every generated crate as `mod emath_rt { ... }`,
   and generated expressions call `emath_rt::<name>(...)`.

Layer: foundation (std-only, no other emath crates).

## Public types and semantics

- `SOURCE: &'static str`; the embeddable kernel body; byte-stable per
  version.
- All functions in `body.rs` are re-exported at the crate root: `vec_*`,
  `mat_*`, `tensor_*`, `stencil_1d`, `stencil_2d`, `factorial`,
  `factorial_checked`, `mod_inv`, `mod_inv_checked`, `poly_eval_mod`,
  `poly_eval_mod_checked`, `rs_encode`, `rs_encode_checked`,
  `hamming_distance`, `hamming_distance_checked`, `cmp_i64_f64`,
  `eq_i64_f64`, `fold_add`, `fold_mul`,
  `fold_add_i64`, `fold_mul_i64`, `fold_all`, `fold_any`,
  `fold_add_checked`, `fold_mul_checked`, `fold_all_checked`,
  `fold_any_checked`, `simpson`,
  `sample_limit`, `einsum_checked`, `einsum_as_scalar`, `einsum_as_vector`,
  `einsum_as_matrix`, `einsum_as_tensor`, `EinsumError`, `EinsumIn`,
  `EdgePolicy`, `Tensor`, `IndexError`, `SliceAxis`, `whole_index`,
  `vec_index_checked`, `mat_index_checked`, `tensor_index_checked`,
  `tensor_slice_checked`, `tensor_slice_as_scalar`,
  `tensor_slice_as_vector`, `tensor_slice_as_matrix`,
  `tensor_slice_as_tensor`, `complex_sqrt`, `complex_ln`, `complex_exp`,
  `trapezoid_sum`.
- `stencil_1d` / `stencil_2d` take `EdgePolicy` **by value** (moved from a
  borrowed `&EdgePolicy`); `stencil_1d` honors Clamp / Neumann / OneSided
  / Dirichlet; `stencil_2d` refuses `Dirichlet`.
  `OneSided` linearly extrapolates a ghost cell (`u[-1] = 2u[0] − u[1]`)
  so a central first-difference is exact on linear fields at the edge.
- Every panicking kernel (e.g. `factorial`, `mod_inv`, `einsum_as_*`) has
  a `_checked` twin (`Result<_, &'static str>` or `EinsumError`); the
  panicking form delegates to it. Index/slice kernels are checked-only
  (`IndexError`): there is no panicking `[]` wrapper.

## Invariants

- std-only and dependency-free; `#![forbid(unsafe_code)]`.
- `body.rs` contains no crate-level attributes, no `crate::` paths, and no
  external imports, so the text can be pasted inside a `mod` block in any
  generated crate.
- Every kernel is deterministic: same inputs, same IEEE-754 operation
  order, same output, bit-for-bit. `vec_norm([])` is `+0.0` (empty sum of
  squares), not `-0.0` from `f64` empty `Iterator::sum`.
- Kernel semantics mirror the historical inline generated-code semantics
  exactly (zip truncation, boundary mirroring formulas) except index and
  slice: those are typed `IndexError` faults (negative / non-whole / OOB),
  never panicking `[]`. Rank-3+ values are `Tensor { shape, data }` so a
  flat buffer does not lose rank. Where the interpreter historically
  diverged from codegen (e.g. `sample_limit` direction thresholds), the
  runtime follows the codegen behavior; the interpreter keeps its own
  tested path.

## Error model

- `stencil_2d` panics on `Dirichlet` (unreachable from generated code;
  the backend refuses 2D Dirichlet at codegen time; the interpreter
  pre-checks and returns a typed fault instead of calling).
- `mod_inv` / `poly_eval_mod` / `rs_encode` / `hamming_distance` /
  `factorial` / `einsum_as_*` panic on invalid inputs; the interpreter
  calls `einsum_checked` / `*_checked` / `vec_index_checked` /
  `tensor_slice_checked` and returns typed `EvalFault`s, so panics are
  unreachable from interpreted evaluation of admitted programs.
  Dimension-mismatched einsum is `EinsumError::Arithmetic`. Index/slice
  OOB is `IndexError::OutOfBounds` (mapped to `EvalFault::IndexOutOfBounds`
  in interp; generated evaluate methods return `Result<_, String>`).
- `simpson` asserts on a non-positive or odd panel count `n`.
- `sample_limit` panics when no sample in the geometric progression is
  finite (`sample_limit produced no finite values`).
- `mat_mul_mat` panics on ragged operands (direct `a[i][k]` / `b[k][j]`
  indexing, mirroring the historical inline semantics).
- All remaining kernels are total on arbitrary input.

## Determinism class

Bit-exact deterministic (fixed-point-free IEEE-754 binary64 operations in
fixed order).

## Cancellation behavior

None: all kernels are synchronous pure functions. Higher-order drivers run
to completion; budgets are enforced by callers.

## Unsafe boundary

None (`#![forbid(unsafe_code)]`).

## Feature flags

None.

## Conformance tests

`tests/emath-rt` (workspace member): hand-computed stencil results per edge
policy, modular inverse property, RS codeword round trip, Simpson values,
fold accumulations, sample-limit convergence, embedding smoke (SOURCE
contains no crate-level attribute).

## No-claim boundaries

- Not a general linear-algebra library: no solvers/eigen decompositions.
- `mat_mul_mat` is semantically naive O(n³) with direct indexing.
- Complex arithmetic is not covered (handled in the interpreter).
