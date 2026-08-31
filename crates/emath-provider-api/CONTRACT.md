# CONTRACT.md - emath-provider-api

## Purpose and layer

Tier 3 (goals and providers) adapter seam. Core dependency for
`emath-adapter-*`, `emath-registry`, and provider crates. Defines the
provider descriptor, capability, provider/adapter/checker trait seam plus
registry, filter, descriptor and constellation surfaces.

## Public types and semantics

- `ProviderDescriptor`: schema, id, version, implementation content id,
  goal kinds, semantic subsets, targets, maximum evidence level,
  determinism flag, permissions, checker bindings.
- `CapabilityTable::canonical` encodes isolation, lock, `maximum_evidence`,
  and `deterministic` (plus capability tokens) so evidence-ceiling /
  determinism drift changes identity the same way
  `emath-plan::ProviderFingerprint` does.
- `CapabilityReport` with `CapabilityReason` and `CostEstimate`: whether a
  goal is supported, reasons, and estimated cost.
- `ProviderResult`: untrusted provider transport (schema, goal identity,
  payload, optional certificate, evidence claims) until a checker admits it.
- `Provider` trait: `descriptor`, `supports`, `execute` (budgeted, cancelable).
- `Adapter<Source, Target>` trait: `encode`/`decode` over a declared relation.
- `ResultChecker` trait: `check` admits a `ProviderResult` into `Admitted`.
- `fork_adapter_contracts` is the neutral Dew/Rumoca/Wrenfold census.
  `pinned_fork_adapters` validates that every contract has an immutable
  40-hex source commit and non-empty license in `forks/UPSTREAM_LOCK.json`.
- `adapter` module (fjxh.17): GENERATED provider adapter contracts from
  admitted cell schemas (fjxh.2). `adapter_contract(schema)` derives the
  capability key `adapter:<cell>@<version>:<cell-id>` (identity from the
  cell's content id — identity-affecting schema mutation moves the key,
  `about` does not), the IR-facing `AdapterSpec` (arity, class, gated
  type tokens, numeric policy `strict-f64`), and the deterministic
  conformance fixtures (closed per-arity battery; a cell with a local
  oracle in `emath-ir` — today `std.tensor.softmax` — pins the oracle
  output on every fixture; oracle-less cells carry structural fixtures
  only). `compare_outputs(native, provider)` / `ConformanceFixture::compare`
  admit provider output BIT-FOR-BIT against the oracle (`ConformanceVerdict`:
  `Conformant` / `Diverged{index, native_bits, provider_bits}` / `ShapeMismatch`)
  — the provider is a checked worker, never the public meaning of an
  operation (the oracle stays in `emath-ir`). `ProviderBinding{capability,
  reduction_axis}` + `check_axis`: a wrong reduction axis FAILS typed
  (`E-PROVIDER-002`), never silently reinterpreted. IR purity gate
  (Neutral IR Constitution §7, same rule as `emath-epic-fm-0c8f.12`):
  `ir_type_gate`/`gate_signature` are an ALLOWLIST (`IR_OWNED_TYPES`:
  scalar/matrix/vector/tensor<f64>, bool) — provider-native types
  (torch/jax/ndarray, …) in the public IR-facing signature refuse typed
  (`E-PROVIDER-001`), enforced at contract generation. Refusals closed:
  `E-PROVIDER-001` NativeTypeInIr, `E-PROVIDER-002` AxisMismatch,
  `E-PROVIDER-003` NoLocalOracle (a provider is never its own oracle;
  handwrite a real kernel first). Adding an oracle or a fixture battery
  entry is one data entry — no IR enum grows, no provider is linked.
- Re-exports from `constellation`, `descriptor`, `filter`, `registry`
  modules add constellation, lock, filter and registry types (not exhaustive).

## Invariants

- Provider output is untrusted transport until a `ResultChecker` admits it
  (Constitution 8).
- `encode: E -> Result<P, AdapterRefusal>` and
  `decode: P -> Result<E', AdapterRefusal>` with a declared relation
  `R(E, E')` (Constitution 7).
- An adapter must refuse unsupported semantics before provider execution,
  never silently approximate.
- Stable IR references providers only by neutral ids. Provider-native Rust
  types and crate dependencies are confined to adapter crates.
- Phase 1 ships no concrete providers; this API is the frozen adapter seam.

## Error model

`ProviderError` carries a stable string `code` plus `message`. Sub-modules
emit typed errors under `E-PROV-501..525`: descriptor self-validation
(`E-PROV-501..503`), registry (`E-PROV-510/511/518`), filter
(`E-PROV-512..516`), ledger/constellation (`E-PROV-521..523/524/525`).

## Determinism class

Structural types are deterministic (derive `PartialEq, Eq`, no interior
state). Execution determinism is a declared per-provider property
(`ProviderDescriptor.deterministic`), not enforced or proven by this crate.

## Cancellation behavior

`Provider::execute` takes `&dyn Cancellation` (from `emath-runtime`) and
returns `Outcome`; the crate itself is std-only and synchronous with no own
cancellation surface. Cancellation is passed through to the provider.

## Unsafe boundary

None. The crate sets `#![forbid(unsafe_code)]`.

## Feature flags

None (`Cargo.toml` has no `[features]`).

## Conformance tests

No `crates/emath-provider-api/tests/` directory and no inline `#[cfg(test)]`
modules in `src/`. Conformance lives in the standalone `tests/emath-provider-api`
Conformance lives in the standalone `tests/emath-provider-api`
package: `tests/filter.rs` covers goal-filtering verdicts, including
`undeclared_exactness_provider_excluded_for_any_explicit_goal`).
`undeclared_exactness_provider_excluded_for_any_explicit_goal`);
`tests/fork_constellation_unit.rs` checks the fork census, source/license
locks, and the stable-IR no-leak boundary.
## No-claim boundaries

Phase 1 ships no concrete providers, so no provider semantics are executed
by this crate. Whether a provider's output is correct is out of scope for
the trait seam and must be established by the provider's `ResultChecker`.

## Absorbed module: `runtime` (was `emath-runtime`)

  # CONTRACT.md
  
  ## Purpose and layer
  
  Runtime outcome model: budgets, cancellation, evidence handles, continuations and explicit `Outcome::Unresolved`. Layer: `core` (per CRATE_MAP.md).
  
  ## Public types and semantics
  
  - `Budget { evaluations, iterations, work_units, memory_bytes, output_bytes }`: resource budget with `Default` constants.
  - `EvidenceHandle { schema, identity }`: content-addressed reference to the evidence backing an outcome.
  - `ContinuationHandle { schema, identity, provider_id }`: reference to a deferred provider continuation.
  - `UnresolvedReason`: `MissingProvider`, `UnsupportedSemanticSubset`, `BudgetExhausted`, `InconclusiveEvidence`, `TargetUnavailable`, `PermissionDenied`; each has a stable `as_str`.
  - `Outcome<T, E>`: provider execution outcome; `Resolved { value, evidence }`, `Unresolved { reason, partial, continuation, evidence }`, `Failed(E)`; `is_resolved()`.
  - `Cancellation` trait with `is_cancelled()`, and `NeverCancel` marker that always reports false.
  
  ## Invariants
  
  - Only `Outcome::Resolved` carries admitted value authority.
  - `Outcome::Unresolved` is an explicit, typed disposition carrying reason, optional partial value, continuation and evidence; it is never conflated with success or failure.
  - Out-of-budget/inconclusive/target-unavailable states surface through `UnresolvedReason`, not silent truncation.
  
  ## Error model
  
  No dedicated error type. Failures are carried in `Outcome::Failed(E)` where `E` is the caller's error type; no stable codes emitted by this crate.
  
  ## Determinism class
  
  Deterministic. Outcome variants and budget defaults are pure data; no RNG or wall-clock input to the outcome model itself.
  
  ## Cancellation behavior
  
  This crate is where cancellation surface exists. `Cancellation::is_cancelled()` is the cooperative query seams providers consult; `NeverCancel` is the explicit "never cancels" marker. This is a query-only surface; the crate does not force aborts.
  
  ## Unsafe boundary
  
  None. `#![forbid(unsafe_code)]` at crate root; workspace lint forbids unsafe_code.
  
  ## Feature flags
  
  None. Cargo.toml has no `[features]`.
  
  ## Conformance tests
  
  None on disk currently. No `tests/` directory and no inline `#[cfg(test)]` module in `lib.rs`.
  
  ## No-claim boundaries
  
  - `Budget::default()` values are defaults, not limits enforced by this crate.
  - Cancellation is cooperative and query-only (`is_cancelled`); this crate provides no forced-abort mechanism.

## Absorbed module: `plugin_sdk` (was `emath-plugin-sdk`)

# CONTRACT.md - emath-plugin-sdk

## Purpose and layer

Tier 7 (governance and operations) plugin SDK slice: descriptors, sandbox
policy decisions, and a deterministic test-harness contract. Std-only, no
network, no component host. Depends on `emath-core` (FNV-1a64 content id).

## Public types and semantics

- `PluginDescriptor` (schema `emath.plugin`): id, kind, interface core,
  declared capabilities, sandbox policy. Canonical JSON rendering
  (`canonical_json`) and FNV-1a64 content id (`content_id`).
- `SandboxPolicy`: fuel (`None` = unmetered), granted permissions, network
  flag, allowed capabilities.
- `Trust`: `Local` (locally audited) vs `Untrusted` (third-party).
- `PluginOutput` (`Vec<u8>`): the runtime result contract.
- `PluginError`: typed error with stable `E-PLG-0xx` code.
- Free fns: `admit` (sandbox/fuel/permission gate), `execute` (harness
  entry), `compatible` (interface-core compatibility), `descriptor_for`.
- Constants `PLUGIN_SCHEMA`, `INTERFACE_CORE`.

## Invariants

- Plugin ids must be non-empty and free of ASCII control characters
  (breaks log/diagnostic framing and content-id ambiguity), refused with
  `E-PLG-005` before any sandbox check.
- Every declared capability must be inside `allowed_capabilities`
  (`E-PLG-003`); an empty declared capability set is refused
  (`E-PLG-003`).
- A capability touching a resource class requires the matching granted
  permission; `network` requires the `network` permission (`E-PLG-002`).
- Untrusted descriptors must declare positive fuel (`E-PLG-002`).
- `execute` re-enforces positive fuel under every trust class before
  `E-PLG-001`, so `Trust::Local` can never admit an unmetered plugin onto an
  execution path.
- Phase 1 has no component runtime; `execute` is always a typed refusal
  (`E-PLG-001`).
- `canonical_json` is byte-stable; `content_id` is the shared FNV-1a64
  convention.

## Error model

`PluginError` with stable codes: `E-PLG-001` (component runtime absent),
`E-PLG-002` (sandbox/fuel/permission violation), `E-PLG-003` (capability
outside the allowed set or none declared), `E-PLG-004` (interface-core
mismatch), `E-PLG-005` (empty or ASCII-control-bearing plugin id).

## Determinism class

Admission/refusal decisions, `canonical_json`, and `content_id` are
deterministic; `execute` deterministically refuses every Phase 1 call.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. The crate sets `#![forbid(unsafe_code)]`.

## Feature flags

None (`Cargo.toml` has no `[features]`).

## Conformance tests

None present. No `tests/` directory on disk and no inline `#[cfg(test)]`
module in `src/lib.rs`.

## No-claim boundaries

Plugin execution is not implemented in Phase 1 (component runtime absent);
the `execute` call shape (`descriptor, input -> output`) is the stable
surface the Phase 2+ runtime must fill. A declared permission is only as
good as the gate that enforces it; no runtime verifies a plugin actually
holds the resources it declares.
