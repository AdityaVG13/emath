# emath-schema

## Purpose and layer

Tier 1 custom kind schemas and restricted lowering. Defines a schema
language, bounded lowering into core HIR, the thirteen canonical schema
registry, and kind package loading. Output is the shared
`emath_ir::KindSchema` that compiler and builder both admit against.

## Public types and semantics

- `parse_schema_language`, `SchemaIssue`: schema language parsing.
- `resolve_kind`, `KindPackage`, `ResolveIssue`, `VersionPolicy`,
  `ExpandTrace`, `MAX_EXPANSION_DEPTH`: kind package loading and resolution.
- `apply_lowering`, `validate_lowered`, `is_bound`, `LowerOp`,
  `LoweringIssue`, `MAX_LOWER_OPS`: restricted lowering into core HIR.
- `is_known_schema`, `schema_names`, `all_schema_names`, `schema_json`,
  `example_json`, `write_schema_json`, `write_example_json`: thirteen
  canonical schema registry access.
- `SchemaError`, `UnknownSchemaError`: registry error types.
- Version constants: `VERSION`, `REGISTRY_VERSION`, `SCHEMAS_VERSION`,
  `SCHEMA_VERSION`, `SCHEMA_SPEC_VERSION`, `SCHEMA_NAMES`.
- Modules: `lang`, `load`, `lower`, `registry`.

## Invariants

- Invalid lowering cannot publish HIR; every application keeps an expansion
  trace.
- Schema language sections follow required/optional/repeatable policies with
  payload policies, defaults and predicates.
- Kind package loading fails on missing kinds, checksum mismatch, incompatible
  schema versions and recursive expansion, bounded by `MAX_EXPANSION_DEPTH`
  and `MAX_LOWER_OPS`.
- Unknown schema names are typed refusals.

## Error model

Typed issue enums `SchemaIssue`, `ResolveIssue`, `LoweringIssue` plus
`SchemaError` / `UnknownSchemaError`. Diagnostics also flow through
`emath_core::Diagnostics`.

## Determinism class

Deterministic JSON schema and example writers; stable version constants for
the registry.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. Crate root is `#![forbid(unsafe_code)]` (registry comments note the root
forbids unsafe).

## Feature flags

None.

## Conformance tests

No `tests/` directory present. Inline `#[cfg(test)]` unit tests live in the
`src` modules and validate the registry complete set (not enumerated).

## No-claim boundaries

Registry and lowering cover the five canonical/known schema set (thirteen
canonical schemas) only; arbitrary user-defined kinds beyond these are not
certified.
