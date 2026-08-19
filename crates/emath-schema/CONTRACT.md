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
  canonical schema registry access. Each `$id` emits its own JSON Schema
  document (not a shared stub). Closed-world ids encode the in-tree
  emitter's top-level `properties` / `required`; ids with no JSON
  emitter are an open envelope (`schema` const, `additionalProperties:
  true`).
- `SchemaError`, `UnknownSchemaError`: registry error types.
- Version constants: `VERSION`, `REGISTRY_VERSION`, `SCHEMAS_VERSION`,
  `SCHEMA_VERSION`, `SCHEMA_NAMES`.
- Modules: `lang`, `load`, `lower`, `registry`.

## Invariants

- Invalid lowering cannot publish HIR; every application keeps an expansion
  trace.
- Schema language sections follow required/optional/repeatable policies with
  payload policies, defaults and predicates.
- Kind package loading fails on missing kinds, checksum mismatch, incompatible
  schema versions and recursive expansion, bounded by `MAX_EXPANSION_DEPTH`
  and `MAX_LOWER_OPS`.
- Unknown schema names are typed refusals (`E-SCHEMA-001`).
- Closed-world registry ids (`emath.source-artifact`,
  `emath.parse-forest`, `emath.answer-receipt`,
  `emath.interpretation-portfolio`) set `additionalProperties: false`
  and list the emitter's field names and JSON types. Optional
  `$schema` is admitted on instances so examples can round-trip `$id`.
- The other nine ids have no in-tree JSON document with that `$id`;
  their schemas are envelope-only and must not invent fields.

## Error model

Typed issue enums `SchemaIssue`, `ResolveIssue`, `LoweringIssue` plus
`SchemaError` / `UnknownSchemaError`. Diagnostics also flow through
`emath_core::Diagnostics`.

## Determinism class

Deterministic JSON schema and example writers; stable version constants for
the registry. Closed-world documents are distinct from each other and from
the envelope template even after `$id` / title / description are ignored.

## Cancellation behavior

Not applicable. Std-only synchronous crate, no cancellation surface.

## Unsafe boundary

None. Crate root is `#![forbid(unsafe_code)]` (registry comments note the root
forbids unsafe).

## Feature flags

None.

## Conformance tests

No `tests/` directory present. Inline `#[cfg(test)]` unit tests live in the
`src` modules. Registry tests cover thirteen-name order, pairwise-distinct
documents, emitter field names for source-artifact / parse-forest /
answer-receipt, envelope no-invention, `$id` example round-trip,
determinism, and `E-SCHEMA-001`.

## No-claim boundaries

Registry and lowering cover the thirteen canonical schema ids only;
arbitrary user-defined kinds beyond these are not certified.

No JSON emitter in this tree for: `emath.symbol-signature` (genesis writes
`emath.signature`), `emath.term-ir` (durable document is `emath.free-term`;
`TERM_IR_SCHEMA` versions the canonical text encoding), `emath.world-ir`
and `emath.world-morphism` (canonical text forms, not JSON documents),
`emath.meaning-lock`, `emath.agent-world-proposal` (genesis writes
`emath.world-candidate`), `emath.continuation` (Rust `ContinuationHandle`,
not a JSON document), `emath.math-layout-graph`,
`emath.provenance-receipt` (13th registry id; the bead's "plus examples"
are the per-id `example_json` writers, not a 13th artifact name). These
nine are disclosed envelope-only.
