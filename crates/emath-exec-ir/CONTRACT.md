# emath-exec-ir Contract

## Purpose

`emath-exec-ir` is the executable semantic layer between admitted mathematical terms and interpreters or generated backends. It provides a small generic operation vocabulary, capability-cell application, optimization, execution, specialization, semantic images, and field-pack loading.

## Public behavior

- Programs contain typed registers, operations, nested bodies, and declared outputs.
- Capability calls use `CapabilityId` data rather than domain-specific operation variants.
- The interpreter and emitter preserve operation order and typed refusal behavior.
- Optimization may fold or remove work only when observable values and faults are preserved.
- Every execution is budgeted; exhaustion refuses without exposing a partial answer.

## Capability cells

A capability cell supplies identity, class, version, migration policy, arity, numeric policy, guards, reference semantics, and provider contract. Applying an unregistered cell or violating a guard is a typed refusal.

Pure cells execute through the reference VM. Provider cells produce an explicit outstanding provider request. The dispatcher is generic and must not grow branches keyed by individual capability names.

## Semantic images

A semantic image contains deterministic partitions for cells, bytecode, evidence, locks, and metadata. Partition and image identities derive from canonical content. Corrupt or inconsistent pages refuse.

Tree shaking computes the reachable bytecode closure from declared entries. Required dependencies survive; unknown entries and attempts to demote required dependencies refuse. Source cell records are never removed by bytecode shaking.

## Specialization

Static specialization binds known inputs while preserving parity with the reference VM. Full binding may fold a program to constants. Unsupported bindings, guard failures, and seeded backend discrepancies are typed failures.

## Field packs and lazy loading

Field packs export existing registry cells. Installation resolves each export, writes a deterministic image and lock, and refuses unknown exports.

`LazySession::boot` loads the nucleus, image lock pages, and the packs selected by `Minimal`, `Standard`, or `Custom` profile. `load_for_compile` loads exactly the named reachable packs. Accessing an unloaded page is `E-LAZY-001`; an unknown pack is `E-LAZY-002`. Optional chunks are the sorted set of still-unloaded packs.

## Optimization kernels

`LpMinimize(A, b, c)` handles the standard-form class `Ax <= b`, `x >= 0`, `b >= 0` with deterministic Bland tie-breaking. `ParetoFront` returns a strict non-dominated mask. Invalid shape, non-finite values, and unbounded objectives refuse.

The general-form simplex and MILP surface remain in `emath-core`; the embedded execution runtime cannot call that crate and therefore owns its small compatible kernel.

## Determinism

Identical program, inputs, numeric policy, capability registry, and budget produce bit-identical results or the same typed refusal. No operation may read ambient time, entropy, network, or mutable global state.

## Cancellation

Execution observes its explicit budget at operation and capability boundaries. Cancellation or exhaustion discards incomplete work.

## Unsafe boundary

None. The crate forbids unsafe code.

## Conformance tests

- `tests/emath-exec-ir/tests/interp.rs` covers operation values and faults.
- `tests/emath-exec-ir/tests/optimize.rs` covers value and fault preservation.
- `tests/emath-ir/tests/capability_reference_vm.rs` covers capability execution and budget refusal.
- `tests/emath-ir/tests/semantic_images.rs` covers deterministic images and corruption.
- `tests/emath-ir/tests/static_specialization.rs` covers VM parity and mutation detection.
- `tests/emath-ir/tests/capability_cell_migration.rs` covers generic registration and dual-path parity.
- `tests/emath-ir/tests/semantic_tree_shaking.rs` covers reachability and dependency preservation.
- `tests/emath-ir/tests/lazy_image_loading.rs` covers profile loading and optional chunks.
- `tests/emath-ir/tests/linear_programming.rs` covers objectives, certificates, refusals, and Pareto behavior.

## No-claim boundaries

Only the declared executable vocabulary and registered capability cells run. Exact arithmetic, undeclared providers, ambient effects, and unsupported carriers refuse rather than falling back.
