# emath-exec-ir CONTRACT.md

## Purpose and layer

- Tier 2 (semantics) per `implementation/CRATE_MAP.md`.
- Executable Mathematics IR (EMIR): typed, target-independent operations.
- Phase 1 lowers the strict-f64 subset to a linear op list per output definition.
- Depends on `emath-core` (spans) and `emath-ir` (expr nodes, literals, operators, packages).

## Public types and semantics

- `EmirValue` — typed SSA value reference in the op list (wrapped `u32`).
- `EmirOp` — one EMIR instruction (`ConstF64`, `LoadInput`, `LoadState`, arithmetic, elementary and comparison ops, `Select`, `IsFinite`); exposes a stable `name()`.
- `DomainObligation` — domain obligation recorded during lowering (`DivisionNonZero`, `SqrtNonNegative`, `LogPositive`, `PowFiniteResult`) with `as_str`.
- `EmirProgram` — one lowered definition: linear op list, result value, input/state counts, obligations; `print` renders it deterministically.
- `EmirExprRef` — alias for `emath_ir::ExprId`.
- Functions: `lower_requirement` (constructor precondition) and `lower_definition` (definition expression).

## Invariants

- Lowering is linear and ordered; result is the final produced value.
- Domain obligations are recorded explicitly and emitted as assumptions in Phase 1, never silently erased.
- Strict-f64 policy refuses non-finite constants and literals outside the finite range.
- Function-call arity is enforced in every build, debug or release.
- Exact arithmetic (`ExactAdd`/`ExactSub`/`ExactMul`/`ExactDiv`) is outside the Phase 1 subset and refused.
- Count indexes saturate to `u16::MAX`/`u32::MAX` on overflow instead of panicking.

## Error model

- Lowering returns `Result<EmirProgram, String>` with plain string messages (unknown input/state, unknown function, non-finite constant, unsupported literal or expression form).
- No stable error codes; no `emath_core::Diagnostics`.

## Determinism class

- Lowering produces a deterministic linear op list for a given package and inputs; `EmirProgram::print` output is byte-deterministic.

## Cancellation behavior

- Not applicable; std-only synchronous crate, no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids `unsafe_code`.

## Feature flags

- None.

## Conformance tests

- No `crates/emath-exec-ir/tests/` directory on disk; conformance is unit-level in the `#[cfg(test)]` module: `call_with_wrong_arity_is_refused`, `oversized_integer_literal_is_refused`.

## No-claim boundaries

- Admits only the strict-f64 subset (item kinds that lower to finite f64 ops); exact arithmetic and unlisted function names are refused.
- No type/rounding refinement, no stateful evaluation; this crate only produces op lists, it does not execute them.
