# emath-sema CONTRACT.md

## Purpose and layer

- Tier 2 (semantics) per `implementation/CRATE_MAP.md`.
- Semantic admission (Phase 1): syntax tree to typed neutral SIR.
- Orchestrates field checks, constructor/invariant admission, definition typing, goal elaboration, and plan construction through the public `CompilerSession` surface.
- Everything outside the Phase 1 subset receives a typed capability refusal; nothing is silently dropped.
- May depend on core/ir/syntax/goal; declares only `emath-core` and `emath-ir` (plus `emath-syntax` as a dev-dependency for the parser backend).

## Public types and semantics

- `CompilerSession` — the session facade: `new`, `load_package`, `load_text`, `parse_text`, `check`, `check_owned`, `plan`. Carries a `SourceStore` and `Limits`.
- `SourcePackage` — a loaded source: file id, display name, text.
- `CheckResult` — admitted `SemanticPackage` plus `Diagnostics` and a `SemanticTrace`.
- `PlanResult` — admitted package, elaborated `RequestSpec`s, `ResolutionPlan`s, and `Diagnostics`.
- `CompilerPolicy` — build policy knobs (e.g. `verify_generated_crate`).
- `GeneratedCrate` — build-step result produced by `emath-build`: crate/package/version, file map, `EmittedAnchor` source-map anchors.
- `EmittedAnchor`, `SemanticTrace`, `TraceEntry` — anchor and trace bookkeeping (not exhaustive).

## Invariants

- Goals attach to declarations by construction (the ids built for that declaration), never by span geometry.
- Missing or unloaded source is a typed refusal (`E-PKG-080`), never an empty-source plan that passes silently.
- Declarations distinguishable only by lookalike glyphs are refused (`E-NAME-024`); duplicate names (`E-NAME-022`) and `_` names (`E-NAME-023`) are refused.
- Session limits reach the lexer through the installed parser backend (`E-SYN-108`).
- Request targets must be outputs or definitions of the declaration (`E-GOAL-041`); produce targets outside `rust.library` are refused (`E-GOAL-042`); request kinds other than `evaluate` are refused (`E-GOAL-043`).
- Kind schema is the required/optional source of truth: omitted `inputs:` is admitted (`AtMostOne`; a constant-only declaration) and omitted `outputs:` is admitted (`AtMostOne`, default `definitions`) and those definitions are lifted onto the output surface and evaluated. Untyped `inputs:` names default to `Float64` and emit note `N-TYPE-001`. Later `definitions:` may reference earlier definition names. Missing `ExactlyOne` sections refuse with `E-KIND-011`.
- Stateless `emath function name(args) -> T:` head-args lower into the same Field IR as an `inputs:` section. `-> T` declares one output named after the declaration (so `square = x * x` satisfies the binding). Untyped head-args default to `Float64` with `N-TYPE-001`. Mixing head-args with `inputs:` or `-> T` with `outputs:` is `E-SYN-122`. Head-args on a stateful or non-function declaration are `E-SYN-123`.
- Live `request:` / `requests:` sections refuse with `E-SEC-101` and a `goals:` migration hint.
- Numeric model selection: omitted `numeric:` defaults to `strict-f64`.
  `numeric interval-f64` is an explicit alternate. Unknown models refuse
  with `E-NUM-001`. `precision` / `error-limit` demands the selected model
  cannot honor refuse with `E-NUM-002` / `E-NUM-003`. `representation Real`
  without a named model is `E-NUM-004` (no silent `Real` → `f64` map).
- Known unit types (`Duration`, `MiB`, `Per<Duration>`, …) and quantity
  literals (`1 s`) admit; unknown units are `E-UNIT-104`, ill-formed
  `Per<>` is `E-UNIT-105`, dimensional mismatch is `E-UNIT-101`.
- Declared `Vector`/`Matrix`/`Tensor` shapes and compile `domain lo..hi`
  are checked: empty/zero extents are `E-SHAPE-004`, inverted intervals
  are `E-DOM-002`. Rank-3+ literals, `:` slices, and equal-or-`Fixed(1)`
  tensor broadcast reuse `E-SHAPE-005` / `E-SHAPE-006`. `Nat`/`Int`
  subscripts admit; a negative constant index is `E-SHAPE-006`.
- Finite `sum` / `product` over a known integer range (`sum i in 1..6: i`,
  inclusive `product i in 1..=5: i`) or a known-shape array
  (`sum([1, 2, 3])`, `product(m)`) unroll to ordinary arithmetic. `mean(v)`
  is `sum(v) / length(v)`; `abs(v)` maps elementwise over a known-size
  vector. Variable-bound ranges (`sum i in 0..n: v[i]`) lower to a runtime
  `Fold` op (EMIR sub-program evaluated per iteration). `forall`
  and `exists` also lower to `Fold` with `And` / `Or` combine.
  `derivative(expr) wrt var` lowers to a `Differentiate` op: the value
  expression is inlined (definition references resolved) and emitted as a
  nested EMIR sub-program. The interpreter evaluates it via dual-number
  forward-mode autodiff (each op carries its own derivative rule). The
  variable must be a scalar input.
  `solve(residual) wrt var` lowers to a `Solve` op: Newton's method
  iteratively adjusts `var` until the residual is within tolerance of
  zero. Each step uses dual-number evaluation for both the residual and
  its derivative. `minimize(objective) wrt var` and `maximize(objective)
  wrt var` lower to `Optimize` ops: gradient descent (or ascent) with a
  fixed learning rate. Both reuse the autodiff machinery for gradients.
  `integral` lowers to a dedicated `Integral` op (composite Simpson's
  rule, 1000 steps) for continuous-range numerical integration.
- Model equations admit explicit `derivative(state) = rhs` and a recorded
  scalar mass-matrix rewrite `m * der(state) = rhs` when `m` is a named
  scalar input/parameter/definition. Leftover implicit residuals stay
  `E-TYPE-010`. A quantity state requires a matching state/time rate
  (`E-UNIT-101`); unitless A2 models stay unitless.

## Error model

- Emits stable `E-*` diagnostics through `emath_core::Diagnostics`: `E-PKG-080`, `E-SYN-101`, `E-SYN-108`, `E-SYN-120`, `E-SYN-122`, `E-SYN-123`, `E-GOAL-041/042/043`, `E-NAME-022/023/024`, `E-KIND-011`, `E-SEC-101`, `E-NUM-001/002/003/004`, `E-UNIT-101/104/105`, `E-SHAPE-004`, `E-DOM-002`.
- `E-SYN-120` is a typed refusal when the parser backend is not installed; hosts call `emath_syntax::install_source_parser` once per process.

## Determinism class

- `plan` builds deterministic native resolution plans; candidate ordering and tie-breaks follow seeded, ordered rules.
- `parse` and `check` are deterministic given the same source and limits.

## Cancellation behavior

- Not applicable; std-only synchronous crate, no cancellation surface.

## Unsafe boundary

- None; workspace lint forbids `unsafe_code`.

## Feature flags

- None.

## Conformance tests

- No `crates/emath-sema/tests/` directory on disk; conformance is unit-level in `session.rs` and `recognition.rs` `#[cfg(test)]` modules plus `tests/emath-sema`: existing session tests and `tests/numeric.rs` (default `strict-f64`, explicit `interval-f64`, `E-NUM-*` / `E-UNIT-*` / `E-SHAPE-004` / `E-DOM-002` refusals, units e2e).

## No-claim boundaries

- Phase 1 admits the default `strict-f64` numeric model and an explicit
  `interval-f64` alternate; those models are computation descriptors, never
  claims about real-number semantics. Exact real arithmetic, arbitrary
  produce targets, and non-evaluate request kinds are refused.
- The build step (backend plus artifact emission) lives in `emath-build`, not here.
