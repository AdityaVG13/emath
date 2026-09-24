# CONTRACT.md

## Purpose and layer

Rust backend: universal EMIR and artifact contracts to deterministic Rust via the rust-ir AST. Layer: `rust-ir` (per CRATE_MAP.md). The backend emits structural operations and universal control/data operations; mathematical meaning is not selected by feature names or domain-specific emitter branches. Semantic operations must arrive through `ApplyCapability` plus a materializable artifact contract. The backend generates one crate per admission: a struct plus constructor for stateful declarations, a free function (not a method on an empty struct) when there is no state and no constructors, an evaluation item per `evaluate <target>` goal, explicit step methods for `model` declarations, and `#[test]` functions for the `tests:` section. Every generated crate embeds the `emath-rt` kernel module verbatim (`mod emath_rt { ... }` from `emath_rt::SOURCE`) and remains std-only, `#![forbid(unsafe_code)]`, and byte-deterministic.

## Public types and semantics

- `BackendInput { package, crate_name, version }`: input to `generate()`.
- `BackendOutput { files, anchors, assumptions, module, receipts }`: relative path to file content (including `Cargo.toml` and `src/lib.rs`), source-map anchors, surfaced domain obligations, the rendered module for `CrateProfile::validate`, and one `ConstructionReceipt` per generated constructor (the obligation matrix the emitted code discharges).
- `BackendAnchor`: byte-range anchor into generated `src/lib.rs`.
- `BackendError`: typed backend failure (variant list below).

## Native numeric boundaries

Recursive-call arguments and results, closure-call arguments, and authored record
fields share numeric representation conversion, including sequence elements and
sequence index operands. A fixed
`i64` slot checks an `ExactInt` with `to_i64` and returns `E-INT-002` outside
that lane. Int-to-Rat widens exactly, with `E-RAT-002` when a wide part cannot
fit the native i128 pair. Inferred-wide sibling frames and results stay wide;
there is no blanket narrowing of the arbitrary-precision VM carrier.

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
  parameter is `Rat` widens to the declared ratio carrier (`sum_rs_at(xs, 0,
  0)` with `acc: Rat`, `rat_pow(4, m)` with `p: Rat`); the binding crosses
  through the shared numeric boundary. Non-widenable actuals keep the actual
  carrier (generic templates instantiate at the call site's kind).
- Branch arms join before rendering: a rational arm absorbs an integer arm
  (`if k == 0: 0 else: a / b`) with the integer side widened at the render,
  exactly as an exact-int arm absorbs an i64 arm; the VM's Int-to-Rat join is
  the law.
- Mixed exact/float arithmetic (`hr * 1.0f64`, the labeled Float64 tier) meets
  on the float carrier when exactly one operand is `Float64` and the other is
  `Int`/`ExactInt`/`Rat`: the exact operand widens through the as-f64 coercion
  (`numerator as f64 / denominator as f64`, the ratio carrier is normalized).
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

- Generated crates are std-only, `#![forbid(unsafe_code)]`, `#![allow(dead_code)]`, and byte-deterministic.
- Generated crates embed `mod emath_rt { ... }` (the verbatim `emath-rt` kernel source) with an outer `#[allow(dead_code)]` so hosts that strip inner attributes (an `include!` driver pattern) stay warning-free.
- The emitter is exhaustive over the contracted universal `EmirOp`; active backend modules contain no removed domain-op references. Obsolete domain/dual helper files remain unreferenced only because deletion is forbidden.
- The emitter never maps a mathematical feature name or legacy domain operation to a runtime function. A semantic operation without an `ApplyCapability` artifact contract fails as `MissingArtifactContract`.
- `ApplyCapability` dispatches only on generic cell class. Unsupported provider and intrinsic/native bindings fail as `UnsupportedBinding`; other applications without an executable artifact body fail as `MissingArtifactContract`. No identity, interpreter fallback, or compatibility shim is emitted.
- Generated manifest emits `edition = "2024"`, sanitized crate name/version; keywords and reserved identifiers are escaped (`type` to `type_`) and never emitted raw.
- Constructor emit lane (`emit_constructor_entry` / `emit_record_definitions`, driven by `emath build`): one named typed entry per runnable function. The CLI assembles the artifact as a self-contained crate - a generated `Cargo.toml` with zero dependencies plus `mod emath_rt { emath_rt::SOURCE }` embedded verbatim whenever the emitted code references the runtime (use-gated: an artifact that never calls `emath_rt` stays lean), so `cargo build` works offline outside this repository. Closure carriers are shared `Rc<dyn Fn(P...) -> Result<R, String>>` at every position. No-claim boundary: emitted code enforces authored budgets and refusals only - the engine work budget is a VM-host safety net and does not exist natively, so an authored non-terminating recursion that the VM would budget-stop runs unbounded in an emitted crate.
- A declaration with no `state` and no constructors emits a free `fn` per evaluate target (no `self`, no unit struct). Worked-example tests call that function directly.
- Constructors are controlled entry points: every `require` precondition and `ensure`/`invariant` postcondition is checked in generated code before a value escapes.
- Goals and tests attach by declared ids, never by span geometry.
- `model` declarations emit explicit `step_euler` and `step_rk4` step methods over `der_<state>` rates; models with `algebraic:` residual equations render the shared typed residual step program and its authored Newton cells (forward-difference Jacobian, Gaussian elimination, 30 iterations, 1e-9 solve tolerance, 1e-6 convergence check), returning `Result<Self, String>` that errors on non-convergence instead of inventing a value; no silent omission. Algebraic unknowns are fields of `Self` (extended DAE state). After the differential update each step re-solves them at the accepted state so the algebraic residual at the returned point is ~0 (index-1 projection), matching `emath simulate`.
- Program literals retain typed captures. Literal frames bind their own inputs and state. Nested frames and programs disable outer register inlining, so outer substitutions cannot change inner bindings.
- Program-space quote emission (emath-npky7, carriers widened emath-3ran3): `CodeLiteral` renders as a two-stage factory - the nested program lowers once into `move |param: V, free...: V| -> Result<V, String>` where `V` is the template's declared scalar carrier (`emath_rt::ExactRatio` for Rat, `i64` for Int, `bool` for Bool), and `emath_rt::code::open::<V>` wraps it in the outer factory (`Rc<dyn Fn(&[V]) -> Result<Unary<V>, String>>`) that owns the bound constants and yields the specialized closure (`ValueKind::Code(carrier)`). A body computing any carrier other than the declared one refuses typed at emission. `CodeSubstitute` renders `emath_rt::code::substitute(&code, "name", value)` (the value must carry the template's carrier, else `UnsupportedType`; the generic instantiation would not compile a mismatch anyway); `CodeEvaluate` renders `emath_rt::code::evaluate(&code)?` with kind `Closure { V -> V }` so `CallValue` applies it typed. Hygiene is structural: the nested body's only external names are its parameter and the open constants, so there is no ambient capture to render; no tree and no interpreter are emitted - candidates execute as the compiled closures.
- Expression templates (emath-expression-quotes-324y0): a `CodeLiteral` with `param: None` renders the body ONCE over the dynamic value union (`ValueKind::CodeValue`, the rt `CodeValue` Int/Rat/Bool union) - every scalar op of the body renders as a call to a shared rt dynamic kernel (`code_add`/`code_sub`/`code_mul`/`code_div`/`code_neg`/`code_not`/`code_eq`/`code_cmp`/`code_as_bool`) implementing the VM's exact carrier rules, wrapped by `emath_rt::code::open_expr` (the `ValueKind::ExprCode` carrier, the same by-name/no-op/`unbound_code` laws as the function carrier). Kind rules route a joinable union pair (one `CodeValue` operand, the other Int/Rat/Bool) onto the kernels; anything else (Float64 beside the union, structured bodies, a closed body with no free name) refuses named at the body-kind check. `CodeSubstitute` over an expression template accepts any scalar and converts it into the union; `CodeEvaluate` yields the computed union scalar (`CodeValue`), and the entry boundary projects it checked onto the DECLARED output carrier (`project_i64`/`project_ratio`/`project_bool`) - the engine's `type_admits` law (Rat widens Int exactly; Int refuses a Rational by name; Bool admits Bool only); a `CodeValue` result without a declared scalar output refuses named. This lane is still not a tree interpreter: no expression tree is carried into the artifact, the body is compiled arithmetic over the union. The no-interpreter boundary is hereby precise: what is never emitted is a structural walk over a carried tree; compiled dynamic-scalar kernels over a value union are exactly that - compiled arithmetic.
- The residual lane splits the work with the union lane (the pre-existing quote-elimination law): a substitute-then-evaluate chain over statically-known templates folds to plain arithmetic over the function's declared inputs, and the union `CodeLiteral` remains for template VALUES (a function whose def IS the bare quote) and non-residualizable consumers. Mixed Int/Rational arithmetic (`x + 1/2` with Int `x` - the residualized expression lane's shape) now widens the Int operand exactly (`(i128::from(n), 1)`) and computes rationally, matching the VM's `as_rat` law and locking the Rational carrier - previously this shape fell to the float lane and emitted a non-compiling `f64 + (i128, i128)`; the kind rule promotes the mixed pair to Rational. No-claim: mixed `ExactInt`/Rational pairs (an `ExactIntCall` result beside a rational constant) keep the pre-existing float fallback.
- Shared tree view/make (emath-shared-tree-view-make-bp8nu): a union-lane `CodeLiteral` now embeds BOTH representations - `emath_rt::code::open_expr(free, TREE, Some(factory), DEPS)` where `TREE` is the distilled std-only `CodeTree` (`tree_expr` renders it as a constructing Rust expression) and `DEPS` the stamped dependency map. When a lowered program needs it, the crate emits a crate-level `static __EMATH_MODULE_TABLE: emath_rt::code_tree::ModuleTable` built from `module_callable_table` (name, opaque, stamp per callable), and every tree-lane entry renders an `emath_rt::code_tree::reset_mint()` prologue so tokens are deterministic per run. `CodeView` renders `view_quoted`/`view_node` (kind `Node`), `CodeMake` renders `make_quoted` (kind `ExprCode`), and `CodeEvaluate` over an `ExprCode` renders `evaluate_expr(&code, &__EMATH_MODULE_TABLE)?` - the factory runs when present, else the rt tree evaluator. The `Node` value kind (`emath_rt::code_tree::NodeValue`) carries the walk records: `RecordCreate` with a node-tag type name renders `node_record`, `RecordField` over `Node` renders the checked `node_field`, list create with any `Node` element is `Node`-kind, and `VectorIndex`/`VectorLength` over `Node` render `node_index`/`node_field("length")`. Comparison over `Node` renders `node_eq` (the VM's `eq_values` law - scalar VALUE equality). Dual-rep debt: the function-template lane still carries no tree (compiled closures only). Tree-evaluator boundary: the rt `evaluate_tree` computes scalar trees and refuses calls/globals/structured bodies by name - the artifact makes no interpreter claim beyond compiled scalar arithmetic and the compiled factory.
- Definition table - quote.body (emath-quote-body-defs-trto7): `CodeBody` renders `({code}).body(&__EMATH_DEFINITIONS)?` (kind `Node`) - the Available/Opaque body-record unfold over a crate-level `static __EMATH_DEFINITIONS: std::sync::LazyLock<emath_rt::code_tree::DefinitionTable>` built from `module_definition_table` (one `LazyLock` initialization; rows render name, opacity, and the distilled body via the same `tree_expr`). The dual-representation debt is OWNED here: the table is distilled by the same lowering that compiles the entries - one lowering, two projections, cannot drift, neither hand-written. `to_node` folds a list of family-joinable elements (an all-made argument list, `[made, made]`) into the family's dynamic sequence elementwise. No-claim boundaries: a transparent body outside the distilled subset refuses by name at the unfold; a raw union (`CodeValue`) result in a MULTI-output record member is not projected (the checked projection is the single-output boundary) - authored value pins compare instead (`== 8 / 1`), and the compile error is the boundary's honest shape.
- The binder half - quote.open / quote.bind (emath-quote-bind-open-consumer-6f86g): `CodeOpen` renders `({package}).open()?` (a node-family package; kind `ExprCode`) or `({code}).open()` (a plain Code: the factory and snapshot dropped) - the witness-validated unwrap; a function-template operand keeps its named no-tree-lane refusal. `CodeBind` (call form) renders `({code}).bind(&__EMATH_MODULE_TABLE)` - the identity mint walk with the snapshot re-stamped. The call-marshal bridge: the node lane wraps codes as Code nodes, so a Node-kind argument crossing into a declared Code parameter - at a frame input (`frame_input_kind` lets the declared carrier win over the node wrapper) or a recursive self-call argument - converts through `node_as_code` (unwrap or rebuild, never a silent type change; the VM's call arguments ARE codes). The expression-template lane now accepts a closed SCALAR body (a bare literal, e.g. `quote(0)` in an authored transformation rule): the atom wraps into the union (`to_code_value`) instead of refusing - the free-name arithmetic path stays `CodeValue`-typed throughout. No-claim boundaries: the quote.bind BINDER form refuses at lowering (a fresh-tokened function literal is outside the distilled subset); mutually recursive siblings refuse lowering (the emission model inlines acyclic siblings and self-recursion only - entry-call emission for cycles is a future bead); the consumer proof that DID land is the authored differentiation module (`language/modules/calculus/diff.emath`, local-only) driven end-to-end through the session fixture `tests/fixtures/constructor/diff_session.emath` (export parity case 13).
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

`BackendError` enum: `NoEvaluateGoal`, `UnknownTarget`, `MissingInput`, `MissingGiven`, `UnsupportedType`, `MultipleConstructors`, `UnsupportedBinding`, `MissingArtifactContract`, `Lowering`. Provider/native absence and semantic operations that bypass the universal artifact seam are typed backend refusals. All variants implement `Display`/`Error`. Profile validation surfaces E-CODEGEN-002/`E-CODEGEN-004` on the exact rendered module.

## Determinism class

Deterministic and byte-comparable. Same `BackendInput` produces identical generated crate bytes repo-wide; `value_expr` materializes ops deterministically via `__e<i>` temporaries.

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
The `codegen_steps` Euler/RK4/Newton emission templates are therefore
unreachable from admitted source and stay on disk quarantined - the same
step-7 policy as the mathematical kernel files (on disk, never
dispatched).

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
- `ConstructorCrateEmission`, `ConstructorEmitRefusal` (module `constructor_crate`, bead emath-8k3zw): `emit_constructor_crate(main_tree, spec_path)` - the `emath build` emission core (import merge, authored-record collection, one named entry per main-file function, the `mod emath_rt` self-containment embed, and the manifest), shared with the loop host's `export-native` so the two callers' artifact crates are byte-identical by construction. Pure: it returns the lib text, the manifest, the package name, and the admission facts (runnable, functions, unresolved), plus the authored `records` with their EMATH field names - the emitted Rust escapes keywords (`move` becomes `move_` via `escape_ident`, a non-injective escape), so consumers that must recover the authored names map back through that list, never by heuristic.

## Invariants

- All generation goes through the structured AST and its renderer; no string-concatenated Rust emission elsewhere.
- Identifier hygiene: Rust keywords and reserved names are escaped, never emitted raw.
- `Expr::F64` renders finite values with Debug (`1.0`); NaN/Inf use `f64::from_bits(0x…)` so generated crates compile (Debug `NaN`/`inf` are not Rust literals).
- Byte-range anchors are produced for source maps (`Anchor`, `coverage_gaps`).
- Profile validation refuses unknown ranges (E-CODEGEN-003), unsafe code in a safe profile (E-CODEGEN-002) and public items without a source-map anchor (E-CODEGEN-004).

## Error model

`HostBindError` (stable `E-HOST-001`/`E-HOST-002`): unknown/incompatible binding refusal, typed rather than silent stubs. `ProfileProblem` carries stable codes `E-CODEGEN-002`/`E-CODEGEN-003`/`E-CODEGEN-004`. `RenderResult` reports coverage gaps as data, not panics. `ConstructorEmitRefusal` is `NotConstructor(&'static str)` or `ImportsRefused { e_code, detail }`; the CLI keeps its E-code presentation and the loop host maps to `loop_export_emit` - never a partial crate.

## Determinism class

Deterministic. Rendering is byte-stable given the same AST; no RNG or wall-clock input.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface documented.

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

No additional no-claim boundaries documented.

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
