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
- `body.rs` kernels are re-exported at the crate root. The permitted machine
  layer includes storage, checked indexing, and primitive numeric representation
  operations. Mathematical methods such as quadrature and decomposition are
  still present, but they are migration debt, not an unavoidable substrate.
  Direct interpreter or generated-code callers do not justify retaining a
  mathematical method in Rust. Migrate the method and its callers together.
- `rat.rs` and `stochastic.rs` were unlinked and later deleted (no
  production adapter used them); the same orphan cleanup, user-authorized,
  removed `category.rs`, `dynamics.rs`, `linalg.rs`, `pde.rs`, and
  `polynomial.rs` (zero consumers: the only symbols any crate or test
  calls on `emath_rt` are `body::*` kernels and `unit_interval_stream`).
- Neutral `KernelId` names do not establish the language/compiler boundary.
  Inspect the implementation: what remains of mathematical method Rust
  lives in `body` (quadrature, stencils, decomposition, graphs, control)
  and is migration debt, not an unavoidable substrate.
- `body`'s mathematical kernels are linked (glob-re-exported and embedded
  via `SOURCE`). Their methods must move into executable language
  definitions while preserving current behavior. See the ownership rule in
  the root `AGENTS.md` and bead `emath-nwmm6`. The probability wrapper
  keeps only the unit-interval counter-stream leaf; family sampling and
  densities are authored language definitions.
- `stencil_1d` / `stencil_2d` take `EdgePolicy` **by value** (moved from a
  borrowed `&EdgePolicy`); `stencil_1d` honors Clamp / Neumann / OneSided
  / Dirichlet; `stencil_2d` refuses `Dirichlet`.
  `OneSided` linearly extrapolates a ghost cell (`u[-1] = 2u[0] − u[1]`)
  so a central first-difference is exact on linear fields at the edge.
- Every panicking kernel (`einsum_as_*`, `stencil_2d`, `mat_mul_mat`)
  has a `_checked` twin (`Result<_, &'static str>` or `EinsumError`);
  the panicking form delegates to it or is refused ahead at codegen
  time. Index/slice kernels are checked-only (`IndexError`): there is
  no panicking `[]` wrapper. The i64 numeric wrappers
  (`factorial`/`mod_inv`/`pow_mod`/`sqrt_mod`/`poly_eval_mod`/
  `rs_encode`/`hamming_distance`) are gone; their `_checked` twins are
  the only surface (nothing emitted or interpreted ever called the
  panicking forms).

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
- `mod_inv_checked` / `poly_eval_mod_checked` / `rs_encode_checked` /
  `hamming_distance_checked` / `factorial_checked` and the rest of the
  numeric body are checked-only; the interpreter calls
  `einsum_checked` / `*_checked` / `vec_index_checked` /
  `tensor_slice_checked` and returns typed `EvalFault`s, so panics are
  unreachable from interpreted evaluation of admitted programs.
  Dimension-mismatched einsum is `EinsumError::Arithmetic`. Index/slice
  OOB is `IndexError::OutOfBounds` (mapped to `EvalFault::IndexOutOfBounds`
  in interp; generated evaluate methods return `Result<_, String>`).
- `simpson` refuses typed on a non-positive or odd panel count `n`.
- `sample_limit` refuses typed when no sample in the geometric
  progression is finite (`sample_limit produced no finite values`).
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

## Authored exact methods

`body/exact.rs` supplies only the bounded rational carrier and checked
scalar arithmetic. Polynomial brackets, exact linear families, scalar
parameter cases, series enclosures, and quantization execute from authored
Language Image programs. The runtime no longer exports their former
record types or algorithms. Generated Rust emits authored record layouts
and program bodies through the same generic backend.

## Code carrier (emath-npky7)

`body/code.rs` (embedded as `pub mod code` in both the crate and
`SOURCE`) is the artifact-side Code value: a quoted unary program
compiled once into a closure factory. It is a representation
carrier, not a mathematical method: `open(free, make)` pairs the
open-constant names with the compiled factory;
`substitute` binds one constant by partial application (by name - the
value splices at the name's slot; an absent reference is a no-op,
tree-substitution parity); `evaluate` is the guarded executor (open
code refuses `unbound_code` naming the remaining constants in binding
order; closed code yields the specialized closure). No tree is
carried and no interpreter runs. The carrier is generic
(`Code<V: Clone + 'static>`): the backend instantiates it over the
template's declared scalar domain - `Code<ExactRatio>` for Rat,
`Code<i64>` for Int, `Code<bool>` for Bool - so one implementation
serves every scalar program family. Conformance:
`tests/emath-rt/tests/code_carrier.rs` with splice-position and
guard-removal mutation probes; the Int and Bool instantiations are
exercised end-to-end by the program-space fixture and the export
acceptance.

## Expression-template union (emath-expression-quotes-324y0)

The same file carries the expression-template lane: a quoted
EXPRESSION with free names (`quote(x + c)`, no parameter, no
closure). The carrier of each free name is a substitute-time fact,
so the body computes over `CodeValue` - the dynamic union
`Int(i64) | Rat(ExactRatio) | Bool(bool)` - and every scalar
operation renders as a call to a dynamic kernel (`code_add`,
`code_sub`, `code_mul`, `code_div`, `code_neg`, `code_eq`,
`code_cmp`, `code_as_bool`, `code_not`). The kernels implement the
constructor VM's EXACT carrier rules (normative source:
`constructor_layer/ops.rs` - `binary`, `neg`, `eq_values`,
`cmp_numeric`; the short-circuit And/Or arm in `engine_step/core.rs`):

- Int x Int keeps Int for `+ - *` (`2 + 3` is Int 5, never Rat 5/1);
  the collapse lives in the both-Int fast paths only (a probe-proven
  dead collapse branch on the rational path was deleted).
- Any Rational operand locks the Rat carrier forever, even when the
  result is integer-valued (`1/4 - 1/4` is Rat 0/1).
- Division NEVER collapses (`4 / 2` is Rat 2/1); a zero denominator
  refuses `division_by_zero`.
- Equality compares VALUES by cross-multiplication (`2 == 2/1` is
  true); Bool equality is structural; mixed kinds are never equal.
- Checked projections (`project_i64`/`project_ratio`/`project_bool`)
  mirror the engine's `type_admits` at typed boundaries: a
  Rat-declared output widens Int exactly (`5` becomes `5/1`), an
  Int-declared output refuses a Rational by name (matching the
  engine's `output ... does not have the declared type` message), Bool
  admits Bool only.

`ExprCode` (`open_expr`/`substitute_expr`/`evaluate_expr`) obeys the
same laws as the function carrier: by-name binding, absent-reference
no-op, `unbound_code` naming the remaining names in binding order.
Carrier-width law: the VM's Int is arbitrary-precision `ExactInt`;
this union's Int is checked i64 (the existing cross-lane width
distinction) - parity holds in the i64-shared domain and beyond-i64
values refuse here exactly as the Int emission lane always has.
Still no tree, no interpreter: the body is the compiled arithmetic
the backend emitted, as kernel calls over the union. Conformance:
the `probe_union_kernels` probe in `code_carrier.rs` (22 checks) with
fast-path, rat-lock, and truncation mutation probes all killing;
end-to-end parity via the expression-quote fixture and the export
acceptance case.

No-claim boundaries: no structured values (sequences/records) in the
union; no Float64 lane (a float reaching a union op refuses named);
no Text carrier; the rt comparison kernels are pinned at the unit
level - the authored surface's comparison-valued templates compute
through them but the export parity case rides the arithmetic lane.

## Shared code tree - view/make substrate (emath-shared-tree-view-make-bp8nu)

`body/code_tree.rs` (embedded as `pub mod code_tree` in both the
crate and `SOURCE`) is the artifact-side STRUCTURAL Code
representation. Representation decision: embedding emath-core's
`Expr` via `include!` was rejected (the artifact embed law is a
std-only, zero-dependency `emath_rt`), so `CodeTree` is a distilled
std-only tree (Literal/Path/Call/Binary over the 17 scalar ops with
the VM's `scalar_op_name` spellings/Unary/Tuple/If) and the VM keeps
its own CValue quote machinery - parity rests on shared-algorithm
ports pinned at the unit level, not on rewiring the VM to this type.

The node family `NodeValue` (Scalar/Record/Sequence/Tuple/Code)
reproduces the VM's walk records arm for arm:

- view (quote.view): `view_tree`/`view_quoted` mirror `view_of` -
  Call/Binary/Unary view as Call records with op-tag callees, Code
  args, and minted Fragment children; Tuple views as Sequence;
  If views as Branch. Minting is per-run deterministic: one
  thread-local `MintState`, `reset_mint()` at the entry prologue,
  tokens `#scope.{id}` climbing in walk order. `check_scope` is the
  forged-scope law: a Fragment package whose Scope witness carries
  an id this run did not mint refuses
  `invalid_code_construction: forged Scope witness`.
- make (quote.make): `rebuild_node`/`rebuild_call`/`make_quoted`
  mirror `rebuild_expr` - refusal spellings verbatim (`Call missing
  callee`, `node \`{kind}\` is not yet emitted in artifact trees`
  for kinds outside the emitted subset). Made code carries a
  dependency snapshot; `verify_deps` refuses
  `stale_dependency: quoted dependency \`{n}\` changed since capture`.
- node laws: `node_eq` mirrors the VM's `eq_values` (scalar VALUE
  equality - `2 == 2/1`; mixed kinds never equal), `node_field`
  refuses typed (`type: record has no field \`{f}\``), `node_index`
  refuses `invalid_index: sequence index out of range`.

`ExprCode` is dual-representational: `{free, tree,
make: Option<factory>, deps}` - one lowering pass emits the compiled
union factory AND the distilled tree; `evaluate_expr` verifies deps,
then runs the factory when present, else the tree evaluator.
`evaluate_tree` computes scalar trees only; calls, globals, and
structured bodies refuse named (`tree: ... is not emitted in the
scalar tree evaluator`) - the compile-time resolution boundary, no
interpreter claim. Conformance:
`tests/emath-rt/tests/code_tree.rs` (8 cases, 35 checks) with
token-format, forged-scope-drop, and node-kind-tag mutation probes
all killing; `ModuleTable`/`mint_scope`/`check_scope` parity with
the VM's `mint_scope`/`check_fragment_scope`/`dependency_snapshot`
is pinned by the same probe.

