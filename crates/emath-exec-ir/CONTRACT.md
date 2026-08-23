# emath-exec-ir CONTRACT.md

## Purpose and layer

- Tier 2 (semantics) per `implementation/CRATE_MAP.md`.
- Executable Mathematics IR (EMIR): typed, target-independent operations.
- Phase 1 lowers the strict-f64 subset to a linear op list per output definition.
- Depends on `emath-core` (spans) and `emath-ir` (expr nodes, literals, operators, packages).

## Public types and semantics

- `EmirValue` — typed SSA value reference in the op list (wrapped `u32`).
- `EmirOp` — one EMIR instruction (`ConstF64`, `LoadInput`, `LoadState`, arithmetic, elementary functions (exp, ln, log2, log10, sqrt, sin, cos, tan, tanh, sinh, cosh, abs, floor, ceil, sign, cbrt, recip, fract, atan), comparison ops, `Select`, `IsFinite`, `Fold` runtime fold, `Integral` numerical integration, `Differentiate` forward-mode autodiff with nested sub-program, `Solve` Newton's-method root-finding, `Optimize` multi-variable gradient-descent optimization, `Mod` remainder, `Hypot`, `Min`/`Max`, `Atan2`); exposes a stable `name()`.
- `FoldCombine` — accumulation strategy for `Fold` (`Add` / `Mul` / `And` / `Or`).
- `DomainObligation` — domain obligation recorded during lowering (`DivisionNonZero`, `SqrtNonNegative`, `LogPositive`, `PowFiniteResult`) with `as_str`.
- `EmirProgram` — one lowered definition: linear op list, result value, input/state counts, obligations; `print` renders it deterministically.
- `EmirExprRef` — alias for `emath_ir::ExprId`.
- Functions: `lower_requirement` (constructor precondition) and `lower_definition` (definition expression).
- `interp`: `Value` (`F64`/`Bool`/`Vector`/`Matrix`/`Tensor`), `EvalFault` (including `IndexOutOfBounds`), `evaluate(program, inputs: &[Value], state: &[Value])`, and `evaluate_f64` for scalar slices. Single forward pass, typed registers, no panics. Out-of-range index is a fault, not NaN.
- `runner`: `run_package` / `run_package_with_given` / `run_declaration` — constructor requires → Self state → definitions → example given/expect verdicts (`RunReport`). A declaration with no inputs evaluates definitions against an empty `given`. Definitions may reference earlier definition names in source order (let-binding semantics, recovered from expression spans; matches admission). Later definitions and `expect` expressions receive earlier values as-is (`Vector` / `Matrix` / `Tensor` are not flattened to `f64`). `TestVerdict::Computed` is a worked example (`expect` omitted): values are recorded, no pass/fail claim. A declaration with no tests still emits a synthetic `_pane` worked run when every input is bound; `run_package_with_given` adds that `_pane` entry (or a typed `missing input \`name\`` refusal) in addition to source examples. `RunSummary` counts `{tests, passed, failed, refused, computed}`.
- Continuous models: `step_continuous` (scalar), `step_continuous_values` (scalar/vector/matrix/tensor state), `simulate_continuous` (fixed `dt`), `simulate_continuous_with` (`SimulateOptions` for `--atol/--rtol/--dt-max` and one scalar event), `StepMethod::{Euler, Rk4, Rk45}`. Default stays fixed-step. Adaptive RK45 compares Cash-Karp 4th vs 5th and is a typed refusal on non-positive tolerances or a collapsed dt. Causalized Newton (`causal_newton`) refuses missing declaration inputs (no silent `0.0`); only `__rate_*` unknowns start at zero. Interpreter `Solve` / `Optimize` refuse vanished derivatives and `max_iter` exhaustion (`EvalFault::Arithmetic`) rather than returning a non-root / non-stationary point.

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
- Interpretation is bit-exact IEEE-754 binary64 for arithmetic/comparisons/`min`/`max`/`abs`/`floor`/`ceil`/`is_finite`/boolean ops. Transcendentals follow platform libm (same class as generated Rust / Tier 1).

## Cancellation behavior

- Not applicable; std-only synchronous crate, no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids `unsafe_code`.

## Feature flags

- None.

## Conformance tests

- Integration tests in `tests/emath-exec-ir`: `call_with_wrong_arity_is_refused`, `oversized_integer_literal_is_refused`.
- Interpreter/runner unit tests in `src/interp.rs` and `src/runner.rs`: op spot checks (add/pow/select/is_finite/div-by-zero/eq-NaN/type-confusion) and programmatic Square / constructor-refused / expect-less worked-example packages.

## No-claim boundaries

- Admits only the strict-f64 subset (item kinds that lower to finite f64 ops); exact arithmetic and unlisted function names are refused.
- Domain obligations are assumptions, not runtime checks. The interpreter does not compile or invoke cargo; it is not a substitute for the Tier-1 generated crate on transcendental bit-identity across libm implementations.
