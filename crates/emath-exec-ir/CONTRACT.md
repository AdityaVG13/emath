# emath-exec-ir Contract

## Purpose and layer

`emath-exec-ir` is the stable executable machine between admitted semantic terms and execution providers. It owns universal literals, registers, construction/storage/indexing, control, capability application, provider continuations, semantic images, and artifact loading. It does not own mathematical feature identity or meaning.

## Stable instruction boundary

`EmirOp` contains only:

- literal and input/state load instructions;
- closed scalar and carrier instructions used by reference bytecode;
- generic record, set, series, vector, matrix, tensor, Option, and Result construction/storage/indexing;
- selection and finite fold control;
- `ApplyCapability { capability, class, args }`.

There are no graph, optimization, probability, differential-equation, PDE, control-theory, category-theory, exact-arithmetic, geometry, units, chemistry, or other domain-named public variants. A legacy `ExprNode::Call` or domain computation that reaches this layer is refused: semantic admission must first resolve it to an interned FeatureID application. No compatibility alias or fallback operation is provided.

## Capability execution

`ApplyCapability` treats its capability string as an opaque FeatureID. Non-pure cells return an explicit `ProviderCallRequired` continuation. Pure cells execute either as an authored `EmirProgram` in the reference VM or, at an `ApplyCapability` boundary, through an optional native kernel binding selected from the checked Language Image by domain-neutral kernel ID plus exact carrier signature.

The VM never branches on a FeatureID spelling. An applied pure capability without an installed kernel binding refuses; it does not fall back to a handwritten cell registry. Argument count is checked before a native kernel runs. Kernel failures remain typed capability refusals.

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

Execution observes explicit step and capability-application budgets. Exhaustion returns `BudgetExhausted`; incomplete work and partial capability authority do not escape.

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
