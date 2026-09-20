# emath-exec-ir Contract

## Purpose and layer

`emath-exec-ir` is the stable executable machine between admitted semantic terms and execution providers. It owns universal literals, registers, construction/storage/indexing, control, capability application, provider continuations, semantic images, and artifact loading. It does not own mathematical feature identity or meaning.

## Stable instruction boundary

`EmirOp` contains only:

- literal and input/state load instructions;
- closed scalar and carrier instructions used by reference bytecode;
- generic record, set, series, vector, matrix, tensor, Option, and Result construction/storage/indexing;
- selection, lazy branch programs, and bounded accumulator iteration;
- authored refusals that return no value;
- `ApplyCapability { capability, class, args }`.

There are no graph, optimization, probability, differential-equation, PDE, control-theory, category-theory, exact-arithmetic, geometry, units, chemistry, or other domain-named public variants. A legacy `ExprNode::Call` or domain computation that reaches this layer is refused: semantic admission must first resolve it to an interned FeatureID application. No compatibility alias or fallback operation is provided.

## Capability execution

`ApplyCapability` treats its capability string as an opaque FeatureID. Non-pure cells return an explicit `ProviderCallRequired` continuation. Pure cells execute either as an authored `EmirProgram` in the reference VM or, at an `ApplyCapability` boundary, through an optional native kernel binding selected from the checked Language Image by domain-neutral kernel ID plus exact carrier signature.

The VM never branches on a FeatureID spelling. An applied pure capability without an installed reference program or kernel binding refuses; it does not fall back to a handwritten cell registry. Argument count is checked before a native kernel runs. Kernel failures remain typed capability refusals.

## Executable reference vocabulary

Authored `reference_body` terms lower arithmetic, comparisons, Boolean
operations, and finiteness checks to existing universal instructions.
`const(true)` and `const(false)` produce Booleans. `const(i64:42)`
produces an exact checked-range integer. An unprefixed numeric constant
produces a finite binary64 value; malformed or nonfinite literals refuse.
The capsule signature must declare every constant as a nullary symbol.

`call_program(program, inputs)` invokes a numeric Program. Ordinary programs
bind one Float64 argument per declared input. Vector-input programs bind the
whole vector as their first input. Typed captures follow explicit arguments;
the complete input count must match. A scalar Float64 result is packed into
one element; a vector result retains its shape. Unbound state, wrong arity,
and other result carriers refuse. Literal frames bind typed inputs and state
without copying unused state arrays. Dense layout metadata retains carrier
kind, dimensions, and stored length; it does not certify shape validity.
`call_scalar_program` requires a Float64 result without packing.
`call_real_program` accepts Float64 or Int and converts Int to binary64.
`try_call_real_program` returns Result<Float64, Text>; ordinary evaluation
faults and nonnumeric results become Err. Budget exhaustion still propagates.
These operations preserve each authored caller's result-conversion policy.
Nested execution uses the caller's shared evaluation budget. Method selection,
rate-shape checks, and finiteness guards belong to authored definitions.
`vector(sequence)` packs only Float64 elements; it never widens exact values.
`sort_total(vector)` uses stable binary64 total ordering, including signed
zero and NaNs. Statistical guards and quantile formulas remain authored.
Authored trailing defaults bind left-to-right against preceding parameters.
Defaults share the caller's budget and run only for omitted arguments.
Semicolons separate defaults only between complete canonical terms.
Semicolons inside text constants remain part of the text.
`iterate_until(index, state, count, initial, stop, body)` evaluates `stop`
before each update. Both programs bind the index and accumulator. A true
stop returns the current accumulator without executing the body. The count
remains a hard update limit. Predicates and bodies share the caller's budget.
Reference bodies use binary64 `sqrt`, `abs`, `sin`, `cos`, `max`,
`exp`, `ln`, `exp2`, `powf`, and `powi` primitives. The `powi` exponent
must be Int and fit i32. These are numeric representation operations;
special-function algorithms and domain guards remain authored.

`text_trim` uses Unicode whitespace trimming. `text_length` counts UTF-8
bytes, and `text_byte` returns one checked byte as Int. `format_scientific`
takes a Float64 and a nonnegative Int precision after the decimal point.
`parse_f64` parses Text as binary64. Invalid indices, conversions, and
formatting allocation failures refuse. These operations contain no
significant-figure policy. `index_text` uses the target's Float64-to-usize
conversion and decimal formatting. `format_text` replaces positional `{}`
markers in a constant template. It never scans inserted argument text for
more markers. Unfilled markers remain text; extra arguments add no text.
`refuse_value` raises a refusal with the
supplied Text, including dynamic index details.

`installed_reference_cell` exposes only reference programs from the verified
installed distribution. Artifact generation uses this same program when no
native binding exists; it does not identify mathematical features by name.

## Source case execution

`runner::run_test` executes one admitted source example. `runner::run_direct`
executes one declaration with explicit bindings. Both use the existing
constructor, assumption, lowering, and verdict rules. They return `TestRun`
values without evaluating sibling cases. The CLI uses these boundaries
for saved progress; they do not suspend inside a capability or solver.

## Loop-session scratch checkpoints

`constructor_layer::scratch` is the `emath.scratch.v1` host-side checkpoint
of one research-loop session: module identity, target Step function,
explicit loop state (a `CValue`), and the immutable batch ledger. It is a
sibling of the continuation checkpoint with a different job: the
continuation suspends an engine mid-reduction; the scratch commits a
session at a batch boundary (all loop state is explicit data, X1).

Identity is carried, never computed: the host passes the module's meaning
id, language image id, and target name in; `decode_scratch`/`load_scratch`
compare them against the expectation and refuse `scratch_identity` on any
mismatch. File-internal consistency is enforced on both save and load:
the ledger length must equal the declared revision and entries are
1-based and ordered (`scratch_ledger`); a non-writer-JSON document refuses
`scratch_torn`; a foreign schema line refuses `scratch_schema`. The
module-semantic law that the revision equal the loop state's own batch
count belongs to the host (it knows the `LoopState` shape), not this
contract.

The value interchange is verbatim, not normalizing: exact integers render
as decimal (arbitrary magnitude), rationals as `[num, den]` (zero
denominator refused), floats as bit-exact hex, records/variants/tuples/
sequences with ordered fields. Scalar arithmetic follows the
operand-carrier rule: integer operands keep the integer carrier
(`2 + 3` is Int 5), and rational or mixed operands keep the rational
carrier even when the canonical result is integer-valued
(`1/4 - 1/4` is Rat 0/1). The emitted ExactRatio arithmetic (the
artifact ABI) implements the same rule, so a Rat-typed field renders
identically in the VM and native lanes (cross-lane scratch parity).
Closures, code, receipts, and buffers
refuse `scratch_unserializable`: closures are re-instantiated by the host
from authored declarations; the others are engine artifacts or mutable
state, not checkpoint cargo. Ledger projections are machine-scale
(`i128`); loops whose keys or scores exceed that scale cannot keep a
scratch ledger.

Determinism class: the encoding is byte-deterministic for equal inputs.
Conformance is `tests/emath-exec-ir/tests/constructor_scratch.rs` over
the valley session surface
(`tests/fixtures/constructor/research_step_valley.emath`), including
resume-through-file equality with the straight run, mutation divergence,
identity refusal, and torn-file refusal.


Case runners retain independent definitions after a lowering or execution fault.
A failed definition never supplies a guessed binding to later expressions.
The first failure remains the case verdict; a failed case cannot pass its expectation.
Constructor and assumption checks remain strict. `eval_definitions_values`, used
by numerical solver callers, remains fail-fast.

## Program-space quote emission (emath-npky7)

Constructor lowering emits exactly one quote subset: `quote(<unary
scalar-carrier function literal>)`, `quote.substitute(code, "name",
value)`, and `quote.evaluate(code)`. The ops are `CodeLiteral` (the
template's nested program with the open constants as trailing runtime
inputs, plus the declared carrier), `CodeSubstitute` (partial
application; the reference is a static string resolved at lowering,
the value is carried in the template's carrier), and `CodeEvaluate`
(the guarded executor; a def bound to it is closure-valued, so later
`f(x)` lowers as `CallValue` — the fourth closure-valued def source).
The admitted carriers are the declared scalar domains `Int`, `Rat`,
and `Bool`: the domain governs the parameter AND the open constants,
so the artifact's compiled factory is monomorphic in the carrier
(emath-3ran3 widened this from the original Rat-only cut).

The hygiene law is structural: a quote never captures the ambient
frame. Every free name of the template body (beyond the parameter)
stays open and becomes a substitution input; nested rebinders keep
their own scope (the free-name collector's L2 rule). One predicate
(`emitted_quote_carrier`) is the single authority admitting the shape
in both the lowering and the unresolved walk, so the two cannot
disagree. Every other quote form (`QuoteBind`, non-lambda bodies,
other domains, other `quote.*` spellings or arities) keeps the
emission fence: it lowers to a named refusal and the module is not
runnable.

The E-MIR interpreter refuses the three ops as emission-carried
(`CarrierRefused`): the constructor lane computes quotes through its
own `CValue::Code` machinery; these ops exist for the native lane.

Determinism class: lowering is a pure function of the authored tree.
Conformance is `tests/emath-tui/tests/export_native.rs` case 9
(cross-lane scratch parity over
`tests/fixtures/constructor/dream_program_space.emath`, whose
template carriers are Rat, Int, and Bool) and
`tests/emath-rt/tests/code_carrier.rs` (the carrier laws), with
mutation probes: splice position, the `unbound_code` guard, the
closure-valued source, the unresolved exemption, and the
carrier-kind mapping each kill their suite.

## Kernel boundary

Native kernels are immutable implementations keyed by domain-neutral kernel IDs and carrier signatures. `install_language_distribution` derives FeatureID bindings exclusively from capsule-active Language Image rows and starts from an empty binding map. There are no built-in FeatureID aliases or legacy bindings.

A kernel computes values or faults only. It does not select feature identity, semantics, exactness, applicability, world, evidence, authority, or result labels.

## Optimization

Capability applications and storage/control operations are opaque to optimization. The optimizer does not inspect FeatureIDs or select kernels. The current stable optimizer performs no speculative rewrite; generic operand enumeration remains available to artifact/liveness consumers.

## Semantic images and artifacts

Semantic images contain deterministic cell, bytecode, evidence, lock, and metadata partitions. Partition and image identities derive from canonical content. Corrupt, stale, duplicate, manually edited, or inconsistent pages refuse before partial authority is exposed.

`LanguageImage`, `RuntimeTables`, and `GeneratedReferenceViews` are projections of authored capsules and authority data. Semantic, distribution, and operational hashes remain distinct. Operational metadata is outside semantic bytes.

Tree shaking computes reachable bytecode closure from declared entries. Required dependencies survive; unknown entries and attempts to demote required dependencies refuse. Source cell records are not removed by bytecode shaking.

## Determinism

Identical program, inputs, state, installed Language Image, and budget produce bit-identical values or the same typed refusal. The machine reads no ambient time, entropy, network, or mutable process-global feature registry.

## Budget and cancellation

Execution observes explicit step and capability-application budgets. Nested calls, branches, and iterations share the outer evaluation fuel. Iterations charge fuel even for an empty body. Exhaustion returns `BudgetExhausted`; incomplete work and partial capability authority do not escape.

## Error model

Malformed registers, carriers, indices, storage shapes, missing inputs/state, budget exhaustion, missing kernels/reference bytecode, provider continuations, and kernel refusals are typed. Unsupported legacy named operations refuse during lowering. The machine does not silently coerce, truncate, invent an identity, or consult an obsolete path.

## Unsafe boundary

None. The crate forbids unsafe code.

## Feature flags

The stable machine has no domain feature flags. Feature availability is Language Image data.

## Conformance

Conformance must cover generic FeatureID lowering, capsule-derived kernel installation, reference/native parity, typed provider continuation, budget refusal, image tamper/staleness refusal, deterministic artifact identity, and the whole-nucleus contraction gate. Historical tests constructing removed domain-named `EmirOp` variants are external caller residue and must migrate to capsule applications before their packages can compile against this contract.

## No-claim boundaries

This crate does not claim that every authored capability has local reference bytecode or a native kernel. It does not assign mathematical meaning, prove a kernel equivalent to a capsule, choose a world/provider, or generate a compatibility path for removed operations. Missing executable material is an explicit refusal.

The live interpreter is `src/interp.rs` (`mod helpers` / `mod value` only).
The compiled optimizer is the no-rewrite `optimize.rs` plus `ops_impl.rs`
name metadata. Undeclared leftover interpreter/optimizer files were deleted
on 2026-09-04 with user authorization (`emath-5gz4o`).


## Authored structured programs

Reference terms support lexical `let(name, value, body)`, lazy
`if(condition, then, else)`, and `iterate(index, state, count, init, body)`.
The two iteration binders have distinct names. Counts are nonnegative integers.
Only the selected branch executes. Nested calls share the evaluation budget.
Authored call graphs are acyclic; repetition uses explicit bounded iteration.

`list(...)` preserves element carriers. `length(value)` and
`index(value, index)` access sequences. `record:Name:field:field(...)`
constructs a nominal record; `get(record, const(field))` reads a field.
A capsule declares its layout with `record_fields: "field=Type;field=Type"`
and `output=Record<Name>`. Installed layouts come from verified source data.

A canonical FeatureID operator calls another capability. No mathematical
operation name becomes a compiler branch. `refuse(const(text:CODE))`
returns the authored diagnostic without a value.

## Authored method state

Verified capsules can supply `method_step`, `method_check`, `method_complete`,
and `method_bindings`. Installation validates their record and callable ABIs.
The progress scope binds saved arguments to stable call sites. After case
initialization, one work unit advances one pending method in call-key order.
The checker validates saved certificates without replaying refinement.
No mathematical algorithm or completion-field name is selected in Rust.
Outside a progress scope, evaluation remains ordinary authored execution.
`Value::json` preserves recursive records, exact rational components, and
floating-point bits. Program values are output-only, not resumable arguments.

## Authored model method boundary

Explicit and residual Euler/RK4 use typed per-model programs and authored
stage formulas. Residual models use authored Newton iteration and Gaussian
elimination. The returned state includes accepted-point algebraic projection.
Cash-Karp stage coefficients and adaptive policy, backward-Euler numerical
updates, and Verlet updates and structure tolerances also use authored cells.
The runner retains model frames, step scheduling, carrier checks, and events.
Newton counts scalar components, not fields. Scalar and vector carriers remain
distinct, including one-element vectors. A one-component residual can be
scalar or a one-element vector. Each explicit rate must match its own state
field shape, not only the total flat width. These API invariants do not add
new source-syntax admission.

Matrix row and column operations read carrier metadata without certifying
the stored length. Authored guards can therefore return the method-specific
shape refusal before arithmetic. Matrix element access still checks shape
and bounds.
