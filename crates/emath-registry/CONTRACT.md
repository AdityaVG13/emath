# CONTRACT.md - emath-registry

## Purpose and layer

Tier 5 (integration) package/provider registry slice: a std-only,
deterministic index + lock model providing reproducible snapshot and lock
documents, version resolution, and typed compatibility diagnostics. Depends
on `emath-core` (shared FNV-1a64). Registry services are not implemented.

## Public types and semantics

- `IndexSnapshot`: package name -> version -> `PackageVersion` records;
  `canonical_json` renders a byte-stable document and `snapshot_id` is its
  FNV-1a64 fingerprint.
- `PackageVersion`: version, content id, source location, kind schemas,
  provider descriptors, yanked/revoked state, license, security notes,
  evidence summary, artifact link.
- `RegistryLock`: schema, snapshot fingerprint, package -> pinned version;
  `verify` enforces reproducible offline reproduction.
- `Constraint`: `Any`, `Exact(String)`, `Major(u64)` version selection.
- `RegistryError`: typed error with stable `E-REG-0xx` code.
- Fns `check_kind_schema`, `check_provider_capability`: typed compatibility
  diagnostics. Constants `INDEX_SCHEMA`, `LOCK_SCHEMA`.

## Invariants

- `canonical_json` is stable (sorted keys throughout); `snapshot_id` is the
  FNV-1a64 fingerprint of that document.
- Resolution refuses yanked and revoked pins.
- `RegistryLock::verify` requires a matching snapshot fingerprint and that
  every pin resolves (the "lock reproduces offline" gate).
- Version comparison is lexicographic (documented limitation).

## Error model

`RegistryError` with stable codes: `E-REG-020` (unknown package),
`E-REG-021` (lock/snapshot mismatch or non-resolving pin), `E-REG-022`
(yanked pin), `E-REG-023` (revoked pin), `E-REG-024` (no usable version),
`E-REG-030` (kind schema not served), `E-REG-031` (provider capability not
served).

## Determinism class

`canonical_json`, `snapshot_id`, and `resolve` are deterministic (BTreeMap
ordering, lexicographic version max under `Any`/`Major`).

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. The crate sets `#![forbid(unsafe_code)]`.

## Feature flags

None (`Cargo.toml` has no `[features]`).

## Conformance tests

None present. No `tests/` directory on disk and no inline `#[cfg(test)]`
module in `src/lib.rs` (the module doc describes tests on both sides of
every refusal, but no test module is currently in the source).

## No-claim boundaries

Registry services (fetch, publish, hosting) are not implemented; this
crate is the reproducible index + lock core only. Version ordering is
lexicographic, not semantic.
