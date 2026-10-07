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

## Mutable buffer carrier

`body/buffer.rs` supplies `Buffer` and `CellValue`, embedded in generated
artifacts. A cloned buffer handle shares its mutable storage. A read snapshots
the cell value; a nested buffer handle retains its shared identity. Cells hold
heterogeneous cloneable native values, with immutable sequence/record metadata
for checked projections. Typed use checks the actual carrier, including
Int/Rat/Bool projections from quoted scalar values. Float and Text retain their
native carriers rather than entering the quoted scalar union.

Creation accepts nonnegative exact machine indices and caps storage at
4,000,000 cells. Invalid sizes and indices refuse `invalid_index`; a size
above the cell bound refuses `overflow`. Reads and writes check bounds.
Typed carrier mismatches refuse `type`, and narrowing an integer to i64
remains checked. Operations are synchronous and single-threaded; this is
mutable storage, not a mathematical algorithm or a persistence format.
Buffer equality has no value semantics and is refused by the backend.
Conformance: `emission_conformance_buffer` runs authored sieve, aliasing,
heterogeneous cells, snapshots, eager/lazy writes, and named bound refusals
against the constructor VM.

## Exact integer comparison

Ordering compares Small/Small directly as i128. Wide magnitudes are borrowed;
a mixed Small operand uses four stack limbs, including i128::MIN via
unsigned_abs. Sign order and negative-magnitude reversal are unchanged. No
heap allocation is needed for comparison itself. Native generated code and
constructor VM share this implementation. The numeric_typed regression checks
all pairs around signed and storage boundaries and equivalent Big/Small forms.
`cmp_exact_int_f64` decodes finite binary64 mantissa/exponent bits and compares
with exact integer scaling, without rounding the integer. Non-finite floats
and integer memory-bound failures refuse. Native `numeric_boundaries` pins
mixed equality and ordering on both sides of 2^53 and beyond i128.

`exact_ratio_f64` rounds a ratio once, ties to even, including operands at
the integer limb cap. Scaling uses bounded limb scratch rather than capped
integer products; this does not widen admitted integer storage. The shared
VM/native helper preserves signed underflow and gradual subnormals. Its
`ratio_float` test covers near-cap operand pairs and rounding boundaries.

## Wide integer division

Single-u32-limb divisors use high-to-low word division. Remainder < divisor
ensures that joining it with the next u32 limb fits in u64. Multi-limb divisors
use Knuth's Algorithm D (TAOCP 4.3.1): normalize the divisor's top bit, estimate
each quotient limb from the top two remainder limbs, correct it (at most two
too large), multiply-subtract, add back on the rare negative. Cost is
O(quotient limbs x divisor limbs) with no per-step allocation; the earlier
bit-at-a-time loop cost a vector per quotient bit and dominated exact-rational
work (every Rat normalizes through `gcd`). Signed/Euclidean wrappers, zero
handling, and normalization are unchanged. Conformance:
`tests/emath-rt/tests/numeric_typed.rs` checks `n = q d + r`, `|r| < |d|`,
sign of `r`, including an add-back vector. This shared representation
primitive does not widen native Int type admission.

## Public types and semantics

- `SOURCE: &'static str`; the embeddable kernel body; byte-stable per
  version.
- `body.rs` kernels are re-exported at the crate root. The permitted machine
  layer includes storage, checked indexing, and primitive numeric representation
  operations. Mathematical methods that remain (modular/coding kernels, dense
  linear helpers, stencils) are migration debt, not an unavoidable substrate.
  Direct interpreter or generated-code callers do not justify retaining a
  mathematical method in Rust. Migrate the method and its callers together.
  Quadrature, limit sampling, and the polynomial/series bodies were removed
  2026-09-23 after their authored language twins landed
  (`language/modules/calculus/float64_quadrature.emath`,
  `language/modules/numerics/limits.emath`,
  `language/modules/algebra/float64_polynomials.emath`);
  `body/poly.rs` retains only the unit-interval counter-stream leaf.
- `rat.rs` and `stochastic.rs` were unlinked and later deleted (no
  production adapter used them); the same orphan cleanup, user-authorized,
  removed `category.rs`, `dynamics.rs`, `linalg.rs`, `pde.rs`, and
  `polynomial.rs` (zero consumers: the only symbols any crate or test
  calls on `emath_rt` are `body::*` kernels and `unit_interval_stream`).
- Neutral `KernelId` names do not establish the language/compiler boundary.
  Inspect the implementation: what remains of mathematical method Rust
  lives in `body` (stencils, decomposition, graphs, control)
  and is migration debt, not an unavoidable substrate.
- `body`'s mathematical kernels are linked (glob-re-exported and embedded
  via `SOURCE`). Their methods must move into executable language
  definitions while preserving current behavior. See the ownership rule in
  the root `AGENTS.md`. The probability wrapper
  keeps only the unit-interval counter-stream leaf; family sampling and
  densities are authored language definitions.
- `stencil_1d` / `stencil_2d` take `EdgePolicy` **by value** (moved from a
  borrowed `&EdgePolicy`); `stencil_1d` honors Clamp / Neumann / OneSided
  / Dirichlet; `stencil_2d` refuses `Dirichlet`.
  `OneSided` linearly extrapolates a ghost cell (`u[-1] = 2u[0] − u[1]`)
  so a central first-difference is exact on linear fields at the edge.
- Kernels are checked-only (`Result<_, &'static str>`, `EinsumError`,
  `BigError`); generated code maps each refusal to its typed code.
  There are no panicking twins. Index/slice kernels are checked-only (`IndexError`): there is
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
  diverged from codegen (the former `sample_limit` direction
  thresholds, removed with the function's migration into
  `language/modules/numerics/limits.emath`), the runtime follows the
  codegen behavior; the interpreter keeps its own tested path.

## Error model

- `stencil_2d` panics on `Dirichlet` (unreachable from generated code;
  the backend refuses 2D Dirichlet at codegen time; the interpreter
  pre-checks and returns a typed fault instead of calling).
- `poly_eval_mod_checked` / `rs_encode_checked` /
  `hamming_distance_checked` / `factorial_checked` and the rest of the
  numeric body are checked-only; the interpreter calls
  `einsum_checked` / `*_checked` / `vec_index_checked` /
  `tensor_slice_checked` and returns typed `EvalFault`s, so panics are
  unreachable from interpreted evaluation of admitted programs.
  Dimension-mismatched einsum is `EinsumError::Arithmetic`. Index/slice
  OOB is `IndexError::OutOfBounds` (mapped to `EvalFault::IndexOutOfBounds`
  in interp; generated evaluate methods return `Result<_, String>`).
- `mat_mul_mat` panics on ragged operands (direct `a[i][k]` / `b[k][j]`
  indexing, mirroring the historical inline semantics).
- All remaining kernels are total on arbitrary input.

## Determinism class

Bit-exact deterministic (fixed-point-free IEEE-754 binary64 operations in
fixed order).

## Cancellation behavior

Kernels and storage operations remain synchronous. Constructor artifacts and
dynamic Code evaluators share a cooperative `execution` budget on the current
thread. Callable evaluation enters that scope before executing an expression
factory, as well as on later application. Non-callable expressions therefore
retain their ordinary type refusal without an explicit host budget.
Default entry limits are 50,000,000 native work units and 128 active
execution frames. `with_budget(work, depth, body)` establishes explicit limits
before entry; depth is capped at 512. Nested scopes cannot increase a parent's
remaining allowance, and consumed work remains charged to that parent.

Work covers generated bodies, calls, control iterations and evaluated tree
nodes, not VM instructions or wall-clock time. Exhaustion returns
`E-NATIVE-WORK` or `E-NATIVE-DEPTH` and stays sticky until the run ends, even
if a host callback catches the nested error. Guards release context on return,
refusal and unwinding. Returned callables acquire or inherit a budget when
invoked. Different threads have independent budgets.

Opaque host callbacks and individual synchronous arithmetic/storage kernels
cannot be preempted. This is not a CPU-time, stack-byte or process sandbox.
Conformance: `emission_conformance_native_budget` covers tail/non-tail/mutual
recursion, folds, returned closures, dynamic Code, inherited scopes, swallowed
exhaustion and unwind recovery, with work/depth guard-removal mutations.

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

`body/exact.rs` supplies `ExactRatio(ExactInt, ExactInt)` and checked
scalar arithmetic. Kernel results have a positive, gcd-reduced denominator;
raw host parts must be canonical before arithmetic. Both parts share the
integer memory bound. Arithmetic cancels before multiplying, and ordering
uses continued fractions rather than overflowing cross-products. A zero
denominator or noncanonical operand refuses `E-RAT-001`; exceeding the limb
bound refuses `overflow`. The carrier is `Clone`, not `Copy`.
Polynomial brackets, exact linear families, scalar
parameter cases, series enclosures, and quantization execute from authored
Language Image programs. The runtime no longer exports their former
record types or algorithms. Generated Rust emits authored record layouts
and program bodies through the same generic backend.

## Code carrier

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

## Expression-template union

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
- Equality compares VALUES through exact rational ordering (`2 == 2/1` is
  true); Bool equality is structural; mixed kinds are never equal.
- Checked projections (`project_i64`/`project_ratio`/`project_bool`/
  `project_exact_int`) mirror the engine's `type_admits` at typed
  boundaries: a Rat-declared output widens Int exactly (`5` becomes
  `5/1`), an Int-declared output refuses a Rational by name (matching
  the engine's `output ... does not have the declared type` message),
  Bool admits Bool only, and an ExactInt boundary admits an Int union
  member exactly (`ExactInt::from`) while Rational and Bool refuse by
  name.

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

## Shared code tree - view/make substrate

`body/code_tree.rs` (embedded as `pub mod code_tree` in both the
crate and `SOURCE`) is the artifact-side STRUCTURAL Code
representation. Representation decision: embedding emath-core's
`Expr` via `include!` was rejected (the artifact embed law is a
std-only, zero-dependency `emath_rt`), so `CodeTree` is a distilled
std-only tree (Literal/Path/Call/Binary over the 17 scalar ops with
the VM's `scalar_op_name` spellings/Unary/Tuple/If) and the VM keeps
its own CValue quote machinery - parity rests on shared-algorithm
ports pinned at the unit level, not on rewiring the VM to this type.

The node family `NodeValue` (Scalar/Text/Record/Sequence/Tuple/Code)
reproduces the VM's walk records arm for arm:
- Text remains distinct from schema-name tags. Equality does not turn a tag
  into a string; substitution references accept either representation.
  This does not add Text literals to `CodeValue`.
- `ExprCode::captured_tree` exposes the tree and dependency snapshot without
  exposing its compiled factory. `evaluate_node_expr` (also the
  `ExprCode::evaluate_node` method) checks dependencies and open names, then
  evaluates scalar leaves, tuples, lazy branches, and indexing into the node
  family. Unsupported leaves retain named refusals.

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
  refuses `invalid_index: sequence index out of range`. `_N` on a
  family `Sequence` indexes positionally, exactly as on a `Tuple`:
  emitted code builds tuples as positional sequences (the typed
  lane's tuple law), so a tuple the artifact built reads back the
  same way one the caller built does.
- applied evaluate: `evaluate_callable_expr` closes a code exactly as
  `evaluate_expr` does (stale dependencies, then `unbound_code`) and
  returns an `AppliedCode` callable that binds a function literal's
  nested parameters stage by stage and evaluates the body through
  `evaluate_tree`; each argument meets its binder's `TreeDomain` before
  it binds (the VM's check-then-bind order and scalar admission: `Rat`
  admits Int and Rat, `Int` Int, `Bool` Bool, a sequence domain no
  scalar, `Unknown` everything), refusing `type: closure domain `{d}`
  does not admit the argument `{v}``. A closed non-function code
  evaluates up front and every application refuses `type: value is not
  callable: {v}` (the VM's words).
- Recursive quoted functions execute with lexical captures and a self binding
  installed in each call frame. The captured environment never points back to
  its own closure, avoiding reference-count cycles. Inline function calls and
  curried stages share this callable-value evaluator and the scalar kernels.
  Explicit binder domains remain checked before binding.
  Pure recursive scalar bodies memoize up to 4096 successful argument/result
  pairs per closure instance. Keys preserve Int/Rat/Bool carrier identity.
  Module calls, nested function bodies, higher-order arguments, and faults
  do not enter this cache. Reference-engine cost measurements are unchanged.
- `ExprCode::evaluate_value` keeps partially applied functions in an
  `EvaluatedValue`, rather than projecting them prematurely onto a scalar.
  Its arguments cross as checked node data; sequence binder domains check
  every element. Curried stages retain earlier sequence captures. Sequence
  equality, length, and checked indexing compute in this callable tree lane.
  Function values cannot be serialized as data nodes.
- `CodeTree::Closure` records its binder domain (`TreeDomain`, the
  VM's `DomainShape` in artifact vocabulary; distillation reuses the
  VM's `domain_shape_of_expr`). The view does not expose it (the VM's
  view does not); `rebuild_node` gives a made closure the `Rat` domain,
  as the VM's make does; substitution preserves it; tree equality
  includes it.
- module calls: `ModuleTable::callables` maps a module function name
  to a `ModuleCallable` (`fn(&[CodeValue]) -> Result<CodeValue, String>`,
  the artifact's adapter over the compiled entry); `evaluate_tree`
  takes the table and runs `CodeTree::Call` through it, arguments
  evaluated left to right. A callee bound in the input environment
  refuses `type: value is not callable: {v}` (the VM's words); a call
  the table does not carry keeps `tree: call `{f}` is not emitted in
  the scalar tree evaluator`. `ModuleTable` is `Copy` (all `'static`
  slices), so an applied-evaluate callable captures it by value.
  `node_callables` additionally maps data-carrier entries through checked
  node arguments/results, including sequence parameters and results.
- `node_items` yields a family `Sequence`'s or `Tuple`'s items (the
  call-marshal bridge into a declared sequence parameter); any other
  value refuses `type: expected a sequence value`.
- `ExprCode` equality is tree + stamped dependencies (the factory is
  derived from the tree); `node_eq` on code nodes uses it.

`ExprCode` is dual-representational: `{free, tree,
make: Option<factory>, deps}` - one lowering pass emits the compiled
union factory AND the distilled tree; `evaluate_expr` verifies deps,
then runs the factory when present, else the tree evaluator.
`evaluate_tree` computes scalar leaves and module calls carried by
`ModuleTable::callables`; unsupported calls and global references refuse
by name. `evaluate_node_tree` also preserves tuples, lazy branches,
and bounds-checked tuple indexing. This is a limited tree evaluator,
not an unrestricted interpreter. Conformance:
`tests/emath-rt/tests/code_tree.rs` (8 cases, 35 checks) with
token-format, forged-scope-drop, and node-kind-tag mutation probes
all killing; `ModuleTable`/`mint_scope`/`check_scope` parity with
the VM's `mint_scope`/`check_fragment_scope`/`dependency_snapshot`
is pinned by the same probe.

## Definition table - the quote.body unfold

The same file carries the definition-table lane: `DefinitionTable`
(rows of name, opacity, and - for a transparent callee - the
distilled body tree) and `quote_body_node`/`ExprCode::body`, the
arm-for-arm port of the VM's `quote_body`: a Path-named Code yields
`Opaque { identity, signature: "opaque" }` for an opaque callee,
`Available { fragment }` for a transparent one (the fragment is a
dependency-free, factory-free Code of the body - the VM's
`available_body` law), `Opaque { identity, signature: "unbound" }`
for a name the table does not carry (a record, never a fault), and
`Available` of the code's own tree for any non-Path Code. Function
bodies distill as `Closure { param, body }` (the `FunctionAbs` arm -
the binder shadows in `substitute_tree`, binds in
`collect_free_names`, and the VM view shows the param tag with the
body as a minted fragment), so a transparent callee's authored
function body is `Available` with a computable fragment.
`quote_body_over_node` is the same unfold over a NODE callee: a
named node resolves through the table (`Available`), an op-tag
callee yields the no-identity `Opaque` (the VM's apply law - an
operator has no authored body). No-claim boundary: a transparent row
outside the distilled subset (quotes, structured shapes) carries
`None` and the unfold refuses
`tree: the body of \`{name}\` is not emitted in artifact trees` -
the VM shows the raw expression there; parity holds inside the
distillable subset. The table is emitted by the same lowering that
compiles the entries (one lowering, two projections - neither is
hand-written), built once at first use. Conformance: the
`definition-table-laws` case in `code_tree.rs` (record shapes,
fragment computation, the unbound record, the named boundary
refusal) with a signature-swap mutation probe killing.

## The binder half - quote.open and the call-form quote.bind

`open_node` is the arm-for-arm port of the VM's
`check_fragment_scope` + `open_fragment`: a Fragment package
validates its minted Scope witness FIRST (a forged witness refuses
by the VM's `invalid_code_construction: forged Scope witness`
name), then unwraps - a Code term opens to its own tree with the
compiled factory and the capture snapshot dropped (an opened term
claims nothing), any other node value rebuilds over the distilled
subset, a missing term refuses `Fragment missing term`, and
anything but a Code or a Fragment refuses `quote expects Code or
Fragment`. A plain Code input opens through `open_code` (same
drop law). `node_as_code` is the call-marshal bridge: the node
lane wraps codes as Code nodes (the view's `args`/`children`
fields), so a node value crossing into a Code position unwraps
unchanged (the VM's call arguments ARE codes) or rebuilds - the
VM's `rebuild_expr` law for a value in a code slot.

`bind_code` is the call-form port (the VM's `mint_binds` +
`dependency_snapshot`): nested binder syntax mints fresh tokens,
and over the distilled subset (no binder nodes) the mint walk is
the IDENTITY - the tree and the compiled factory are preserved and
the dependency snapshot re-derives exactly (a stale snapshot
heals; a true one is unchanged). No-claim boundary: the binder
FORM of quote.bind (a fresh-tokened function literal, the VM's
`bind_fresh` with its `#{spelling}.{id}` tokens) is OUTSIDE the
distilled subset - the lowering refuses it by name and this
substrate makes no claim about it; binder hygiene and token
distinctness are VM-lane laws (pinned by the authored
`bind_open.emath` fixture and the exec-ir cutover probes).
Conformance: the `bind-open-laws` case in `code_tree.rs` (the
minted token scheme, the opened term computing, the forged/
termless/ non-package refusals by name, the stale-snapshot heal,
tree preservation under per-run mint resets) with three mutation
probes killing: a perturbed `#scope.{id}` token scheme, a dropped
witness check in the open, and a dropped dependency re-stamp in
the bind.

