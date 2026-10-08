# CONTRACT.md

## Purpose and layer

Rust target backend, not a source admission authority. The current constructor
crate path consumes a semantically checked `PreparedModule`, lowers its retained
declarations and emits named entries, runtime, manifest and any reference-engine
sources needed by explicit engine-backed operations. It is shared by CLI build,
the source build library, WASM generation and native loop export. A loading-only
preparation is not evidence of semantic admission; callers must enforce the
session's diagnostics before emission.

Mathematical meaning must arrive through executable contracts, never
domain-named emitter branches. Direct constructor artifacts are std-only; engine-backed artifacts
vendor first-party engine source and pin, but do not vendor, third-party
dependencies. Offline compilation requires those packages in the local cache.
Artifact roots forbid unsafe code; dependencies have their own boundaries.
Artifacts using runtime carriers expose `emath_rt` to their consumers, either
as the embedded module or as a re-export of the single vendored runtime crate.
Both the named-entry and standalone-program emitters establish execution
frames, including recursive calls; hosts need not install a frame to call them.
Conformance: `numeric_boundaries`, `export_native`'s engine-backed scratch parity.
Engine-backed artifacts retain root, search-root and dependency keys under
one lexical absolute-path policy, including relative source-build inputs.
They do not reread checked source bytes or follow changed symlinks during
packaging. Conformance: the source-build `edition` integration test.

## Public types and semantics

- `BackendError`: typed backend failure (variant list below).

## Native numeric boundaries

Code/node carrier support is pinned by the expanded
`emission_conformance_engine_worlds` fixture and the complete
`emission_census`: 141 modules, zero non-runnable lanes.
Every module artifact compiles offline. This is a compilation census,
not a claim that every user workflow executes within every native carrier.
Full workflow probes pass deterministic neural training, learning, and
exploration (including engine-authored work measurements). The complete formula
probe rediscovers Euler, Brouncker, Apéry and Catalan fractions and checks
their authored witnesses with exact rational and integer state.
The complete conjecture probe passes:
three rounds, kept-law discrimination, and curried two-sequence checks.
These are named workflow claims, not a guarantee for every user program.

## Native mutable buffers

Generic buffer creation, writes, reads, and length emit shared `Buffer`
handles and heterogeneous `CellValue` snapshots. The authored sieve is
translated through these operations, with no sieve-specific Rust or engine
fallback. Writes stay sequenced even when their unit result is unused.
Typed calls, results, fields, and indices project cells checked; record/sequence
metadata supports structural projection. Nested buffer handles remain shared.
Exact integer sizes and indices reach the checked machine-index boundary
without first narrowing to i64.

Float/Text cells retain their ordinary native carriers. Quoted scalar cells
project under the existing Int/Rat/Bool law; this does not add Float/Text to
`CodeValue`. Buffer comparisons refuse `buffer_equality_refused`.
Dynamic cell comparisons retain the existing scalar-union limits; typed
Float/Text comparisons project checked. Other comparison carriers refuse
during emission rather than assuming their Rust types implement value equality.
`emission_conformance_buffer` runs the authored algorithm and carrier/bounds
rows against the constructor VM. Native buffers do not cross the existing
engine bridge or acquire native checkpoint serialization through this change.

## Native Code and numeric carriers

Function-valued boundaries adapt compatible carrier representations rather
than casting incompatible Rust closure types. Arguments cross contravariantly;
results cross covariantly, including vectors and nested curried functions.
The adapter clones the callable handle once, keeps non-copy arguments borrowed
where no conversion is needed, and retains checked narrowing at fixed slots.
Converted closure arguments remain owned `Rc` handles, not borrowed data
carriers. The numeric-boundary fixture exercises a higher-order local binder
receiving an externally supplied machine-width callable.
Native authored record sequence slots retain exact integer elements, including
the argument and result sequences of callable fields. Local binder inference
retains incoming exact integers through generic bindings and captured callables.
Fixed scalar slots still narrow checked. The generic-record conformance lane
exercises wide transforms, capture, storage, callable results, and fixed-slot
refusals against the VM.
Quoted scalar-call adapters also accept ExactInt entry arguments and results:
arguments lift from the fixed scalar union, and results project back with
the existing `E-INT-002` bound check. Rebuilding a quoted tree does not remove
an otherwise callable small integer entry. This does not widen CodeValue's
i64 integer carrier.

- Code, nested Code records/sequences, and shared arbitrary-precision integers
  cross the existing reference-engine bridge. Cost and WorldAnswer records use
  the node family so done/value and fault variants retain their actual fields.
- Locally emitted closures and recursive accumulators retain exact integers.
  Viewed schema names use nodes, separately from Text. Carrier inference follows call and
  capture edges; mutual-cycle argument carriers settle before entry emission.
  Externally supplied closure ABIs are not rewritten.
- Generic signature inference represents Code and the scalar union explicitly.
  Expected-result bindings remain available to body binders and empty seeds.
  Dynamic scalar/sequence branches project checked onto typed sibling arms.
- Non-scalar unary templates retain a distilled tree instead of forcing a
  scalar factory. Evaluated Code can return a declared scalar callable or a
  structured node value; unsupported tree leaves still refuse by name.
- A caller constructing callbacks for engine work measurement executes that
  authored computation in the engine. It is not a native step-count estimate
  and does not marshal opaque Rust closures.

Recursive-call arguments and results, closure-call arguments, and authored record
fields share numeric representation conversion, including sequence elements and
sequence index operands. A fixed
`i64` slot checks an `ExactInt` with `to_i64` and returns `E-INT-002` outside
that lane. Int-to-Rat retains both exact integer parts without an i128
crossing. Authored integer fields and local mathematical binders retain
`ExactInt`, including scalar accumulators initialized with small literals.
Inferred-wide recursive and sibling frames and results stay wide;
there is no blanket narrowing of the arbitrary-precision VM carrier.
Mixed Int/Rat comparisons lift integers exactly and preserve operand order.
Mixed ExactInt/Float64 comparisons decode the finite float exactly, without
rounding the integer, including beyond binary64 and i128 integer precision.
Machine control counts accept exact integers through checked i64 projection;
out-of-range counts refuse `E-SCALAR-CONVERT` rather than wrapping.

The same boundary laws govern the kind inference, so a register's kind always
predicts the rendered Rust type:

- A self-recursive call's register kind mirrors the render's crossing decision:
  when a numeric boundary exists between the body's instantiated carrier and
  the authored output, the render crosses to the declared lane and the register
  carries the declared kind; when no boundary exists (a generic instantiation
  such as `tabulate_at`'s `sequence(Rat)` at an `Int -> sequence(Rat)`
  closure), the render emits the body's carrier raw and the register carries
  the body kind. A degenerate provisional body (an empty-literal tail) keeps
  the declared carrier.
- A frame input whose actual is an `Int`/`ExactInt` value and whose declared
  parameter is `Rat` widens to the declared ratio carrier (`rat_pow(4, m)`
  with `p: Rat`); the binding crosses
  through the shared numeric boundary. Non-widenable actuals keep the actual
  carrier (generic templates instantiate at the call site's kind).
- Branch arms join before rendering: a rational arm absorbs an integer arm
  (`if k == 0: 0 else: a / b`) with the integer side widened at the render,
  exactly as an exact-int arm absorbs an i64 arm; the VM's Int-to-Rat join is
  the law.
- Mixed exact/float arithmetic (`hr * 1.0f64`, the labeled Float64 tier) meets
  on the float carrier when exactly one operand is `Float64` and the other is
  `Int`/`ExactInt`/`Rat`: the exact operand widens by one rounding to the
  nearest binary64 (`emath_rt::exact_ratio_f64`, the VM's own conversion).
  Pure int/int, rat/rat, and float/float pairs keep their own lanes.
- An exact-integer machine operand over a `Rat`-kinded register (an `Int` datum
  routed through a Rat-declared record field, which projects the layout's ratio
  carrier) narrows value-exactly: a denominator-1 ratio projects to the integer
  it equals (`ExactInt::from(num)`), anything else refuses
  `E-INT-003: exact integer operand is a non-integral rational`. The VM's `Int`
  parameter admits integer values only; the projection admits exactly the
  value-equal ratios.
- A degenerate kind is recursive: a vector whose element kind is itself
  degenerate carries no carrier (a list OF empty literals), so branch joins and
  frame inputs treat it exactly like `Other` - the other arm's concrete kind or
  the declared parameter wins (`assignments`' nested `vec!(vec!())` base arm
  joins to `Vec<Vec<bool>>`).
- A non-copy frame binding carries its carrier in the binding itself
  (`let name: &ty = &expr;`): a degenerate operand (an empty `[]` literal) has
  no self-evident element type to infer, so the declared frame kind names it -
  the same law as the closure-capture lane.
- A recursion-cycle edge (`CallSibling`) renders as a call to the callee's
  emitted entry fn, and the emission worklist settles the entry set to a fixed
  point: every `CallSibling`-reachable function (including IMPORTED cycle
  members, the caller's `use`d tree merged for declared lookups) gets its own
  entry - `probability/sampling`'s imported `sort_loop` from `discrete/order`
  is the pin. Arguments cross through the callee's declared carriers with the
  same frame-input laws (numeric boundaries, closure handle clones, owned
  non-copy carriers), and the register kind carries the callee's declared
  result.
- Cycle ABI inference settles returned carriers as well as argument carriers.
  A wider stored value returned by one member cannot leave its caller's
  register or capture annotated as a machine-integer sequence.
- Entry reuse: an inlined sibling frame names its callee
  (`CallFrame.callee`). Entries emit callees-first (post-order over the
  named-call graph) and register their ABI (parameter carriers, returned
  carrier); a named frame whose `frame_input_kind` carriers and inferred
  result equal the callee's registered ABI renders as `crate::callee(args)?`
  instead of re-inlining the body. Same computation over the same kinds, so
  the site's register kind is unchanged; any disagreement (a call-site
  specialization, a `CodeValue` body the entry projects, a diverging body)
  keeps the inline frame. Named callees outside the main file emit as helper
  entries only to serve these calls (a helper that cannot emit is skipped,
  never a refusal). This removes the multiplicative blowup of nested sibling
  calls: linalg's lib 156k -> 12k lines, autodiff's 302k -> 32k, and the
  117-module census 1.76M -> 0.71M lines with byte-identical behavior on the
  conformance lanes. A tree-lane entry is a public `reset_mint()` wrapper over
  a `emath_body_<name>` body fn; in-crate calls (named frames and
  `CallSibling` edges) target the body fn so a run's mint never restarts
  mid-run (the VM mints once per query).
- Applied evaluate: `CodeEvaluate { applied: Some(Union(n)) }` over a
  bare expression `Code` takes `n` checked node arguments and retains an
  `EvaluatedValue` result, which can be data or a curried function.
  Further `CallValue` stages apply this value without guessing a scalar
  carrier. Typed output, branch, and integer-operation boundaries project
  checked. `Declared(sig)` uses the scalar adapter when possible, otherwise
  a node-data adapter following each declared arrow stage.
- Module callables: when the crate emits `__EMATH_MODULE_TABLE`, every
  emitted entry whose ABI is scalar (Int/Rat/Bool parameters and
  result) also gets an adapter `emath_callable_<name>` in the table's
  `callables`: arguments project checked onto the declared parameters
  (the slot names the parameter), the arity is checked, the result
  wraps into the union, and a tree-lane entry is called through its
  body fn. The rt tree evaluator reaches module code only through these.
  When any lowered program evaluates code (`CodeEvaluate` at any depth),
  every merged-tree function with a declared data ABI - imported ones
  included, and ones no emitted program calls - joins the emission as a
  helper root, so a run-time formula can reach it (a formula handed in
  as a `Code` input may name any of them). Helpers never make the crate
  not runnable; one that cannot emit is skipped and its run-time call
  refuses by name.
- Data-carrier module entries also get node-callable adapters, so quoted
  sequence programs call compiled module entries with checked sequence
  arguments/results. Helper discovery follows these declared data carriers.
- Call-marshal bridge (`node_into`): a node-family value crossing into
  a declared `Code` parameter unwraps/rebuilds through `node_as_code`,
  and into a declared sequence of code (`sequence(Code)`,
  `sequence((Code, Code))`) maps `node_items` through the same bridge,
  recursively - at recursive self calls, cycle-edge sibling calls, and
  entry-reuse calls alike. A union arm beside a typed scalar arm joins onto the typed
  carrier in the kind walk, matching the render's checked projection.
  The module table static emits whenever emitted code references it.
- `RecordField` over a scalar (Int, Float64, Bool, ExactInt, Text) or
  callable carrier refuses by name (`field `f` projected off a <kind>
  carrier`): such a projection is an application the lowering misread,
  and the artifact never claims runnable over Rust that cannot compile.
- Code-node and scalar-union conversion is fallible at every rendering
  boundary, including comparisons, lists, recursive/closure arguments,
  substitutions, and branch arms. A kind outside the native family returns
  `BackendError::UnsupportedType`, never a kind-invariant panic.
  Comparisons name the node or union lane and the unsupported carrier.
  This does not add Text or Float64 to the quoted scalar union: the reference
  VM computes those values, while affected native entries remain unresolved.

Authored lists join Int/ExactInt/Rat representations before construction or cons.
Concatenation borrows each input once, sums lengths without cloning, allocates
one output vector, and copies/converts each element once. Mixed borrowed/owned
ExactInt comparisons and exact arithmetic (`add`/`sub`/`mul`/`cmp`) pass the
right operand by reference without cloning: a borrowed register (the non-copy
load lane, a generic record projection, or a checked sequence index of a
non-copy element - the render's `Option::get` reference) already renders
exactly one reference layer, so consumers add `&` only for owned operand
expressions. Index lanes that `.cloned()`/`.copied()` (an `ExactInt` element,
copy elements) render owned values and take the `&`.
`tests/emath-rust-backend/tests/numeric_boundaries.rs` compiles and executes the
generated fixture, including named overflow refusals and wide-frame preservation.

## Authored sugar and refusal carriers

- Sequence `length` sugar: authored `p.length` over a `Vector`/`Matrix`/`Tensor`
  receiver lowers as a record-field projection whose kind is `Int` and whose
  render is the storage length (`Vec::len`, the matrix row slice, the tensor
  data) - never a Rust field access on a sequence carrier.
- A body whose every path refuses (the `cycle_gate` shape) never produces a
  value: entries emit the DECLARED output carrier as the result type (the
  unit carrier when nothing is declared) and diverge through the refusal
  returns; frames emit the unit carrier. The experimental Rust never type is
  never emitted. `contains_call_self` stops at `CallFrame` edges - a frame
  renders self-contained with its own `__frame_self`, so a nested sibling's
  self-recursion never forces a wrapper (or a unit result type) on the
  enclosing body.
- Indexing a concrete non-sequence carrier (`VectorIndex` whose receiver kind
  is a scalar, record, or closure) refuses by the VM's own name - the
  `vector_of` type confusion, op `vector-index` - as a `return Err` carrier
  whose kind is `Never`; every consumer join absorbs it (a branch arm, an
  arithmetic operand whose other side wins the carrier), and the refusal
  expression coerces into operand positions so the faulting arm still
  compiles. This is the authored `lp_ray(lp_zeros(1), ...)` shape: a flat
  actual into a nested-declared parameter whose double-index arm never runs
  under the authored givens - the emitted code preserves both facts (it
  compiles; the arm refuses if ever taken). `Other`-kinded receivers keep the
  float-index lane's dynamic guard - an uninferred kind may still be a
  sequence at runtime.

## Rendering cost

`value_expr` computes register kinds once per body/input context and shares the
table with SSA, data, control, and carrier rendering. Nested bodies retain their
own contexts. No global type cache, arithmetic reordering, or emitted-byte
change is introduced by this reuse.

## Invariants

- Direct generated crates are std-only. Engine-backed crates carry the explicit
  dependency profile above. Artifact roots use `#![forbid(unsafe_code)]` and
  `#![allow(dead_code)]`; output is deterministic for the same checked snapshot,
  target inputs and embedded engine source.
- Generated crates embed `mod emath_rt { ... }` (the verbatim `emath-rt` kernel source) with an outer `#[allow(dead_code)]` so hosts that strip inner attributes (an `include!` driver pattern) stay warning-free.
- The emitter is exhaustive over the contracted universal `EmirOp`; active backend modules contain no removed domain-op references.
- The emitter never maps a mathematical feature name or legacy domain operation to a runtime function. A semantic operation without an `ApplyCapability` artifact contract fails as `MissingArtifactContract`.
- `ApplyCapability` dispatches only on generic cell class. Unsupported provider and intrinsic/native bindings fail as `UnsupportedBinding`; other applications without an executable artifact body fail as `MissingArtifactContract`. No identity, interpreter fallback, or compatibility shim is emitted.
- Generated manifest emits `edition = "2024"`, sanitized crate name/version; keywords and reserved identifiers are escaped (`type` to `type_`) and never emitted raw.
- Constructor emit lane (`emit_constructor_entry` / `emit_record_definitions`, driven by `emath build`): one named typed entry per runnable function. The CLI assembles the artifact as a self-contained crate - a generated `Cargo.toml` that is its own workspace root (an empty `[workspace]` table, so it builds in place under a parent workspace such as the default `target/emath` output) with zero dependencies (except that a function using `quote.cost`, `quote.world`, or `quote.worlds` runs in the reference engine, so its artifact carries the engine's crates under `vendor/` (the `emath-*` dependency closure of `emath-exec-ir`, found and embedded by `build.rs`, with a `Cargo.lock` pinning the third-party versions; the vendored `emath-rt` crate replaces the inline `mod emath_rt`), names them by relative path, and embeds the module source, its path, every module its `use` imports resolve to (path and text, so the artifact never reads them from disk or `EMATH_ROOT`), and the build's world list) plus `mod emath_rt { emath_rt::SOURCE }` embedded verbatim whenever the emitted code references the runtime, so `cargo build` works offline outside this repository. Closure carriers are shared `Rc<dyn Fn(P...) -> Result<R, String>>` at every position. Constructor entries now require the runtime for cooperative execution budgets.
- Constructor execution shares the runtime's thread-local work/depth budget across entries, local recursive helpers, returned closures, host reentry and dynamic Code. Tail-loop iterations retain their entry scope; body evaluation and collection/control loops charge work. Defaults are 50,000,000 native units and depth 10,000, the VM's call-depth law; an explicit `emath_rt::execution::with_budget` scope can lower either before entry. A recursive entry's public wrapper charges work but no depth, so `deep(9_999)` computes and `deep(10_000)` refuses as in the VM. Entries without a tree wrapper whose input and result carriers are all `Send` run inside `emath_rt::execution::on_stack`, a reserved-stack thread per top-level call; other entries are capped at 512 frames off that stack. Exhaustion is sticky and named `E-NATIVE-WORK` / `E-NATIVE-DEPTH`. Native units are not VM instruction counts. Synchronous kernels and opaque host callbacks are not preempted, and separate threads do not share an allowance. Conformance: `emission_conformance_native_budget`; complete learning/exploration and conjecture workflows select explicit host budgets without changing mathematical assertions.
- A declaration with no `state` and no constructors emits a free `fn` per evaluate target (no `self`, no unit struct). Worked-example tests call that function directly.
- Constructors are controlled entry points: every `require` precondition and `ensure`/`invariant` postcondition is checked in generated code before a value escapes.
- Goals and tests attach by declared ids, never by span geometry.
- `model` declarations emit explicit `step_euler` and `step_rk4` step methods over `der_<state>` rates; models with `algebraic:` residual equations render the shared typed residual step program and its authored Newton cells (forward-difference Jacobian, Gaussian elimination, 30 iterations, 1e-9 solve tolerance, 1e-6 convergence check), returning `Result<Self, String>` that errors on non-convergence instead of inventing a value; no silent omission. Algebraic unknowns are fields of `Self` (extended DAE state). After the differential update each step re-solves them at the accepted state so the algebraic residual at the returned point is ~0 (index-1 projection), matching `emath simulate`.
- Program literals retain typed captures. Literal frames bind their own inputs and state. Nested frames and programs disable outer register inlining, so outer substitutions cannot change inner bindings.
- Program-space quote emission:
  scalar unary templates use a two-stage compiled factory,
  `open::<V>`, with the declared Int/Rat/Bool carrier. Substitution
  retains that carrier and evaluate returns its typed callable.
  A non-scalar template instead uses `open_expr` with the distilled
  tree and no scalar factory. Its declared callable result or
  structured value follows the checked tree lane described below.
- Expression templates: scalar
  bodies compile once over the Int/Rat/Bool `CodeValue` union using
  shared dynamic kernels. Closed scalar bodies wrap into the union.
  Float64 and Text do not enter it. Comparisons return checked Bool;
  boolean combinators retain `CodeValue::Bool`, and dynamic conditions
  project through `code_as_bool`. Substitution converts supported
  scalars into the union. Evaluate projects onto a declared scalar
  output with `project_i64`/`project_ratio`/`project_bool`: Rat widens
  Int exactly, Int refuses Rational, and Bool admits only Bool.
  Branches, frame inputs, call edges, sequence elements, and record
  fields use the same checked projection law, including dynamic
  nodes and recursive sequence joins. Non-scalar expression bodies
  retain a factory-free tree instead of forcing a scalar union.
- The residual lane splits the work with the union lane: a
  substitute-then-evaluate chain over statically known templates folds to plain
  arithmetic over the declared inputs. The union `CodeLiteral` remains for
  template values and non-residualizable consumers. Mixed Int/Rational
  arithmetic (`x + 1/2`) retains the integer as an exact numerator over 1,
  including wide `ExactInt` operands. A rational operand locks the rational
  carrier; no Float64 fallback implements this crossing.
- Shared tree view/make:
  `open_expr(free, TREE, factory, DEPS)` carries the distilled
  `CodeTree` and stamped dependencies; scalar templates also retain
  a compiled factory. The crate emits `__EMATH_MODULE_TABLE` when
  needed and resets mint tokens at tree-lane entry boundaries.
  View/make, checked node fields and sequence indexing preserve
  the node family; `node_eq` keeps Text separate from schema tags.
  Evaluate uses the compiled factory when present, otherwise the
  limited tree evaluator. Declared scalar arrows use the callable
  adapter, structured results use `ExprCode::evaluate_node`, and
  ordinary scalar results use `evaluate_expr`. Module calls carried
  by the table compute; tuples, lazy branches and tuple indexing
  compute in the structured lane. Unsupported leaves and globals
  still refuse by name. Scalar unary factories remain closure-only.
- Definition table - quote.body: `CodeBody` renders `({code}).body(&__EMATH_DEFINITIONS)?` (kind `Node`) - the Available/Opaque body-record unfold over a crate-level `static __EMATH_DEFINITIONS: std::sync::LazyLock<emath_rt::code_tree::DefinitionTable>` built from `module_definition_table` (one `LazyLock` initialization; rows render name, opacity, and the distilled body via the same `tree_expr`). Over a NODE callee the same op renders `quote_body_over_node`: a named node resolves through the table (`Available`), an op-tag callee yields the no-identity `Opaque` (an operator has no authored body). The dual-representation debt is OWNED here: the table is distilled by the same lowering that compiles the entries - one lowering, two projections, cannot drift, neither hand-written. `to_node` folds a list of family-joinable elements (an all-made argument list, `[made, made]`) into the family's dynamic sequence elementwise. No-claim boundaries: a transparent body outside the distilled subset refuses by name at the unfold; a raw union (`CodeValue`) result in a MULTI-output record member is not projected (the checked projection is the single-output boundary) - authored value pins compare instead (`== 8 / 1`), and the compile error is the boundary's honest shape.
- The binder half - quote.open / quote.bind: `CodeOpen` validates and
  unwraps a node-family package or drops a plain Code's factory and
  capture snapshot. Closure-only unary templates still refuse the
  no-tree lane. `CodeBind` call form mints identities and re-stamps
  the snapshot. Node-wrapped Code arguments unwrap or rebuild
  checked at frame, recursive and sibling-call boundaries.
  The quote.bind binder form remains outside the distilled subset.
  Mutual recursion uses emitted sibling entries; its argument
  carriers settle before entry emission. Consumer evidence includes
  authored differentiation through `diff_session.emath` and the
  mutual-name cycle in `engine_worlds.emath`.
- Text formatting uses positional operands and preserves literal braces. Statistical estimates use the authored `Estimate` record layout.
- `SameVector`, `SameMatrix`, and `SameTensor` result signatures retain their dense carrier kind. Authored guards retain shape validation.
- Current subset: one constructor and one evaluate goal per declaration.
  `Float64` is `f64`; `Int`/`Nat` are exact `i64` (`ConstI64` is not
  widened through f64). Mixed Int/Float64 arithmetic widens to `f64`.
  Rank-3+ values are `emath_rt::Tensor`. Index/slice emit checked
  `emath-rt` helpers (`vec_index_checked`, `tensor_slice_as_*`); evaluate
  methods that can fault return `Result<T, String>` instead of panicking
  `[]`.

## Error model

`BackendError` enum: `MissingInput`, `UnsupportedType`, `UnsupportedBinding`, `MissingArtifactBinding`, `StaleArtifactBinding`, `MissingArtifactContract`, `Lowering`. Provider/native absence and semantic operations that bypass the universal artifact seam are typed backend refusals. All variants implement `Display`/`Error`. Profile validation surfaces E-CODEGEN-002/`E-CODEGEN-004` on the exact rendered module.

## Determinism class

Deterministic and byte-comparable. The same checked snapshot and target inputs produce identical generated crate bytes repo-wide; `value_expr` materializes ops deterministically via `__e<i>` temporaries.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface documented.

## Unsafe boundary

None in the backend itself (`#![forbid(unsafe_code)]`; workspace lint forbids unsafe_code). Generated crates also carry `#![forbid(unsafe_code)]`.

## Feature flags

None. Cargo.toml has no `[features]`.

## Conformance tests

Integration tests in `tests/emath-rust-backend/tests/`:
- `numeric_boundaries.rs` (`native_numeric_boundaries`): compiles and
  executes the generated numeric-boundary fixture, including named
  overflow refusals and wide-frame preservation.
- `op_domains_render.rs` (`op_domains_render`): the op-domain render
  matrix.
- `profile_validate.rs` (`profile_validate`): profile parse and
  validation refusals.
- `render_paren.rs` (`probe`): rendered atoms stay unwrapped.

The historical model-step tests (`stateless_declaration_emits_free_function`,
`causalized_model_emits_newton_step_methods`,
`model_emits_explicit_step_methods`, and the escape/keyword rows) were
removed with the model surface: `model` is not a core kind (`E-KIND-GONE`
at admission, `is_model = false` hardcoded in sema), `emath simulate` is
not a constructor command, and the authored steppers live in
`language/modules/numerics/euler.emath` and `rk4.emath` over the
`analysis/evolution/one_step` family (exact Rat tier, census-runnable).
The `codegen_steps` Euler/RK4/Newton emission templates and the CLI's
`simulate_cmd` were removed outright after that verification (user
authorization 2026-09-24): no caller from admitted source, no tests, the
generate.rs model arms and the rate-context render lane went with them.
The exec-ir `runner/simulate` refusal shell was later deleted with the rest
of the SIR runner.

Legacy domain-render assertions are not part of this contract. They must be migrated outside this crate to construct `ApplyCapability` programs with executable artifact contracts.

## No-claim boundaries

- Only the current subset is generated: a declaration needs exactly one evaluate goal and supports one constructor. Admitted types: `Float64`, `Bool`, `Int`, `Nat`, vectors/matrices/tensors, authored records, host opaques. Other types yield `UnsupportedType`.
- Capability generation reads the verified installed distribution. With no native binding, an installed reference program supplies the body. Argument count must match. Separate cell guards and result guards still refuse rather than disappear. Unsupported instructions retain their existing refusals. Native bindings still require a matching artifact contract; the backend does not recover legacy domain dispatch.
- No certification power; generated crates carry invariants but the backend itself performs no evidence checks.

## Absorbed module: `rust_ir` (was `emath-rust-ir`)

# CONTRACT.md

## Purpose and layer

Structured Rust IR: a target AST with deterministic rendering, identifier hygiene and byte-range anchors for source maps. Layer: `ir` (per CRATE_MAP.md). No string-concatenated generation outside this renderer.

## Public types and semantics

Frequently re-exported types (not exhaustive):

- `HostBinding`, `HostMethod`, `HostTraitSpec`, `HostBindError` (module `host`): `generate_binding`, `fallback_binding`, `append_to_module`, `check_version`.
- `CrateProfile`, `ProfileProblem` (module `profiles`): `parse_profile`.
- `FileSet`, `Anchor`, `RenderResult` (module `render`): `render_module`, `render_file_set`, `render_file_set_partitioned`, `render_generics`, `coverage_gaps`.
- Module `ast`: full AST item types (`Module`, `Item`, `StructDef`, `FnDef`, `ImplDef`, `EnumDef`, `Expr`, `Stmt`, `Ty`, etc.) and helpers `escape_ident`, `snake_case`, `RUST_KEYWORDS`.
- `ConstructorCrateEmission`, `ConstructorEmitRefusal` (module
  `constructor_crate`): `emit_constructor_crate(prepared, spec_path, root_text)`.
  Callers pass the session's preparation and matching original root text. Import
  merging and engine source packaging use retained trees/bytes, not file reloads.
  `spec_path` still determines package/source names and may be canonicalized;
  this is not an atomic filesystem snapshot or transitive identity seal.
  The result includes lib text, manifest, files, package name, runnable/functions/
  unresolved status and authored records. Hosts recover escaped Rust field
  names using those records, never a heuristic. Complete crate files, including
  named entries for multiple root functions, are shared across current hosts.

## Invariants

- The elaborated-package path uses the structured Rust AST and its renderer.
  Constructor entries use the constructor program renderer; crate assembly
  concatenates their named entries, headers and embedded source.
- Identifier hygiene: Rust keywords and reserved names are escaped, never emitted raw.
- `Expr::F64` renders finite values with Debug (`1.0`); NaN/Inf use `f64::from_bits(0x…)` so generated crates compile (Debug `NaN`/`inf` are not Rust literals).
- Byte-range anchors are produced for source maps (`Anchor`, `coverage_gaps`).
- Profile validation refuses unknown ranges (E-CODEGEN-003), unsafe code in a safe profile (E-CODEGEN-002) and public items without a source-map anchor (E-CODEGEN-004).

## Error model

`HostBindError` (stable `E-HOST-001`/`E-HOST-002`): unknown/incompatible binding refusal, typed rather than silent stubs. `ProfileProblem` carries stable codes `E-CODEGEN-002`/`E-CODEGEN-003`/`E-CODEGEN-004`. `RenderResult` reports coverage gaps as data, not panics. `ConstructorEmitRefusal` is `NotConstructor(&'static str)` or `ImportsRefused { e_code, detail }`; the CLI keeps its E-code presentation and the loop host maps to `loop_export_emit` - never a partial crate.

## Determinism class

Deterministic. Rendering is byte-stable given the same AST; no RNG or wall-clock input.

## Cancellation behavior

Synchronous target rendering, no async cancellation surface documented.

## Unsafe boundary

None. `#![forbid(unsafe_code)]` at crate root; workspace lint forbids unsafe_code.

## Feature flags

None. Cargo.toml has no `[features]`.

## Conformance tests

No `crates/`-side tests for the profile surface. The former
`tests/emath-rust-ir` package's suite lives in `tests/emath-rust-backend`:
`tests/profile_validate.rs` exercises `CrateProfile::validate`
(`E-CODEGEN-002`/`E-CODEGEN-003`/`E-CODEGEN-004`).

## No-claim boundaries

Source admission, atomic snapshots, transitive identity and offline dependency
availability remain the explicit boundaries stated above.

## Shared authored carriers

Generated input and state loads borrow non-Copy carriers. Repeated record
arguments do not move the source value. Existing owning boundaries
materialize returned values and record fields. Input-dependent quantized
self-products exercise this rule through compiled search.

## Matrix carrier

Generated `Matrix<Float64>` values use `emath_rt::Matrix`, not nested vectors.
The carrier stores both dimensions and flat row-major data. It preserves
`0×N` and `N×0` without allocating rows. `Matrix::new(rows, cols, data)`
checks the dimension product and data length. `get(row, col)` checks both
indices. The constructor does not perform mathematical operations.
The reference operators `matrix_rows`, `matrix_cols`, `matrix_at`, and
`matrix_pack` use this carrier. Their checked operations return errors.
