# CONTRACT.md

## Purpose and layer

`emath-rt` is a provider-free storage and numeric kernel implementation layer.
It supplies deterministic functions selected by generic `KernelId` adapters; it
does not decide feature identity, labels, admission, applicability, result
authority, or exactness claims. Those decisions belong to authored capsules and
the generated language image. The implementation is shared two ways:

1. The interpreter (`emath-exec-ir`) calls these functions directly for
   op evaluation.
2. The Rust backend (`emath-rust-backend`) embeds [`SOURCE`] (the verbatim
   `body.rs` text) into every generated crate as `mod emath_rt { ... }`,
   and generated expressions call `emath_rt::<name>(...)`.

Layer: foundation (std-only, no other emath crates).

## Public types and semantics

- `SOURCE: &'static str`; the embeddable kernel body; byte-stable per
  version.
- `body.rs` kernels are re-exported at the crate root. The unavoidable generic
  substrate comprises shape-preserving vector/matrix/tensor storage and
  arithmetic, checked indexing/slicing/einsum, deterministic scalar and integer
  arithmetic (including bounded big integers), folds, quadrature/limit drivers,
  stencil application, and numeric decomposition/solve routines. These remain
  public because interpreter adapters and generated Rust call them directly.
- `rat.rs` and `stochastic.rs` are no longer linked: no production adapter uses
  them. The source files remain present and unreferenced because deletion was not
  authorized.
- Native `KernelId` adapters call only neutral root names for dense carriers,
  decompositions, checked polynomial/linear operations, optimization, sampling,
  and densities. Root sampling takes an image-supplied numeric kernel code rather
  than publishing a distribution-family enum.
- `category`, `control`, `dynamics`, `graph`, `linalg`, `optimization`, `pde`,
  `polynomial`, `probability`, and `sequence` remain public solely because
  production interpreter/backend callers outside this task's edit scope still
  use those paths. They are unavoidable public-module residue, not semantic
  authority, and must become private when those callers use the root kernel ABI.
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
- Kernels branch only on numeric/storage inputs and explicit algorithm
  parameters. They do not inspect `FeatureId`, `KernelId`, capsule labels,
  worlds, evidence, or result-authority metadata.
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

- Kernel availability is not language admission, and a returned number carries
  no independent claim of applicability, proof, evidence, or semantic identity.
- This is not a general linear-algebra API even though bounded decomposition and
  solve kernels exist for generic adapters.
- `mat_mul_mat` is semantically naive O(n³) with direct indexing.
- Complex helpers provide numeric operations only; they do not choose a complex
  world or authorize complex-valued language semantics.
