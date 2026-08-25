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

- `SOURCE: &'static str` — the embeddable kernel body; byte-stable per
  version.
- All functions in `body.rs` are re-exported at the crate root: `vec_*`,
  `mat_*`, `tensor_*`, `stencil_1d`, `stencil_2d`, `factorial`, `mod_inv`,
  `poly_eval_mod`, `rs_encode`, `hamming_distance`, `fold_add`, `fold_mul`,
  `fold_all`, `fold_any`, `simpson`, `sample_limit`, `EdgePolicy`.

## Invariants

- std-only and dependency-free; `#![forbid(unsafe_code)]`.
- `body.rs` contains no crate-level attributes, no `crate::` paths, and no
  external imports, so the text can be pasted inside a `mod` block in any
  generated crate.
- Every kernel is deterministic: same inputs, same IEEE-754 operation
  order, same output, bit-for-bit.
- Kernel semantics mirror the historical inline generated-code semantics
  exactly (zip truncation, direct indexing on ragged matrices, boundary
  mirroring formulas). Where the interpreter historically diverged from
  codegen (e.g. `sample_limit` direction thresholds), the runtime follows
  the codegen behavior; the interpreter keeps its own tested path.

## Error model

- `stencil_2d` panics on `Dirichlet` (unreachable from generated code —
  the backend refuses 2D Dirichlet at codegen time; the interpreter
  pre-checks and returns a typed fault instead of calling).
- `mod_inv` / `poly_eval_mod` / `rs_encode` / `hamming_distance` /
  `factorial` panic on invalid inputs; the interpreter performs its own
  validation first and returns typed `EvalFault`s, so panics are
  unreachable from interpreted evaluation of admitted programs.
- All other kernels are total on arbitrary input.

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
